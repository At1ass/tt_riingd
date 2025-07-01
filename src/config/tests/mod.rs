//! Configuration module unit tests
//!
//! Comprehensive test suite for configuration loading, validation, and management

#[cfg(test)]
pub mod config_test;

pub use pretty_assertions::assert_eq;
pub use proptest::prelude::*;
pub use tempfile::NamedTempFile;

mod fan_curve_test;
mod mappings_test;
