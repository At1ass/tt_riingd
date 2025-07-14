use anyhow::Result;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, mpsc};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::ConfigManager;
use crate::config::EffectStore;
use crate::core::Event;
use crate::drivers::commands::{BatchCommand, ExecutionMode};
use crate::event::{RequestPayload, Response, ServiceType};
use crate::{
    app_context::AppState, event::EventBus, providers::traits::ServiceProvider,
    task_manager::TaskManager,
};

/// RGB fan lighting control service provider.
///
/// Provides a non-critical service that manages RGB lighting on fans based on
/// temperature changes and configured color mappings. The service responds to
/// temperature events and applies color changes according to the configuration.
///
/// # Priority and Criticality
///
/// - **Priority**: 4 (medium-low)
/// - **Critical**: No (optional service)
///
/// # Features
///
/// - Temperature-based color changes
/// - Event-driven color updates
/// - Periodic color refresh (5-second interval)
/// - Configuration-based color mapping
/// - Color change event publishing
///
/// # Configuration
///
/// Requires `colors` and `color_mappings` sections in configuration:
/// - `colors`: Define RGB values for named colors
/// - `color_mappings`: Map colors to specific fan targets
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::FanColorControlServiceProvider;
/// use tt_riingd::event::EventBus;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = EventBus::new();
/// let provider = FanColorControlServiceProvider::new(state, event_bus);
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
struct FanColorServiceCache {
    pub runners: Arc<ArcSwap<EffectStore>>,
    pub new_runners: Arc<ArcSwap<EffectStore>>,
}

pub struct FanColorControlServiceProvider {
    state: Arc<AppState>,
    event_bus: EventBus,
    cache: FanColorServiceCache,
}

type ControllerColorBuffer = Vec<(usize, Vec<(u8, u8, u8)>)>;
// type ConrollerId = u8;
type ConrollerId = String;
type Buffer = HashMap<ConrollerId, ControllerColorBuffer>;

struct DoubleBuffer {
    buffer: [RwLock<Buffer>; 2],
    write_index: AtomicUsize,
}

impl DoubleBuffer {
    fn new() -> Self {
        Self {
            buffer: [RwLock::new(HashMap::new()), RwLock::new(HashMap::new())],
            write_index: AtomicUsize::new(0),
        }
    }

    async fn get_write_buffer(&self) -> RwLockWriteGuard<'_, Buffer> {
        let index = self.write_index.load(std::sync::atomic::Ordering::SeqCst);
        self.buffer[index].write().await
    }

    async fn get_read_buffer(&self) -> RwLockReadGuard<'_, Buffer> {
        let index = self.write_index.load(std::sync::atomic::Ordering::SeqCst) ^ 1;
        self.buffer[index].read().await
    }

    fn swap_buffers(&self) {
        self.write_index
            .fetch_xor(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl FanColorControlServiceProvider {
    /// Creates a new fan color control service provider.
    pub async fn new(state: Arc<AppState>, event_bus: EventBus, config: &ConfigManager) -> Self {
        Self {
            state,
            event_bus,
            cache: FanColorServiceCache {
                runners: Arc::new(ArcSwap::new(Arc::new(EffectStore::build_effect_store(
                    &config.get().await.effects,
                    &config.get().await.effect_mappings,
                )))),
                new_runners: Arc::new(ArcSwap::new(Arc::new(EffectStore::default()))),
            },
        }
    }
}

#[async_trait]
impl ServiceProvider for FanColorControlServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();
        let runners = self.cache.runners.clone();
        let new_runners = self.cache.new_runners.clone();

        let double_buffer = Arc::new(DoubleBuffer::new());
        task_manager
            .spawn_task(format!("{}_calculate", self.name()), {
                let state = state.clone();
                let event_bus = event_bus.clone();
                let buffer = double_buffer.clone();
                |cancel_token| async move {
                    run_calculate_colors_service(
                        state,
                        event_bus,
                        buffer,
                        cancel_token,
                        runners,
                        new_runners,
                    )
                    .await
                }
            })
            .await?;

        task_manager
            .spawn_task(format!("{}_transmit", self.name()), {
                let state = state.clone();
                let event_bus = event_bus.clone();
                let buffer = double_buffer.clone();
                |cancel_token| async move {
                    run_transmit_color_changes(state, event_bus, buffer, cancel_token).await
                }
            })
            .await
    }

    fn name(&self) -> &'static str {
        "FanColorService"
    }

    fn priority(&self) -> i32 {
        4
    }

    fn is_critical(&self) -> bool {
        false
    }
}

async fn run_calculate_colors_service(
    state: Arc<AppState>,
    event_bus: EventBus,
    buffer: Arc<DoubleBuffer>,
    cancel_token: CancellationToken,
    runners: Arc<ArcSwap<EffectStore>>,
    new_runners: Arc<ArcSwap<EffectStore>>,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(50));
    let mut subscriber = event_bus.subscribe();

    let (rx, mut command_tx) = mpsc::channel(100);
    event_bus.register_handler(ServiceType::FanColor, rx);

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Fan color service cancelled");
                break;
            }
            request = command_tx.recv() => {
                match request {
                    Some(req) => {
                        info!("Received request: {:?}", req);
                        let e = handle_event(&state, req.payload.clone(), runners.clone(), new_runners.clone()).await;
                        let _ = req.response_channel.send(e);
                    },
                    None => {
                        info!("Command channel closed, exiting fan color service");
                        break;
                    }
                }
            }
            notify = subscriber.recv() => {
                match notify {
                    Ok(e) => {
                        debug!("Received event: {:?}", e);
                        if let Err(e) = handle_notify(&state, e, runners.clone(), new_runners.clone()).await {
                            error!("Failed to handle event: {e}");
                        }
                    }
                    Err(e) => {
                        warn!("Event bus error: {e}");
                    }
                }
            }
            _instant = interval.tick() => {
                if let Err(e) = calculate_fan_colors(&state, &event_bus, buffer.clone(), runners.clone()).await {
                    error!("Failed to update fan colors: {e}");
                }
            }
        }
    }
    Ok(())
}

async fn handle_event(
    state: &Arc<AppState>,
    event: Arc<RequestPayload>,
    _runners: Arc<ArcSwap<EffectStore>>,
    new_runners: Arc<ArcSwap<EffectStore>>,
) -> Result<Response> {
    match *event {
        RequestPayload::PrepareConfigUpdate(_) => {
            info!("Configuration change detected, recalculating fan colors");
            rebuild_effects(state, new_runners.clone()).await;
            Ok(Response::Success)
        }
        _ => {
            warn!("Unhandled request type: {:?}", event);
            Err(anyhow::anyhow!("Unhandled request type: {:?}", event))
        }
    }
}

async fn rebuild_effects(state: &Arc<AppState>, new_runners: Arc<ArcSwap<EffectStore>>) {
    info!("Rebuilding effect runners");
    let config_manager = state.config_manager.get().await;
    new_runners.store(Arc::new(EffectStore::build_effect_store(
        &config_manager.effects.clone(),
        &config_manager.effect_mappings.clone(),
    )));
}

async fn handle_notify(
    _state: &Arc<AppState>,
    event: Event,
    runners: Arc<ArcSwap<EffectStore>>,
    new_runners: Arc<ArcSwap<EffectStore>>,
) -> Result<()> {
    match event {
        Event::CommitConfigUpdate { transaction_id: _ } => {
            info!("Committing configuration update, rebuilding effect runners");
            runners.store(new_runners.load_full());
        }
        Event::RollbackConfigUpdate { transaction_id: _ } => {
            info!("Rolling back configuration update, restoring previous effect runners");
        }
        _ => debug!("Received event: {:?}", event),
    }
    Ok(())
}

async fn run_transmit_color_changes(
    state: Arc<AppState>,
    event_bus: EventBus,
    buffer: Arc<DoubleBuffer>,
    cancel_token: CancellationToken,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(16));

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Fan color service cancelled");
                break;
            }
            _instant = interval.tick() => {
                if let Err(e) = transmit_color_changes(state.clone(), &event_bus, buffer.clone()).await {
                    error!("Failed to update fan colors: {e}");
                }
            }
        }
    }
    Ok(())
}

async fn calculate_fan_colors(
    state: &Arc<AppState>,
    _event_bus: &EventBus,
    buffer: Arc<DoubleBuffer>,
    runners: Arc<ArcSwap<EffectStore>>,
) -> Result<()> {
    let mut write_buffer = buffer.get_write_buffer().await;
    let runners = runners.load();

    for runner in runners.runners.iter() {
        debug!("Calculating colors for effect: {}", runner.key());
        let instance = runner.value();
        if let Some(rgb) = instance.runner.next_rgb().await {
            for fan_ref in &instance.targets {
                let buf = write_buffer
                    .entry(fan_ref.controller_id.clone())
                    .or_default();

                if !buf
                    .iter_mut()
                    .any(|(channel, _)| *channel == fan_ref.channel)
                {
                    if let Ok(led) = state
                        .controllers
                        .read()
                        .await
                        .led_count(&fan_ref.controller_id)
                        .await
                    {
                        buf.push((fan_ref.channel, vec![(0, 0, 0); led]));
                    } else {
                        warn!(
                            "Controller {} not found for fan reference: {:?}",
                            fan_ref.controller_id, fan_ref
                        );
                        continue;
                    }
                }

                buf.iter_mut()
                    .find(|(channel, _)| *channel == fan_ref.channel)
                    .iter_mut()
                    .for_each(|(_, buffer)| {
                        debug!(
                            "Setting color for controller {} channel {}: {:?}",
                            fan_ref.controller_id, fan_ref.channel, rgb
                        );
                        buffer.iter_mut().for_each(|color| {
                            color.0 = rgb[0];
                            color.1 = rgb[1];
                            color.2 = rgb[2];
                        });
                    });
            }
        } else {
            warn!("No RGB color defined for effect '{}'", runner.key());
        }
    }

    buffer.swap_buffers();

    Ok(())
}

async fn transmit_color_changes(
    state: Arc<AppState>,
    _event_bus: &EventBus,
    buffer: Arc<DoubleBuffer>,
) -> Result<()> {
    let read_buffer = buffer.get_read_buffer().await;

    state
        .controllers
        .read()
        .await
        .batch_update(
            BatchCommand::SetColors { data: &read_buffer },
            ExecutionMode::Blocking,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/fan_color_test.rs"]
mod fan_color_test;
