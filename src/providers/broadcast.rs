use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{
    app_context::AppState,
    event::{Event, EventBus},
    providers::traits::ServiceProvider,
    task_manager::TaskManager,
};

/// Temperature broadcast service provider.
///
/// Provides a non-critical service that periodically broadcasts current
/// temperature readings to all event subscribers. This enables other services
/// and external systems to monitor system temperature status.
///
/// # Priority and Criticality
///
/// - **Priority**: 3 (low)
/// - **Critical**: No (optional service)
///
/// # Features
///
/// - Periodic temperature state broadcasting
/// - Configurable broadcast interval
/// - Event-driven communication
/// - Non-blocking operation
///
/// # Configuration
///
/// The broadcast interval is determined by `tick_seconds * 2` from the
/// main configuration, providing less frequent updates than monitoring.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use tt_riingd::providers::BroadcastServiceProvider;
/// use tt_riingd::event::EventBus;
/// use tt_riingd::app_context::AppState;
///
/// # async fn example(state: Arc<AppState>) -> anyhow::Result<()> {
/// let event_bus = EventBus::new();
/// let provider = BroadcastServiceProvider::new(state, event_bus);
/// // Use with TaskManager to start the service
/// # Ok(())
/// # }
/// ```
pub struct BroadcastServiceProvider {
    state: Arc<AppState>,
    event_bus: EventBus,
}

impl BroadcastServiceProvider {
    /// Creates a new broadcast service provider.
    pub fn new(state: Arc<AppState>, event_bus: EventBus) -> Self {
        Self { state, event_bus }
    }
}

#[async_trait]
impl ServiceProvider for BroadcastServiceProvider {
    async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
        let state = self.state.clone();
        let event_bus = self.event_bus.clone();

        task_manager
            .spawn_task(self.name().to_string(), |cancel_token| async move {
                run_broadcast_service(state, event_bus, cancel_token).await
            })
            .await
    }

    fn name(&self) -> &'static str {
        "BroadcastService"
    }

    fn priority(&self) -> i32 {
        3
    }

    fn is_critical(&self) -> bool {
        false
    }
}

async fn run_broadcast_service(
    state: Arc<AppState>,
    event_bus: EventBus,
    cancel_token: CancellationToken,
) -> Result<()> {
    let mut interval = interval(Duration::from_secs(
        u64::from(state.config().await.tick_seconds) * 2,
    ));

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("Broadcast service cancelled");
                break;
            }
            _instant = interval.tick() => {
                broadcast_current_state(&state, &event_bus).await;
            }
        }
    }
    Ok(())
}

async fn broadcast_current_state(state: &Arc<AppState>, event_bus: &EventBus) {
    let sensor_data = state.sensor_data.read().await.clone();

    if let Err(e) = event_bus.publish(Event::TemperatureChanged(sensor_data)) {
        error!("Failed to broadcast temperature state: {e}");
    }
}
