//! Providers module unit tests
//!
//! Comprehensive test suite for service providers and traits

#[cfg(test)]
pub mod traits_test;

// Re-export common test utilities
pub use mockall::mock;
pub use pretty_assertions::assert_eq;
pub use async_trait::async_trait;

mod config_watcher_test;
mod dbus_test;
mod fan_color_test;
mod hot_reload_test;
mod monitoring_test; 