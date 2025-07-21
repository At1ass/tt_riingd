//! Service provider system for modular system architecture.
//!
//! Implements service provider pattern with dependency injection for managing daemon services.
//! Each service is encapsulated in a provider that handles instantiation, configuration, and lifecycle.
//!
//! See the [Service Provider Guide](https://docs.rs/tt_riingd/latest/tt_riingd/docs/providers.html)
//! for detailed architecture, dependency injection patterns, and custom provider implementation.

pub mod app_state;
pub mod broadcast;
pub mod config_watcher;
pub mod dbus;
pub mod fan_color;
pub mod monitoring;
pub mod traits;
pub mod udev_watcher;

// Re-export core types for convenience
pub use app_state::AppStateProvider;
pub use broadcast::BroadcastServiceProvider;
pub use config_watcher::ConfigWatcherServiceProvider;
pub use dbus::DBusServiceProvider;
pub use fan_color::FanColorControlServiceProvider;
pub use monitoring::MonitoringServiceProvider;
pub use traits::{AsyncProvider, ServiceProvider};
pub use udev_watcher::UdevWatcherServiceProvider;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        config::{Config, ConfigManager},
        core::{AppState, event::MessageBroker},
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
        let event_bus = MessageBroker::new();

        // Test that all providers can be created with shared dependencies
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let monitoring =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let fan_color =
            FanColorControlServiceProvider::new(state.clone(), event_bus.clone(), &config_manager)
                .await;

        // Verify provider metadata
        std::assert_eq!(monitoring.name(), "MonitoringService");
        std::assert_eq!(broadcast.name(), "BroadcastService");
        std::assert_eq!(fan_color.name(), "FanColorService");

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
        let event_bus = MessageBroker::new();

        // Create providers and collect their metadata
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let monitoring =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let fan_color =
            FanColorControlServiceProvider::new(state.clone(), event_bus.clone(), &config_manager)
                .await;

        let providers = vec![
            (broadcast.name(), broadcast.priority()),
            (monitoring.name(), monitoring.priority()),
            (fan_color.name(), fan_color.priority()),
        ];

        // Sort by priority (high to low)
        let mut sorted_providers = providers;
        sorted_providers.sort_by_key(|(_, priority)| std::cmp::Reverse(*priority));

        // Verify correct order: Monitoring (10) > FanColor (4) > Broadcast (3)
        std::assert_eq!(sorted_providers[0].0, "MonitoringService");
        std::assert_eq!(sorted_providers[1].0, "FanColorService");
        std::assert_eq!(sorted_providers[2].0, "BroadcastService");

        // Verify priorities
        std::assert_eq!(sorted_providers[0].1, 10);
        std::assert_eq!(sorted_providers[1].1, 4);
        std::assert_eq!(sorted_providers[2].1, 3);
    }

    #[tokio::test]
    async fn test_shared_state_dependency_injection() {
        let state = create_test_app_state().await;
        let event_bus = MessageBroker::new();

        // Create multiple providers with the same shared state
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let _monitoring =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let _broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let _fan_color =
            FanColorControlServiceProvider::new(state.clone(), event_bus.clone(), &config_manager)
                .await;

        // All providers should share the same underlying state
        // This is tested by ensuring they can all be created successfully
        // and that the Arc reference counting works correctly

        // Verify that all providers are using the same MessageBroker capacity
        let monitoring_bus =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let broadcast_bus = BroadcastServiceProvider::new(state.clone(), event_bus.clone());

        // All should have the same base properties
        std::assert_eq!(monitoring_bus.name(), "MonitoringService");
        std::assert_eq!(broadcast_bus.name(), "BroadcastService");
    }

    #[tokio::test]
    async fn test_service_provider_trait_compliance() {
        let state = create_test_app_state().await;
        let event_bus = MessageBroker::new();

        // Test that all providers implement ServiceProvider trait correctly
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let monitoring =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let fan_color =
            FanColorControlServiceProvider::new(state.clone(), event_bus.clone(), &config_manager)
                .await;

        let providers: Vec<Box<dyn ServiceProvider>> = vec![
            Box::new(monitoring),
            Box::new(broadcast),
            Box::new(fan_color),
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
        let event_bus = MessageBroker::new();

        // Test criticality classification
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let monitoring =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let fan_color =
            FanColorControlServiceProvider::new(state.clone(), event_bus.clone(), &config_manager)
                .await;

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
        let event_bus = MessageBroker::new();

        // Test that multiple providers can share the same event bus
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let _monitoring =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let _broadcast = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let _fan_color =
            FanColorControlServiceProvider::new(state.clone(), event_bus.clone(), &config_manager)
                .await;

        // All providers should be able to use the same event bus
        // This tests that MessageBroker::clone() works correctly and
        // that multiple providers can share event communication

        // Test event bus functionality
        let _receiver = event_bus.subscribe();
        assert!(
            event_bus
                .notify(crate::core::event::Event::SystemShutdown)
                .is_ok()
        );

        // The receiver should be able to receive the event
        // (This is a basic functionality test, not requiring async)
    }

    #[tokio::test]
    async fn test_app_state_concurrent_access() {
        let state = create_test_app_state().await;
        let event_bus = MessageBroker::new();

        // Test that AppState can be safely accessed concurrently
        let state1 = state.clone();
        let state2 = state.clone();
        let state3 = state.clone();

        let event_bus1 = event_bus.clone();
        let event_bus2 = event_bus.clone();
        let event_bus3 = event_bus.clone();

        // Create providers in different "threads" (tasks)
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let config1 = config_manager.clone();
        let config2 = config_manager.clone();

        let task1 = tokio::spawn(async move {
            let _provider = MonitoringServiceProvider::new(state1, event_bus1, &config1).await;
        });

        let task2 = tokio::spawn(async move {
            let _provider = BroadcastServiceProvider::new(state2, event_bus2);
        });

        let task3 = tokio::spawn(async move {
            let _provider = FanColorControlServiceProvider::new(state3, event_bus3, &config2).await;
        });

        // All tasks should complete successfully
        assert!(task1.await.is_ok());
        assert!(task2.await.is_ok());
        assert!(task3.await.is_ok());
    }

    #[tokio::test]
    async fn test_provider_metadata_consistency() {
        let state = create_test_app_state().await;
        let event_bus = MessageBroker::new();

        // Test that provider metadata is consistent across multiple creations
        let config_manager =
            ConfigManager::new(Config::default(), std::path::PathBuf::from("/tmp/test.yml"));
        let monitoring1 =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;
        let monitoring2 =
            MonitoringServiceProvider::new(state.clone(), event_bus.clone(), &config_manager).await;

        std::assert_eq!(monitoring1.name(), monitoring2.name());
        std::assert_eq!(monitoring1.priority(), monitoring2.priority());
        std::assert_eq!(monitoring1.is_critical(), monitoring2.is_critical());

        let broadcast1 = BroadcastServiceProvider::new(state.clone(), event_bus.clone());
        let broadcast2 = BroadcastServiceProvider::new(state.clone(), event_bus.clone());

        std::assert_eq!(broadcast1.name(), broadcast2.name());
        std::assert_eq!(broadcast1.priority(), broadcast2.priority());
        std::assert_eq!(broadcast1.is_critical(), broadcast2.is_critical());
    }
}
