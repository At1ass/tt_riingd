use anyhow::Result;
use async_trait::async_trait;
use log::info;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

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
        log::error!("Failed to broadcast temperature state: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigManager};
    use tokio::time::{sleep, timeout};

    // Helper function to create mock AppState
    async fn create_mock_app_state() -> Arc<AppState> {
        let config = Config::default();
        let config_manager = ConfigManager::new(config, std::path::PathBuf::from("/tmp/test.yml"));
        Arc::new(AppState::new(config_manager).await.unwrap())
    }

    #[tokio::test]
    async fn broadcast_service_provider_creation() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();

        let provider = BroadcastServiceProvider::new(state, event_bus);

        assert_eq!(provider.name(), "BroadcastService");
        assert_eq!(provider.priority(), 3);
        assert!(!provider.is_critical());
    }

    #[tokio::test]
    async fn broadcast_service_starts_successfully() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();
        let mut task_manager = TaskManager::new();

        let provider = BroadcastServiceProvider::new(state, event_bus);
        let result = provider.start(&mut task_manager).await;

        assert!(result.is_ok());
        assert_eq!(task_manager.active_count(), 1);
        assert!(task_manager.is_running("BroadcastService"));

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }

    #[tokio::test]
    async fn broadcast_service_publishes_periodic_events() {
        let state = create_mock_app_state().await;

        // Add some sensor data to broadcast
        {
            let mut sensor_data = state.sensor_data.write().await;
            sensor_data.insert("cpu_temp".to_string(), 45.5);
            sensor_data.insert("gpu_temp".to_string(), 62.3);
        }

        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();
        let mut task_manager = TaskManager::new();

        let provider = BroadcastServiceProvider::new(state, event_bus);
        provider.start(&mut task_manager).await.unwrap();

        // Wait for the service to broadcast at least one event
        let event = timeout(Duration::from_secs(5), receiver.recv()).await;
        assert!(event.is_ok());

        match event.unwrap().unwrap() {
            Event::TemperatureChanged(temperatures) => {
                assert_eq!(temperatures.len(), 2);
                assert!(temperatures.contains_key("cpu_temp"));
                assert!(temperatures.contains_key("gpu_temp"));
                assert_eq!(temperatures["cpu_temp"], 45.5);
                assert_eq!(temperatures["gpu_temp"], 62.3);
            }
            _ => panic!("Expected TemperatureChanged event"),
        }

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }

    #[tokio::test]
    async fn broadcast_service_responds_to_cancellation() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();
        let mut task_manager = TaskManager::new();

        let provider = BroadcastServiceProvider::new(state, event_bus);
        provider.start(&mut task_manager).await.unwrap();

        // Verify service is running
        assert!(task_manager.is_running("BroadcastService"));

        // Request shutdown
        let shutdown_result = task_manager.shutdown_all().await;
        assert!(shutdown_result.is_ok());

        // Verify service stopped
        assert_eq!(task_manager.active_count(), 0);
    }

    #[tokio::test]
    async fn broadcast_service_uses_config_timing() {
        let config = Config {
            tick_seconds: 1, // Fast interval for testing
            ..Default::default()
        };

        let config_manager =
            crate::config::ConfigManager::new(config, std::path::PathBuf::from("/tmp/test.yml"));
        let state = Arc::new(AppState::new(config_manager).await.unwrap());

        let event_bus = EventBus::new();
        let mut task_manager = TaskManager::new();

        let provider = BroadcastServiceProvider::new(state, event_bus);
        let result = provider.start(&mut task_manager).await;

        assert!(result.is_ok());

        // Service should start and run with custom timing
        sleep(Duration::from_millis(100)).await;
        assert!(task_manager.is_running("BroadcastService"));

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }

    #[tokio::test]
    async fn broadcast_service_handles_empty_sensor_data() {
        let state = create_mock_app_state().await;

        // Ensure sensor data is empty
        {
            let mut sensor_data = state.sensor_data.write().await;
            sensor_data.clear();
        }

        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();
        let mut task_manager = TaskManager::new();

        let provider = BroadcastServiceProvider::new(state, event_bus);
        provider.start(&mut task_manager).await.unwrap();

        // Wait for the service to broadcast an event
        let event = timeout(Duration::from_secs(5), receiver.recv()).await;
        assert!(event.is_ok());

        match event.unwrap().unwrap() {
            Event::TemperatureChanged(temperatures) => {
                assert!(temperatures.is_empty());
            }
            _ => panic!("Expected TemperatureChanged event"),
        }

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }

    #[tokio::test]
    async fn broadcast_service_multiple_broadcasts() {
        let state = create_mock_app_state().await;

        // Set up initial sensor data
        {
            let mut sensor_data = state.sensor_data.write().await;
            sensor_data.insert("cpu_temp".to_string(), 40.0);
        }

        let event_bus = EventBus::new();
        let mut receiver = event_bus.subscribe();
        let mut task_manager = TaskManager::new();

        let provider = BroadcastServiceProvider::new(state.clone(), event_bus);
        provider.start(&mut task_manager).await.unwrap();

        // Receive first broadcast
        let event1 = timeout(Duration::from_secs(5), receiver.recv()).await;
        assert!(event1.is_ok());

        // Update sensor data
        {
            let mut sensor_data = state.sensor_data.write().await;
            sensor_data.insert("cpu_temp".to_string(), 50.0);
        }

        // Receive second broadcast
        let event2 = timeout(Duration::from_secs(5), receiver.recv()).await;
        assert!(event2.is_ok());

        // Both should be TemperatureChanged events
        match (event1.unwrap().unwrap(), event2.unwrap().unwrap()) {
            (Event::TemperatureChanged(temps1), Event::TemperatureChanged(temps2)) => {
                assert!(temps1.contains_key("cpu_temp"));
                assert!(temps2.contains_key("cpu_temp"));
                // Note: Since broadcasts use current state, both might have the same value
                // depending on timing of when the broadcast reads the state
            }
            _ => panic!("Expected TemperatureChanged events"),
        }

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }

    #[tokio::test]
    async fn broadcast_service_concurrent_access() {
        let state = create_mock_app_state().await;
        let event_bus = EventBus::new();
        let mut task_manager = TaskManager::new();

        // Start broadcast service
        let provider = BroadcastServiceProvider::new(state.clone(), event_bus);
        provider.start(&mut task_manager).await.unwrap();

        // Spawn concurrent tasks that update sensor data
        let handles = (0..5)
            .map(|i| {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    for j in 0..10 {
                        let mut sensor_data = state_clone.sensor_data.write().await;
                        sensor_data.insert(format!("sensor_{}", i), j as f32);
                        drop(sensor_data);
                        sleep(Duration::from_millis(10)).await;
                    }
                })
            })
            .collect::<Vec<_>>();

        // Let everything run for a bit
        sleep(Duration::from_millis(200)).await;

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Service should still be running
        assert!(task_manager.is_running("BroadcastService"));

        // Cleanup
        task_manager.shutdown_all().await.unwrap();
    }
}
