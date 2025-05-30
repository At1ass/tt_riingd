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
//! - [`SystemCoordinator`](coordinator::SystemCoordinator) - Main lifecycle manager
//! - [`EventBus`](event::EventBus) - Inter-service communication
//! - [`AppState`](app_context::AppState) - Shared application state
//! - Service providers for modular functionality
//!
//! ## Example
//!
//! ```no_run
//! use tt_riingd::{application::Application, config::ConfigManager};
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

pub mod app_context;
pub mod application;
pub mod config;
pub mod controller;
pub mod coordinator;
pub mod drivers;
pub mod event;
pub mod fan_controller;
pub mod fan_curve;
pub mod interface;
pub mod mappings;
pub mod providers;
pub mod sensors;
pub mod task_manager;
pub mod temperature_sensors;
