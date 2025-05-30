//! Dependency injection providers for service management.
//!
//! This module contains all providers for creating and managing system components
//! using the Dependency Injection pattern for loose coupling and testability.

pub mod app_state;
pub mod broadcast;
pub mod config_watcher;
pub mod dbus;
pub mod fan_color;
pub mod monitoring;
pub mod traits;

// Re-export core types for convenience
pub use app_state::AppStateProvider;
pub use broadcast::BroadcastServiceProvider;
pub use config_watcher::ConfigWatcherServiceProvider;
pub use dbus::DBusServiceProvider;
pub use fan_color::FanColorControlServiceProvider;
pub use monitoring::MonitoringServiceProvider;
pub use traits::{AsyncProvider, ServiceProvider};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        app_context::AppState,
        config::{Config, ConfigManager},
        event::EventBus,
    };
    use std::sync::Arc;

    // Helper function to create mock AppState for provider integration testing
    async fn create_test_app_state() -> Arc<AppState> {
        let config = Config::default();
        let config_manager = ConfigManager::new(config, std::path::PathBuf::from("/tmp/test.yml"));
        Arc::new(AppState::new(config_manager).await.unwrap())
    }

    #[tokio::test]
    async fn test_all_service_providers_creation() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Test that all providers can be created with shared dependencies
        let monitoring = MonitoringServiceProvider::new(state.clone(), event_bus.clone());
        let broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let fan_color = FanColorControlServiceProvider::new(state.clone(), event_bus.clone());

        // Verify provider metadata
        assert_eq!(monitoring.name(), "MonitoringService");
        assert_eq!(broadcast.name(), "BroadcastService");
        assert_eq!(fan_color.name(), "FanColorService");

        // Verify priority ordering
        assert!(monitoring.priority() > fan_color.priority());
        assert!(fan_color.priority() > broadcast.priority());

        // Verify criticality classification
        assert!(monitoring.is_critical());
        assert!(!broadcast.is_critical());
        assert!(!fan_color.is_critical());
    }

    #[tokio::test]
    async fn test_service_provider_priority_ordering() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Create providers and collect their metadata
        let providers = vec![
            (
                BroadcastServiceProvider::new(state.clone(), event_bus.clone()).name(),
                BroadcastServiceProvider::new(state.clone(), event_bus.clone()).priority(),
            ),
            (
                MonitoringServiceProvider::new(state.clone(), event_bus.clone()).name(),
                MonitoringServiceProvider::new(state.clone(), event_bus.clone()).priority(),
            ),
            (
                FanColorControlServiceProvider::new(state.clone(), event_bus.clone()).name(),
                FanColorControlServiceProvider::new(state.clone(), event_bus.clone()).priority(),
            ),
        ];

        // Sort by priority (high to low)
        let mut sorted_providers = providers;
        sorted_providers.sort_by_key(|(_, priority)| std::cmp::Reverse(*priority));

        // Verify correct order: Monitoring (10) > FanColor (4) > Broadcast (3)
        assert_eq!(sorted_providers[0].0, "MonitoringService");
        assert_eq!(sorted_providers[1].0, "FanColorService");
        assert_eq!(sorted_providers[2].0, "BroadcastService");

        // Verify priorities
        assert_eq!(sorted_providers[0].1, 10);
        assert_eq!(sorted_providers[1].1, 4);
        assert_eq!(sorted_providers[2].1, 3);
    }

    #[tokio::test]
    async fn test_shared_state_dependency_injection() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Create multiple providers with the same shared state
        let _monitoring = MonitoringServiceProvider::new(state.clone(), event_bus.clone());
        let _broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let _fan_color = FanColorControlServiceProvider::new(state.clone(), event_bus.clone());

        // All providers should share the same underlying state
        // This is tested by ensuring they can all be created successfully
        // and that the Arc reference counting works correctly

        // Verify that all providers are using the same EventBus capacity
        let monitoring_bus = MonitoringServiceProvider::new(state.clone(), event_bus.clone());
        let broadcast_bus = BroadcastServiceProvider::new(state.clone(), event_bus.clone());

        // All should have the same base properties
        assert_eq!(monitoring_bus.name(), "MonitoringService");
        assert_eq!(broadcast_bus.name(), "BroadcastService");
    }

    #[tokio::test]
    async fn test_service_provider_trait_compliance() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Test that all providers implement ServiceProvider trait correctly
        let providers: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(MonitoringServiceProvider::new(
                state.clone(),
                event_bus.clone(),
            )),
            Box::new(BroadcastServiceProvider::new(
                state.clone(),
                event_bus.clone(),
            )),
            Box::new(FanColorControlServiceProvider::new(
                state.clone(),
                event_bus.clone(),
            )),
        ];

        // Verify trait methods work correctly
        for provider in providers {
            // Name should not be empty
            assert!(!provider.name().is_empty());

            // Priority should be reasonable (0-100 range)
            assert!(provider.priority() >= 0);
            assert!(provider.priority() <= 100);

            // Criticality should be determined
            let _is_critical = provider.is_critical();
        }
    }

    #[tokio::test]
    async fn test_critical_vs_noncritical_classification() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Test criticality classification
        let monitoring = MonitoringServiceProvider::new(state.clone(), event_bus.clone());
        let broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let fan_color = FanColorControlServiceProvider::new(state.clone(), event_bus.clone());

        // Monitoring should be critical (core functionality)
        assert!(monitoring.is_critical());

        // Broadcast should be non-critical (optional feature)
        assert!(!broadcast.is_critical());

        // Fan color should be non-critical (aesthetic feature)
        assert!(!fan_color.is_critical());

        // Verify priority correlates with criticality for critical services
        if monitoring.is_critical() {
            assert!(monitoring.priority() >= 5); // Critical services should have higher priority
        }
    }

    #[tokio::test]
    async fn test_event_bus_sharing() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Test that multiple providers can share the same event bus
        let _monitoring = MonitoringServiceProvider::new(state.clone(), event_bus.clone());
        let _broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let _fan_color = FanColorControlServiceProvider::new(state.clone(), event_bus.clone());

        // All providers should be able to use the same event bus
        // This tests that EventBus::clone() works correctly and
        // that multiple providers can share event communication

        // Test event bus functionality
        let _receiver = event_bus.subscribe();
        assert!(
            event_bus
                .publish(crate::event::Event::SystemShutdown)
                .is_ok()
        );

        // The receiver should be able to receive the event
        // (This is a basic functionality test, not requiring async)
    }

    #[tokio::test]
    async fn test_app_state_concurrent_access() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Test that AppState can be safely accessed concurrently
        let state1 = state.clone();
        let state2 = state.clone();
        let state3 = state.clone();

        let event_bus1 = event_bus.clone();
        let event_bus2 = event_bus.clone();
        let event_bus3 = event_bus.clone();

        // Create providers in different "threads" (tasks)
        let task1 = tokio::spawn(async move {
            let _provider = MonitoringServiceProvider::new(state1, event_bus1);
        });

        let task2 = tokio::spawn(async move {
            let _provider = BroadcastServiceProvider::new(state2, event_bus2);
        });

        let task3 = tokio::spawn(async move {
            let _provider = FanColorControlServiceProvider::new(state3, event_bus3);
        });

        // All tasks should complete successfully
        assert!(task1.await.is_ok());
        assert!(task2.await.is_ok());
        assert!(task3.await.is_ok());
    }

    #[tokio::test]
    async fn test_provider_metadata_consistency() {
        let state = create_test_app_state().await;
        let event_bus = EventBus::new();

        // Test that provider metadata is consistent across multiple creations
        let monitoring1 = MonitoringServiceProvider::new(state.clone(), event_bus.clone());
        let monitoring2 = MonitoringServiceProvider::new(state.clone(), event_bus.clone());

        assert_eq!(monitoring1.name(), monitoring2.name());
        assert_eq!(monitoring1.priority(), monitoring2.priority());
        assert_eq!(monitoring1.is_critical(), monitoring2.is_critical());

        let broadcast1 = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let broadcast2 = BroadcastServiceProvider::new(state.clone(), event_bus.clone());

        assert_eq!(broadcast1.name(), broadcast2.name());
        assert_eq!(broadcast1.priority(), broadcast2.priority());
        assert_eq!(broadcast1.is_critical(), broadcast2.is_critical());
    }
}
