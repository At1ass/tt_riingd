//! RGB color buffer implementation.
//!
//! This module provides specialized buffer types and utilities for RGB LED control.
//! It uses the generic buffer system with optimizations specific to color data.

use super::{
    Rgb,
    core::{Buffer, BufferElement, Snapshot},
    strategies::DoubleBufferStrategy,
};

/// Maximum LED buffer capacity
pub const MAX_LEDS: usize = 2048;

/// Buffer data type for RGB colors
pub type ColorBufferData = super::core::BufferData<Rgb, MAX_LEDS>;

/// Snapshot type for RGB color data
pub type ColorSnapshot = Snapshot<Rgb, MAX_LEDS>;

/// High-performance double-buffered RGB color buffer
///
/// Optimized for concurrent access patterns with producer-consumer separation:
/// - Producer: Color calculation service (50ms intervals)
/// - Consumer: Hardware transmission service (50ms intervals, independent)
pub type ColorBuffer = Buffer<Rgb, MAX_LEDS, DoubleBufferStrategy<Rgb, MAX_LEDS>>;

impl BufferElement for Rgb {
    fn zero() -> Self {
        Rgb { r: 0, g: 0, b: 0 }
    }

    fn leds_per_channel() -> usize {
        0 // Dynamic value taken from hardware spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::registry::ControllerSpec;

    #[tokio::test]
    async fn test_color_buffer_creation() {
        let buffer = ColorBuffer::new();
        assert_eq!(buffer.layout.controller_count(), 0);
    }

    #[tokio::test]
    async fn test_color_buffer_with_specs() {
        let specs = vec![ControllerSpec {
            id: "test_controller".to_string(),
            channels: 4,
            leds: 16,
        }];

        let buffer = ColorBuffer::from_cache(&specs);
        assert_eq!(buffer.layout.controller_count(), 1);
    }

    #[tokio::test]
    async fn test_color_buffer_batch_fill() -> anyhow::Result<()> {
        let buffer = ColorBuffer::new();

        buffer
            .batch_fill(|data| {
                data[0..10].fill(Rgb::new(255, 0, 0)); // Red
                data[10..20].fill(Rgb::new(0, 255, 0)); // Green
                Ok(())
            })
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_color_snapshot() -> anyhow::Result<()> {
        let buffer = ColorBuffer::new();

        // Fill some data
        buffer
            .batch_fill(|data| {
                data[0..5].fill(Rgb::new(255, 255, 255));
                Ok(())
            })
            .await?;

        // Take snapshot
        let snapshot = buffer.take_snapshot().await?;

        // Verify snapshot contains the data
        assert_eq!(snapshot.data.as_slice()[0], Rgb::new(255, 255, 255));

        // Restore snapshot
        buffer.restore_snapshot(snapshot).await?;

        Ok(())
    }
}
