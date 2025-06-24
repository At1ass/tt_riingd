//! Bootstrap module unit tests
//!
//! Comprehensive test suite for logging and initialization

#[cfg(test)]
pub mod logging_test;

// Re-export common test utilities
pub use std::io::Write;
pub use tracing::Level; 