//! Core system components for tt_riingd daemon.
//!
//! Contains fundamental building blocks: application lifecycle management, event-driven
//! communication, async task management, and shared application state.
//!
//! See the [Core Architecture Guide](https://docs.rs/tt_riingd/latest/tt_riingd/docs/architecture.html)
//! for detailed system design and service-oriented architecture patterns.

pub mod app_context;
pub mod application;
pub mod coordinator;
pub mod event;
mod macros;
pub mod task_manager;
// use macros::*;

// Re-export commonly used items
pub use app_context::AppState;
pub use event::{ConfigChangeType, Event, EventBus};
pub use task_manager::TaskManager;
