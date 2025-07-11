//! Service provider system for modular system architecture.
//!
//! This module implements a service provider pattern with dependency injection for managing
//! the various services that make up the tt_riingd daemon. Each service is encapsulated
//! in a provider that handles instantiation, configuration, and lifecycle management.
//!
//! # Architecture Overview
//!
//! The provider system follows a dependency injection pattern with these key concepts:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   SystemCoordinator                         │
//! │              (Service Orchestration)                       │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 Service Providers                           │
//! │  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────────┐ │
//! │  │ Monitoring  │ │    D-Bus    │ │      UdevWatcher        │ │
//! │  │  Provider   │ │  Provider   │ │       Provider          │ │
//! │  └─────────────┘ └─────────────┘ └─────────────────────────┘ │
//! │  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────────┐ │
//! │  │   Broadcast │ │  FanColor   │ │    ConfigWatcher        │ │
//! │  │   Provider  │ │  Provider   │ │       Provider          │ │
//! │  └─────────────┘ └─────────────┘ └─────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   Shared Dependencies                       │
//! │     AppState, EventBus, ConfigManager                      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Service Provider Pattern
//!
//! Each service provider implements the [`ServiceProvider`] trait, which defines:
//! - **Metadata**: Name, priority, and criticality classification
//! - **Creation**: Factory methods for service instantiation
//! - **Lifecycle**: Start, stop, and cleanup operations
//! - **Dependencies**: Required shared state and communication channels
//!
//! # Service Categories
//!
//! Services are categorized by their role in the system:
//!
//! ## Core Services (Critical)
//! - **[`MonitoringService`](monitoring)**: Temperature polling and fan control
//! - **[`ConfigWatcherService`](config_watcher)**: Configuration hot-reload
//!
//! ## Interface Services (High Priority)
//! - **[`DBusService`](dbus)**: External API and system integration
//! - **[`UdevWatcherService`](udev_watcher)**: Hardware hotplug detection
//!
//! ## Feature Services (Normal Priority)
//! - **[`FanColorService`](fan_color)**: RGB lighting control
//! - **[`BroadcastService`](broadcast)**: Temperature broadcasting
//!
//! # Dependency Injection
//!
//! Services receive their dependencies through constructor injection:
//!
//! ```no_run
//! use tt_riingd::providers::{ServiceProvider, MonitoringServiceProvider};
//! use tt_riingd::core::{AppState, EventBus};
//! use tt_riingd::config::ConfigManager;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create shared dependencies
//! let config_manager = ConfigManager::load(None).await?;
//! let app_state = Arc::new(AppState::new(config_manager).await?);
//! let event_bus = EventBus::new();
//!
//! // Create provider with injected dependencies
//! let monitoring_provider = MonitoringServiceProvider::new(
//!     app_state.clone(),
//!     event_bus.clone()
//! );
//!
//! // Access provider metadata
//! println!("Service: {} (priority: {})",
//!          monitoring_provider.name(), monitoring_provider.priority());
//! println!("Critical: {}", monitoring_provider.is_critical());
//! # Ok(())
//! # }
//! ```
//!
//! # Service Lifecycle
//!
//! Services follow a standardized lifecycle managed by the SystemCoordinator:
//!
//! 1. **Registration**: Providers registered with coordinator
//! 2. **Dependency Resolution**: Shared dependencies prepared
//! 3. **Priority Ordering**: Services sorted by priority and criticality
//! 4. **Instantiation**: Services created via provider factories
//! 5. **Startup**: Services started in dependency order
//! 6. **Runtime**: Services operate independently via EventBus
//! 7. **Shutdown**: Graceful shutdown in reverse priority order
//!
//! # Event-Driven Communication
//!
//! Services communicate through the [`EventBus`](crate::core::event::EventBus) using
//! strongly-typed events:
//!
//! ```no_run
//! use tt_riingd::core::event::{Event, EventBus};
//! use std::collections::HashMap;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Event-driven communication between services
//! let event_bus = EventBus::new();
//! let mut subscriber = event_bus.subscribe();
//!
//! // Publish temperature update event
//! let mut temps = HashMap::new();
//! temps.insert("cpu_temp".to_string(), 65.0);
//! event_bus.publish(Event::TemperatureChanged(temps))?;
//!
//! // Handle events in services
//! if let Ok(event) = subscriber.recv().await {
//!     match event {
//!         Event::TemperatureChanged(temperatures) => {
//!             for (sensor_id, temp) in temperatures {
//!                 println!("Sensor {} updated: {}°C", sensor_id, temp);
//!             }
//!         }
//!         Event::ConfigChangeDetected(change_type) => {
//!             println!("Configuration change detected: {:?}", change_type);
//!         }
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Examples
//!
//! ## Creating a Custom Service Provider
//!
//! ```no_run
//! use tt_riingd::providers::traits::ServiceProvider;
//! use tt_riingd::core::task_manager::TaskManager;
//! use async_trait::async_trait;
//! use anyhow::Result;
//!
//! pub struct CustomServiceProvider;
//!
//! #[async_trait]
//! impl ServiceProvider for CustomServiceProvider {
//!     fn name(&self) -> &'static str {
//!         "CustomService"
//!     }
//!     
//!     fn priority(&self) -> i32 {
//!         5
//!     }
//!     
//!     fn is_critical(&self) -> bool {
//!         false
//!     }
//!
//!     async fn start(&self, task_manager: &mut TaskManager) -> Result<()> {
//!         // Custom service initialization logic
//!         println!("Starting custom service");
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Service Registration and Startup
//!
//! ```no_run
//! use tt_riingd::core::coordinator::SystemCoordinator;
//! use tt_riingd::providers::{MonitoringServiceProvider, BroadcastServiceProvider};
//! use tt_riingd::core::{AppState, EventBus};
//! use tt_riingd::config::ConfigManager;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Initialize shared dependencies  
//! let config_manager = ConfigManager::load(None).await?;
//! let app_state = Arc::new(AppState::new(config_manager).await?);
//! let event_bus = EventBus::new();
//!
//! // Create coordinator and register services
//! let mut coordinator = SystemCoordinator::new();
//!
//! // Services are registered in priority order and started automatically
//! println!("System coordinator orchestrates service lifecycle");
//! # Ok(())
//! # }
//! ```

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
        std::assert_eq!(monitoring_bus.name(), "MonitoringService");
        std::assert_eq!(broadcast_bus.name(), "BroadcastService");
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
