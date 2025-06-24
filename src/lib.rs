//! # tt_riingd
//!
//! A Linux daemon for controlling Thermaltake Riing fans via HID interface.
//!
//! ## Features
//!
//! - **Async Architecture**: Built on Tokio for high performance
//! - **Event-Driven**: Modular services communicate via EventBus
//! - **Temperature Monitoring**: Supports lm-sensors integration
//! - **Fan Control**: Dynamic speed curves based on temperature
//! - **Color Control**: RGB lighting control for compatible fans
//! - **D-Bus Interface**: System integration and external control
//! - **Hot Reload**: Configuration changes without restart
//!
//! ## Architecture
//!
//! The daemon uses a provider-based dependency injection system with:
//! - [`SystemCoordinator`](core::coordinator::SystemCoordinator) - Main lifecycle manager
//! - [`EventBus`](core::event::EventBus) - Inter-service communication
//! - [`AppState`](core::app_context::AppState) - Shared application state
//! - Service providers for modular functionality
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

// Re-exports for backward compatibility and convenience
pub use core::{app_context, application, coordinator, event, task_manager};

pub use config::{cfg::ConfigManager, fan_curve, mappings};

pub use bootstrap::{cli, daemon, logging, runtime};

#[cfg(test)]
pub use test_utils::*;

// Development and testing utilities
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
        // Keep temp_file alive by forgetting it (for test purposes only)
        std::mem::forget(temp_file);
        Ok(config_manager)
    }
}
