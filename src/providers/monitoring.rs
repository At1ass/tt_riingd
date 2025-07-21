use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use futures::{StreamExt, stream::iter};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::{sync::RwLock, time::interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::buffer::speed::{SpeedBuffer, SpeedSnapshot};
use crate::buffer::{Buffer, BufferStrategy, Layout};
use crate::drivers::registry::ControllerSpec;
use crate::{
    ConfigManager,
    config::{CurveCfg, CurveMapping, Mapping},
    core::{
        AppState,
        event::{Event, MessageBroker, RequestPayload, Response, ServiceType},
    },
    drivers::commands::{BatchCommand, ExecutionMode},
    mappings::FanRef,
    providers::traits::ServiceProvider,
    task_manager::TaskManager,
};

/// Temperature monitoring service provider.
///
/// Provides a critical service that continuously monitors temperature sensors
/// and updates fan speeds based on configured curves and mappings. This is
/// the core service responsible for automatic fan control.
///
/// # Priority and Criticality
///
/// - **Priority**: 10 (highest)
/// - **Critical**: Yes (system cannot function without it)
///
/// # Features
///
/// - Periodic temperature sensor reading
/// - Automatic fan speed adjustment based on curves
/// - Temperature event publishing for other services
/// - Sensor failure handling and logging
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::MonitoringServiceProvider;
/// use tt_riingd::core::event::MessageBroker;
/// use tt_riingd::core::AppState;
/// use tt_riingd::config::{Config, ConfigManager};
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = MessageBroker::new();
/// let config = Config::default();
/// let config_manager = ConfigManager::load(None).await?;
/// let provider = MonitoringServiceProvider::new(state, event_bus, &config_manager).await;
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
pub(crate) struct MappingCache {
    /// Cache for mapping data.
    pub mappings: Mapping,
    pub curves: Vec<CurveCfg>,
    pub active_curves: CurveMapping,
}

impl MappingCache {
    /// Creates a new empty mapping cache.
    pub fn new() -> Self {
        Self {
            mappings: Mapping::default(),
            curves: Vec::new(),
            active_curves: CurveMapping::default(),
        }
    }
}

struct MonitoringCache {
    /// Cache for temperature data.
    pub sensor_data: RwLock<HashMap<String, f32>>,
    pub mapping_cache: ArcSwap<MappingCache>,
    pub new_mapping: ArcSwap<MappingCache>,
}

pub struct MonitoringServiceProvider {
    state: Arc<AppState>,
    event_bus: MessageBroker,
    cache: Arc<MonitoringCache>,
}

impl MonitoringServiceProvider {
    /// Creates a new monitoring service provider.
    pub async fn new(
        state: Arc<AppState>,
        event_bus: MessageBroker,
        config: &ConfigManager,
    ) -> Self {
        Self {
            state,
            event_bus,
            cache: Arc::new(MonitoringCache {
                sensor_data: RwLock::new(HashMap::new()),
                mapping_cache: ArcSwap::from_pointee(MappingCache {
                    mappings: Mapping::load_mappings(&config.get().await.mappings),
                    curves: config.get().await.curves.clone(),
                    active_curves: CurveMapping::load_mappings(
                        &config.get().await.active_curve_mappings,
                    ),
                }),
                new_mapping: ArcSwap::new(Arc::new(MappingCache::new())),
            }),
        }
    }
}

pub struct FanOffset {
    offset: usize,
}

pub struct SensorEntry {
    pub id: String,
    pub fans: Vec<FanOffset>,
}

impl SensorEntry {
    pub fn set_fans_speed(&self, buffer: &mut [u8], speed: u8) {
        for fan in &self.fans {
            buffer[fan.offset] = speed;
        }
    }
}

pub struct CurveEntry {
    pub id: String,
    pub curve_cfg_idx: usize,
    pub sensors_entry: Vec<SensorEntry>,
}

pub struct OptimizedMonitoringBuffer {
    pub buffer: SpeedBuffer,
    pub curve_entry: Vec<CurveEntry>,
}

impl OptimizedMonitoringBuffer {
    fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            curve_entry: Vec::new(),
        }
    }

    fn calculate_buffer_offset(fan_ref: &FanRef, layout: &Layout) -> usize {
        let controller = layout
            .controllers
            .iter()
            .find(|c| c.controller_id == fan_ref.controller_id)
            .expect("Controller not found in layout");

        controller.offset + fan_ref.channel - 1
    }

    fn build_curve_entry(cache: &MappingCache, layout: &Layout) -> Vec<CurveEntry> {
        cache
            .active_curves
            .get_curve2fans()
            .iter()
            .filter_map(|curve| {
                let curve_name = curve.key();
                let fans = curve.value();

                let curve_cfg_idx = cache
                    .curves
                    .iter()
                    .position(|c| c.get_id() == *curve_name)?;

                let sensors_entry = fans
                    .iter()
                    .filter_map(|fan_ref| {
                        cache
                            .mappings
                            .get_sensor_for_fan(fan_ref.key())
                            .map(|sensor_entry| (fan_ref.clone(), sensor_entry.clone()))
                    })
                    .fold(
                        std::collections::HashMap::new(),
                        |mut acc: std::collections::HashMap<String, Vec<_>>,
                         (fan_ref, sensor_key)| {
                            acc.entry(sensor_key).or_default().push(fan_ref);
                            acc
                        },
                    )
                    .into_iter()
                    .map(|(sensor_key, fan_refs)| {
                        let fan_offsets = fan_refs
                            .into_iter()
                            .map(|fan_ref| {
                                let offset = Self::calculate_buffer_offset(&fan_ref, layout);
                                FanOffset { offset }
                            })
                            .collect();

                        SensorEntry {
                            id: sensor_key,
                            fans: fan_offsets,
                        }
                    })
                    .collect::<Vec<_>>();

                Some(CurveEntry {
                    id: curve_name.clone(),
                    curve_cfg_idx,
                    sensors_entry,
                })
            })
            .collect::<Vec<_>>()
    }

    /// Creates optimized monitoring buffer from cache, similar to OptimizedBuffer::with_specs
    pub(crate) fn from_cache(
        cache: &MappingCache,
        spec: &[ControllerSpec],
    ) -> anyhow::Result<Self> {
        let buffer = Buffer::from_cache(spec);
        let curve_entry = Self::build_curve_entry(cache, &buffer.layout);

        Ok(Self {
            buffer,
            curve_entry,
        })
    }

    pub fn take_buffer_snapshot(&mut self) -> SpeedSnapshot {
        self.buffer.try_take_snapshot()
    }

    pub fn restore_snapshot(&mut self, snapshot: SpeedSnapshot) -> Result<()> {
        self.buffer.try_restore_snapshot(snapshot)
    }
}

pub struct MonitoringBuffer {
    /// Buffer for batch data to be processed.
    pub batch_data: OptimizedMonitoringBuffer,
    pub temp_data: HashMap<String, f32>,
}

impl Default for MonitoringBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitoringBuffer {
    /// Creates a new monitoring buffer.
    pub fn new() -> Self {
        Self {
            batch_data: OptimizedMonitoringBuffer::new(),
            temp_data: HashMap::new(),
        }
    }

    pub(crate) fn with_specs(
        cache: &MappingCache,
        spec: &[ControllerSpec],
    ) -> anyhow::Result<Self> {
        let batch_data = OptimizedMonitoringBuffer::from_cache(cache, spec)?;
        Ok(Self {
            batch_data,
            temp_data: HashMap::new(),
        })
    }

    pub fn clear(&mut self) {
        self.temp_data.clear();
    }
}

#[async_trait]
impl ServiceProvider for MonitoringServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();
        let cache = self.cache.clone();

        let spec = state.controllers.read().await.get_controllers_spec();
        let buffer = Arc::new(RwLock::new(MonitoringBuffer::with_specs(
            &cache.mapping_cache.load(),
            &spec,
        )?));

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                run_monitoring_service(state, event_bus, buffer, cancel_token, cache.clone()).await
            })
            .await
    }

    fn name(&self) -> &'static str {
        "MonitoringService"
    }

    fn priority(&self) -> i32 {
        10
    }

    fn is_critical(&self) -> bool {
        true
    }
}

async fn run_monitoring_service(
    state: Arc<AppState>,
    event_bus: MessageBroker,
    batch_data: Arc<RwLock<MonitoringBuffer>>,
    cancel_token: CancellationToken,
    cache: Arc<MonitoringCache>,
) -> Result<()> {
    let mut interval = interval(Duration::from_secs(u64::from(
        state.config().await.tick_seconds,
    )));

    info!(
        "Starting monitoring service with tick interval of {} seconds",
        state.config().await.tick_seconds
    );
    let mut subcription = event_bus.subscribe();
    let (rx, mut tx) = tokio::sync::mpsc::channel(100);
    event_bus.register_handler(ServiceType::Monitoring, rx);

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Monitoring service cancelled");
                break;
            }
            request = tx.recv() => {
                match request {
                    Some(req) => {
                        info!("Received request: {:?}", req);
                        let e = handle_event(&state, req.payload.clone(), cache.clone()).await;
                        let _ = req.response_channel.send(e);
                    },
                    None => {
                        info!("Command channel closed, exiting fan color service");
                        break;
                    }
                }
            }
            event = subcription.recv() => {
                match event {
                    Ok(event) => {
                        if let Err(e) = handle_notify(&state, event, batch_data.clone(), cache.clone()).await {
                            error!("Failed to handle event: {e}");
                        }
                    }
                    Err(e) => {
                        error!("Failed to receive event: {e}");
                    }
                }
            }
            _instant = interval.tick() => {
                if let Err(e) = collect_and_process_temperatures(&state, &event_bus, batch_data.clone(), cache.clone()).await {
                    error!("Failed to collect temperatures: {e}");
                }
            }
        }
    }
    Ok(())
}

async fn handle_event(
    state: &Arc<AppState>,
    request: Arc<RequestPayload>,
    cache: Arc<MonitoringCache>,
) -> Result<Response> {
    match *request {
        RequestPayload::PrepareConfigUpdate(_) => {
            info!("Configuration change detected, reloading curves and mappings");
            let config = state.config().await;
            let new_mapping = MappingCache {
                mappings: Mapping::load_mappings(&config.mappings),
                curves: config.curves.clone(),
                active_curves: CurveMapping::load_mappings(&config.active_curve_mappings),
            };
            cache.new_mapping.store(Arc::new(new_mapping));
            Ok(Response::Success)
        }
        _ => {
            debug!("Unhandled event: {:?}", request);
            Err(anyhow::anyhow!("Unhandled event: {:?}", request))
        }
    }
}

async fn handle_notify(
    _state: &Arc<AppState>,
    event: Event,
    _batch_data: Arc<RwLock<MonitoringBuffer>>,
    cache: Arc<MonitoringCache>,
) -> Result<()> {
    match event {
        Event::CommitConfigUpdate { .. } => {
            info!("Committing new configuration");
            cache.mapping_cache.store(cache.new_mapping.load_full());
        }
        Event::RollbackConfigUpdate { .. } => {
            info!("Rolling back configuration to previous state");
        }
        _ => {
            debug!("Unhandled event: {:?}", event);
        }
    }
    Ok(())
}

async fn collect_and_process_temperatures(
    state: &Arc<AppState>,
    _event_bus: &MessageBroker,
    batch_data: Arc<RwLock<MonitoringBuffer>>,
    cache: Arc<MonitoringCache>,
) -> Result<()> {
    let mut batch_data = batch_data.write().await;

    batch_data.clear();

    let sensors = state.sensors.read().await;
    let sensors_results = iter(sensors.iter())
        .filter_map(|sensor| {
            let sensor_name = sensor.key();
            async move {
                if let Ok(temp) = sensor.read_temperature().await {
                    debug!("Temperature of {sensor_name}: {temp:.2}°C");
                    Some((sensor_name, temp))
                } else {
                    error!("Failed to read temperature from sensor {sensor_name}");
                    None
                }
            }
        })
        .collect::<HashMap<_, _>>()
        .await;
    drop(sensors);

    let buffer = &mut batch_data.batch_data;

    for curve in buffer.curve_entry.iter() {
        for sensor_entry in curve.sensors_entry.iter() {
            if let Some(temp) = sensors_results.get(&sensor_entry.id) {
                let speed = cache.mapping_cache.load().curves[curve.curve_cfg_idx]
                    .calculate_speed(*temp)?;
                let _ = buffer.buffer.data.try_with_write_access(|data| {
                    sensor_entry.set_fans_speed(data, speed);
                });
            }
        }
    }

    let snapshot = buffer.take_buffer_snapshot();
    state
        .controllers
        .read()
        .await
        .batch_update(
            BatchCommand::SetSpeeds {
                data: snapshot.clone(),
            },
            ExecutionMode::Blocking,
        )
        .await
        .context("Failed to update fan speeds in batch")?;

    let _ = buffer.restore_snapshot(snapshot);

    *cache.sensor_data.write().await = batch_data.temp_data.clone();

    Ok(())
}

#[cfg(test)]
#[path = "tests/monitoring_test.rs"]
mod monitoring_test;
