//! High-performance buffer management for RGB LED control and fan speed control.
//!
//! This module provides efficient buffer system with specialized implementations
//! for different data types. It uses trait-based design for maximum flexibility
//! and performance with optimal strategies for each use case.
//!
//! # Architecture
//!
//! - **Core**: Fundamental traits and types (`BufferElement`, `BufferStrategy`, `Buffer`)
//! - **Strategies**: Different buffering approaches (`SingleBufferStrategy`, `DoubleBufferStrategy`)
//! - **Color**: RGB LED buffer implementation with double-buffering for concurrent access
//! - **Speed**: Fan speed buffer implementation with single-buffering for simple access
//! - **Layout Management**: Hardware-aware buffer organization
//!
//! # Examples
//!
//! ## RGB Color Buffer
//! ```no_run
//! use tt_riingd::buffer::color::ColorBuffer;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let color_buffer = ColorBuffer::new();
//! color_buffer.batch_fill(|data| {
//!     data[0..12].fill(tt_riingd::buffer::Rgb::new(255, 0, 0)); // Red
//!     Ok(())
//! }).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Fan Speed Buffer
//! ```no_run
//! use tt_riingd::buffer::{speed::SpeedBuffer, BufferStrategy};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut speed_buffer = SpeedBuffer::new();
//! speed_buffer.data.with_write_access(|data| {
//!     data[0..4].fill(128); // 50% fan speed
//!     Ok(())
//! }).await?;
//! # Ok(())
//! # }
//! ```

// Core buffer system
pub mod core;
pub mod strategies;

// Specialized buffer implementations
pub mod color;
pub mod speed;

// Re-export core types and commonly used items
pub use core::{Buffer, BufferData, BufferElement, BufferStrategy, Snapshot};
pub use strategies::{DoubleBufferStrategy, SingleBufferStrategy};

// Re-export specialized buffer types
pub use color::{ColorBuffer, ColorBufferData, ColorSnapshot, MAX_LEDS};
pub use speed::{MAX_FANS_TOTALS, SpeedBuffer, SpeedBufferData, SpeedData, SpeedSnapshot};

/// Maximum LED buffer capacity across all controllers
const MAX_LEDS_BUFFER_SIZE: usize = 2048;

/// Controller layout entry describing location and configuration in the buffer
#[derive(Debug, Clone)]
pub struct ControllerEntry {
    pub controller_id: String,
    pub(crate) offset: usize,
    pub(crate) max_channels: usize,
    pub(crate) leds_per_channel: usize,
    pub(crate) active_channels: u32,
}

impl ControllerEntry {
    /// Create a new controller entry for testing
    pub fn new_for_testing(
        controller_id: String,
        offset: usize,
        max_channels: usize,
        leds_per_channel: usize,
        active_channels: u32,
    ) -> Self {
        Self {
            controller_id,
            offset,
            max_channels,
            leds_per_channel,
            active_channels,
        }
    }

    pub fn is_channel_active(&self, channel_id: usize) -> bool {
        if channel_id >= self.max_channels {
            return false;
        }
        (self.active_channels & (1 << channel_id)) != 0
    }

    pub fn activate_channel(&mut self, channel_id: usize) {
        if channel_id < self.max_channels {
            self.active_channels |= 1 << channel_id;
        }
    }

    pub fn deactivate_channel(&mut self, channel_id: usize) {
        if channel_id < self.max_channels {
            self.active_channels &= !(1 << channel_id);
        }
    }

    pub fn channel_bounds(&self, channel_id: usize) -> Option<(usize, usize)> {
        if channel_id < self.max_channels {
            let start = channel_id * self.leds_per_channel;
            let end = start + self.leds_per_channel;
            Some((start, end))
        } else {
            None
        }
    }

    pub fn controller_id(&self) -> &str {
        &self.controller_id
    }

    pub fn max_channels(&self) -> usize {
        self.max_channels
    }

    pub fn leds_per_channel(&self) -> usize {
        self.leds_per_channel
    }

    pub fn active_channels_mask(&self) -> u32 {
        self.active_channels
    }
}

/// Buffer layout describing controller positions and configurations
#[derive(Debug, Clone)]
pub struct Layout {
    pub controllers: Vec<ControllerEntry>,
    used: usize,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            controllers: Vec::new(),
            used: 0,
        }
    }

    pub fn add_controller(
        &mut self,
        controller_id: String,
        max_channels: usize,
        leds_per_channel: usize,
        active_channels: u32,
    ) -> Result<(), &'static str> {
        let offset = self.used;
        let total_leds = max_channels * leds_per_channel;

        if offset + total_leds > MAX_LEDS_BUFFER_SIZE {
            tracing::warn!(
                "Buffer overflow, cannot add controller: {:?}",
                controller_id
            );
            return Err("Buffer overflow");
        }

        self.controllers.push(ControllerEntry {
            controller_id,
            offset,
            max_channels,
            leds_per_channel,
            active_channels,
        });

        self.used += total_leds;
        Ok(())
    }

    pub fn used_capacity(&self) -> usize {
        self.used
    }

    pub fn total_capacity(&self) -> usize {
        MAX_LEDS_BUFFER_SIZE
    }

    pub fn controller_count(&self) -> usize {
        self.controllers.len()
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

/// RGB color representation for LED control.
///
/// Uses standard 8-bit RGB values with memory layout compatible
/// with hardware USB protocols.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Create new RGB color
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Black color (all LEDs off)
    pub const fn black() -> Self {
        Self::new(0, 0, 0)
    }

    /// White color (all LEDs on)
    pub const fn white() -> Self {
        Self::new(255, 255, 255)
    }

    /// Convert to tuple for compatibility
    pub const fn to_tuple(self) -> (u8, u8, u8) {
        (self.r, self.g, self.b)
    }

    /// Create from tuple
    pub const fn from_tuple(tuple: (u8, u8, u8)) -> Self {
        Self::new(tuple.0, tuple.1, tuple.2)
    }
}

impl From<(u8, u8, u8)> for Rgb {
    fn from(tuple: (u8, u8, u8)) -> Self {
        Self::from_tuple(tuple)
    }
}

impl From<Rgb> for (u8, u8, u8) {
    fn from(rgb: Rgb) -> Self {
        rgb.to_tuple()
    }
}

impl From<[u8; 3]> for Rgb {
    fn from(array: [u8; 3]) -> Self {
        Self::new(array[0], array[1], array[2])
    }
}

impl From<Rgb> for [u8; 3] {
    fn from(rgb: Rgb) -> Self {
        [rgb.r, rgb.g, rgb.b]
    }
}
