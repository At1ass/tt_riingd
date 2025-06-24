//! Temperature sensors module unit tests
//!
//! Comprehensive test suite for temperature sensor functionality

#[cfg(test)]
pub mod sensor_test;

// Re-export common test utilities
pub use pretty_assertions::assert_eq;
pub use proptest::prelude::*;

mod nvidia_test;
mod sensor_manager_test; 