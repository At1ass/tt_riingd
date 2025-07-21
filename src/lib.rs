//! # tt_riingd
//!
//! A high-performance, asynchronous Linux daemon for controlling Thermaltake Riing fans via HID interface.
//!
//! ## Features
//!
//! - **Async Architecture**: Built on Tokio for high performance and non-blocking operations
//! - **Event-Driven**: Modular services communicate via EventBus for loose coupling
//! - **Hotplug Support**: Automatic USB device detection and management with udev integration
//! - **Hardware Fingerprinting**: Stable controller identification across reconnections
//! - **Temperature Monitoring**: Supports lm-sensors and NVIDIA GPU temperature integration
//! - **Advanced Fan Curves**: Constant, step-based, and smooth Bézier curves
//! - **RGB Control**: Full RGB lighting control with temperature-based color mapping
//! - **D-Bus Interface**: Complete API for system integration and external control
//! - **Hot Configuration Reload**: Dynamic configuration updates without daemon restart
//! - **Comprehensive Testing**: 186+ tests covering unit, integration, and documentation
//!
//! ## Architecture
//!
//! The daemon uses a provider-based dependency injection system with event-driven design:
//!
//! - [`SystemCoordinator`](core::coordinator::SystemCoordinator) - Main lifecycle manager and service orchestration
//! - [`EventBus`](core::event::EventBus) - Pub/sub system for inter-service communication
//! - [`AppState`](core::app_context::AppState) - Shared application state and configuration
//! - [`Registry`](drivers::registry::Registry) - Hardware detection and configuration caching
//! - [`UdevWatcher`](providers::udev_watcher) - Linux udev integration for hotplug detection
//! - Service providers for modular functionality (monitoring, D-Bus, color control, etc.)
//!
//! ### Hotplug Flow
//!
//! 1. **Device Event** → `UdevWatcher` detects USB connect/disconnect
//! 2. **Event Bus** → Publishes `DeviceConnected`/`DeviceDisconnected` events  
//! 3. **Coordinator** → Handles events and orchestrates controller lifecycle
//! 4. **Registry** → Manages configuration caching and hardware fingerprinting
//! 5. **Controller** → Created/restored with preserved settings
//!
//! ## Example
//!
//! ```no_run
//! use tt_riingd::{core::application::Application, config::cfg::ConfigManager};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config_manager = ConfigManager::load(None).await?;
//!     Application::builder()
//!         .with_config_manager(config_manager)
//!         .build()
//!         .await?
//!         .run()
//!         .await
//! }
//! ```

// Core modules - fundamental system components
pub mod core;

// Configuration and data structures
pub mod config;

// System initialization and runtime
pub mod bootstrap;

// Visual effects system
pub mod effects;

// External interfaces
pub mod interface;

// Hardware abstraction layers
pub mod drivers;
pub mod temperature_sensors;

// Service providers
pub mod providers;

// Color buffer management
pub mod buffer;

// Re-exports for backward compatibility and convenience
pub use core::{app_context, application, coordinator, event, task_manager};

pub use config::{cfg::ConfigManager, fan_curve, mappings};

pub use bootstrap::{cli, daemon, logging, runtime};

#[cfg(test)]
pub use test_utils::*;

#[cfg(test)]
mod test_utils {
    //! Common test utilities used across the codebase

    use crate::config::{Config, ConfigManager};
    use std::fs;
    use tempfile::NamedTempFile;

    /// Creates a temporary config file with the given config for testing
    pub fn create_temp_config(config: &Config) -> anyhow::Result<NamedTempFile> {
        let yaml = serde_yaml::to_string(config)?;
        let temp_file = NamedTempFile::new()?;
        fs::write(temp_file.path(), yaml)?;
        Ok(temp_file)
    }

    /// Creates a config manager with a temporary config file
    pub async fn create_test_config_manager(config: Config) -> anyhow::Result<ConfigManager> {
        let temp_file = create_temp_config(&config)?;
        let config_manager = ConfigManager::load(Some(temp_file.path().to_path_buf())).await?;
        std::mem::forget(temp_file);
        Ok(config_manager)
    }
}
