//! Fan speed buffer implementation.
//!
//! This module provides specialized buffer types and utilities for fan speed control.
//! It uses the generic buffer system with optimizations specific to speed data.

use super::{
    core::{Buffer, BufferElement, Snapshot},
    strategies::SingleBufferStrategy,
};

/// Fan speed data type (0-255 representing 0-100% speed)
pub type SpeedData = u8;

/// Maximum fan buffer capacity
pub const MAX_FANS_TOTALS: usize = 256;

/// Buffer data type for fan speeds
pub type SpeedBufferData = super::core::BufferData<SpeedData, MAX_FANS_TOTALS>;

/// Snapshot type for fan speed data
pub type SpeedSnapshot = Snapshot<SpeedData, MAX_FANS_TOTALS>;

/// Single-buffered fan speed buffer
///
/// Optimized for synchronous monitoring service access patterns:
/// - Simple synchronous access for temperature-based speed calculations
/// - Direct hardware communication without complex buffering
pub type SpeedBuffer =
    Buffer<SpeedData, MAX_FANS_TOTALS, SingleBufferStrategy<SpeedData, MAX_FANS_TOTALS>>;

impl BufferElement for SpeedData {
    fn zero() -> Self {
        0
    }

    fn leds_per_channel() -> usize {
        1 // One speed value per fan channel
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::core::BufferStrategy;
    use crate::drivers::registry::ControllerSpec;

    #[test]
    fn test_speed_buffer_creation() {
        let buffer = SpeedBuffer::new();
        assert_eq!(buffer.layout.controller_count(), 0);
    }

    #[test]
    fn test_speed_buffer_with_specs() {
        let specs = vec![ControllerSpec {
            id: "test_controller".to_string(),
            channels: 4,
            leds: 16, // Not used for speed buffers
        }];

        let buffer = SpeedBuffer::from_cache(&specs);
        assert_eq!(buffer.layout.controller_count(), 1);
    }

    #[test]
    fn test_speed_buffer_sync_access() {
        let mut buffer = SpeedBuffer::new();

        // Test synchronous write access
        let result = buffer.data.try_with_write_access(|data| {
            data[0..4].fill(128); // 50% speed
            data[4..8].fill(255); // 100% speed
        });

        assert!(result.is_some());

        // Test synchronous read access
        let result = buffer.data.try_with_read_access(|data| {
            assert_eq!(data[0], 128);
            assert_eq!(data[4], 255);
        });

        assert!(result.is_some());
    }

    #[test]
    fn test_speed_snapshot() {
        let mut buffer = SpeedBuffer::new();

        // Fill some data
        buffer.data.try_with_write_access(|data| {
            data[0..5].fill(200); // High speed
        });

        // Take snapshot
        let snapshot = buffer.try_take_snapshot();
        assert_eq!(snapshot.data.as_slice()[0], 200);

        // Restore snapshot
        buffer.try_restore_snapshot(snapshot).unwrap();
    }
}
