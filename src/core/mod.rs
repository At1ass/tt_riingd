//! Core system components for tt_riingd daemon.
//!
//! This module contains the fundamental building blocks of the tt_riingd daemon:
//! - Application lifecycle management and service orchestration
//! - Event-driven inter-service communication system
//! - Async task management with graceful shutdown
//! - Shared application state and configuration management
//!
//! # Architecture Overview
//!
//! The core system follows a service-oriented architecture with these key components:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Application                            │
//! │  Entry point, configuration loading, service coordination  │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  SystemCoordinator                          │
//! │    Service lifecycle, dependency injection, event routing  │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     EventBus                                │
//! │        Pub/sub communication between services               │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                   TaskManager                               │
//! │       Async task lifecycle and graceful shutdown           │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    AppState                                 │
//! │         Shared configuration and runtime state             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Event-Driven Design
//!
//! Services communicate through the [`EventBus`] using strongly-typed events:
//! - `TemperatureChanged` - Temperature sensor updates
//! - `ConfigReloaded` - Configuration file changes
//! - `DeviceConnected`/`DeviceDisconnected` - Hardware hotplug events
//! - `ServiceStopped` - Service lifecycle notifications
//!
//! # Service Provider Pattern
//!
//! Services implement the [`ServiceProvider`](crate::providers::traits::ServiceProvider) trait
//! and are registered with the [`SystemCoordinator`](coordinator::SystemCoordinator):
//!
//! ```no_run
//! use tt_riingd::core::coordinator::SystemCoordinator;
//!
//! # fn example() -> anyhow::Result<()> {
//! // SystemCoordinator manages service lifecycle
//! let _coordinator = SystemCoordinator::new();
//! println!("Service coordination and dependency injection");
//! # Ok(())
//! # }
//! ```
//!
//! # Examples
//!
//! ## Basic Application Setup
//!
//! ```no_run
//! use tt_riingd::core::application::Application;
//! use tt_riingd::config::cfg::ConfigManager;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config_manager = ConfigManager::load(None).await?;
//!     
//!     Application::builder()
//!         .with_config_manager(config_manager)
//!         .build()
//!         .await?
//!         .run()
//!         .await
//! }
//! ```
//!
//! ## Event Publishing and Subscription
//!
//! ```no_run
//! use tt_riingd::core::event::{EventBus, Event};
//! use std::collections::HashMap;
//!
//! # fn example() -> anyhow::Result<()> {
//! let event_bus = EventBus::new();
//! let _subscriber = event_bus.subscribe();
//!
//! // Event bus enables pub/sub communication
//! println!("Event system ready for inter-service communication");
//! # Ok(())
//! # }
//! ```

pub mod app_context;
pub mod application;
pub mod coordinator;
pub mod event;
pub mod task_manager;

// Re-export commonly used items
pub use app_context::AppState;
pub use event::{ConfigChangeType, Event, EventBus};
pub use task_manager::TaskManager;
