use anyhow::Result;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::iter;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::ConfigManager;
use crate::buffer::Rgb;
use crate::buffer::color::ColorBuffer;
use crate::config::EffectStore;
use crate::core::event::{Event, MessageBroker, RequestPayload, Response, ServiceType};
use crate::drivers::commands::{BatchCommand, ExecutionMode};
use crate::{app_context::AppState, providers::traits::ServiceProvider, task_manager::TaskManager};

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
/// use tt_riingd::core::event::MessageBroker;
/// use tt_riingd::core::AppState;
/// use tt_riingd::config::{Config, ConfigManager};
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = MessageBroker::new();
/// let config = Config::default();
/// let config_manager = ConfigManager::load(None).await?;
/// let provider = FanColorControlServiceProvider::new(state, event_bus, &config_manager).await;
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
    event_bus: MessageBroker,
    cache: FanColorServiceCache,
}

impl FanColorControlServiceProvider {
    /// Creates a new fan color control service provider.
    pub async fn new(
        state: Arc<AppState>,
        event_bus: MessageBroker,
        config: &ConfigManager,
    ) -> Self {
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

        let spec = state.controllers.read().await.get_controllers_spec();
        let double_buffer = Arc::new(ColorBuffer::from_cache(&spec));

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
    event_bus: MessageBroker,
    buffer: Arc<ColorBuffer>,
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
    _event_bus: MessageBroker,
    buffer: Arc<ColorBuffer>,
    cancel_token: CancellationToken,
) -> Result<()> {
    let mut interval = interval(Duration::from_millis(50));

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Fan color service cancelled");
                break;
            }
            _instant = interval.tick() => {
                if let Err(e) = transmit_color_changes(state.clone(), buffer.clone()).await {
                    error!("Failed to update fan colors: {e}");
                }
            }
        }
    }
    Ok(())
}

async fn calculate_fan_colors(
    _state: &Arc<AppState>,
    _event_bus: &MessageBroker,
    buffer: Arc<ColorBuffer>,
    runners: Arc<ArcSwap<EffectStore>>,
) -> Result<()> {
    let runners = runners.load();

    let runners_values = iter(runners.runners.iter())
        .filter_map(|v| async move {
            let instance = v.value();
            instance
                .runner
                .next_rgb()
                .await
                .map(|rgb| (instance.clone(), rgb))
        })
        .collect::<Vec<_>>()
        .await;

    let layout = buffer.layout.clone();

    buffer
        .batch_fill(move |data| {
            for (instance, rgb) in &runners_values {
                for fan_ref in &instance.targets {
                    if let Some(entry) = layout
                        .controllers
                        .iter()
                        .find(|c| c.controller_id == fan_ref.controller_id)
                    {
                        let start = entry.offset + (fan_ref.channel - 1) * entry.leds_per_channel;
                        let end = start + entry.leds_per_channel;

                        if end <= data.len() {
                            let color = Rgb {
                                r: rgb[0],
                                g: rgb[1],
                                b: rgb[2],
                            };
                            data[start..end].fill(color);
                        }
                    }
                }
            }
            Ok(())
        })
        .await?;

    Ok(())
}

async fn transmit_color_changes(state: Arc<AppState>, buffer: Arc<ColorBuffer>) -> Result<()> {
    let snapshot = buffer.take_snapshot().await?;

    state
        .controllers
        .read()
        .await
        .batch_update(
            BatchCommand::SetColors {
                data: snapshot.clone(),
            },
            ExecutionMode::Blocking,
        )
        .await?;

    buffer.restore_snapshot(snapshot).await?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/fan_color_test.rs"]
mod fan_color_test;
