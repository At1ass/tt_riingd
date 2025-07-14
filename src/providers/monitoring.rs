use anyhow::{Context, Result};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::{sync::RwLock, time::interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::{
    ConfigManager,
    app_context::AppState,
    config::{CurveCfg, CurveMapping, Mapping},
    core::Event,
    drivers::commands::{BatchCommand, ExecutionMode},
    event::{EventBus, RequestPayload, Response, ServiceType},
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
/// use tt_riingd::event::EventBus;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = EventBus::new();
/// let provider = MonitoringServiceProvider::new(state, event_bus);
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
struct MappingCache {
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
    pub mapping_cache: Arc<ArcSwap<MappingCache>>,
    pub new_mapping: Arc<ArcSwap<MappingCache>>,
    // pub mapping: Mapping,
    // pub curves: Vec<CurveCfg>,
    // pub active_curves: CurveMapping,
}

pub struct MonitoringServiceProvider {
    state: Arc<AppState>,
    event_bus: EventBus,
    cache: Arc<MonitoringCache>,
}

impl MonitoringServiceProvider {
    /// Creates a new monitoring service provider.
    pub async fn new(state: Arc<AppState>, event_bus: EventBus, config: &ConfigManager) -> Self {
        Self {
            state,
            event_bus,
            cache: Arc::new(MonitoringCache {
                sensor_data: RwLock::new(HashMap::new()),
                mapping_cache: Arc::new(ArcSwap::from_pointee(MappingCache {
                    mappings: Mapping::load_mappings(&config.get().await.mappings),
                    curves: config.get().await.curves.clone(),
                    active_curves: CurveMapping::load_mappings(
                        &config.get().await.active_curve_mappings,
                    ),
                })),
                new_mapping: Arc::new(ArcSwap::new(Arc::new(MappingCache::new()))),
            }),
        }
    }
}

pub struct MonitoringBuffer {
    /// Buffer for batch data to be processed.
    pub batch_data: HashMap<String, Vec<(usize, u8)>>,
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
            batch_data: HashMap::new(),
            temp_data: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        // let mut batch_data = self.batch_data.write().await;
        self.batch_data.clear();
        // let mut temp_data = self.temp_data.write().await;
        self.temp_data.clear();
    }
}

#[async_trait]
impl ServiceProvider for MonitoringServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();
        let cache = self.cache.clone();

        // Create a shared buffer for batch data
        let buffer = Arc::new(RwLock::new(MonitoringBuffer::new()));

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                let buffer_m = buffer.clone();
                run_monitoring_service(state, event_bus, buffer_m, cancel_token, cache.clone())
                    .await
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
    event_bus: EventBus,
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

async fn calculate_fan_speed(
    controller_id: &String,
    channel: u8,
    temp: f32,
    cache: &Arc<MonitoringCache>,
) -> Result<u8> {
    let curve_registry: &Vec<_> = &cache.mapping_cache.load().curves;

    let active_curves = &cache.mapping_cache.load().active_curves;

    let curve_name = active_curves
        .get_curve_for_fan(&FanRef {
            controller_id: controller_id.clone(),
            channel: channel as usize,
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No active curve found for controller {} channel {}",
                controller_id,
                channel
            )
        })?;

    let curve = curve_registry
        .iter()
        .find(|c| c.get_id() == curve_name)
        .ok_or_else(|| anyhow::anyhow!("Curve {} not found", curve_name))?;

    curve.calculate_speed(temp)
}

async fn collect_and_process_temperatures(
    state: &Arc<AppState>,
    _event_bus: &EventBus,
    batch_data: Arc<RwLock<MonitoringBuffer>>,
    cache: Arc<MonitoringCache>,
) -> Result<()> {
    let mut batch_data = batch_data.write().await;

    batch_data.clear();

    let sensors = state.sensors.read().await;
    for sensor in sensors.iter() {
        match sensor.read_temperature().await {
            Ok(temp) => {
                let sensor_name = sensor.key();
                batch_data.temp_data.insert(sensor_name.clone(), temp);
                debug!("Temperature of {sensor_name}: {temp:.2}°C");

                for fan in cache
                    .mapping_cache
                    .load()
                    .mappings
                    .fans_for_sensor(&sensor_name)
                {
                    let controller_id = &fan.controller_id;
                    let channel = u8::try_from(fan.channel)
                        .map_err(|_| anyhow::anyhow!("Channel {} too large for u8", fan.channel))?;

                    let speed = calculate_fan_speed(controller_id, channel, temp, &cache)
                        .await
                        .context("Failed to calculate fan speed")?;

                    if let Some(entry) = batch_data.batch_data.get_mut(controller_id) {
                        entry.push((channel as usize, speed));
                    } else {
                        batch_data
                            .batch_data
                            .entry(controller_id.clone())
                            .or_default()
                            .push((channel as usize, speed));
                    }
                }
            }
            Err(e) => {
                error!("Failed to read temperature from sensor: {e}");
            }
        }
    }

    let controllers = state.controllers.read().await;
    controllers
        .batch_update(
            BatchCommand::SetSpeeds {
                data: &batch_data.batch_data,
            },
            ExecutionMode::Blocking,
        )
        .await
        .context("Failed to update fan speeds in batch")?;

    *cache.sensor_data.write().await = batch_data.temp_data.clone();

    Ok(())
}

#[cfg(test)]
#[path = "tests/monitoring_test.rs"]
mod monitoring_test;
