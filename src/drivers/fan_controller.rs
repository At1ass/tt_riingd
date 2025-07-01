//! Fan controller abstraction and trait definitions.

use anyhow::Result;
use async_trait::async_trait;

/// Trait for fan controller hardware implementations.
///
/// Provides a unified interface for controlling fan speed, RGB lighting,
/// and curve management across different hardware types.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::drivers::fan_controller::FanController;
/// use anyhow::Result;
///
/// struct MockController;
///
/// #[async_trait::async_trait]
/// impl FanController for MockController {
///     async fn send_init(&self) -> Result<()> { Ok(()) }
///     async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()> { Ok(()) }
///     async fn update_speed_batch(&self, batch: Vec<(usize, f32, u8)>) -> Result<()> { Ok(()) }
///     async fn update_channel_color(&self, channel: u8, r: u8, g: u8, b: u8) -> Result<()> { Ok(()) }
///     async fn update_color_batch(&self, batch: Vec<(usize, u8, u8, u8)>) -> Result<()> { Ok(()) }
///     async fn firmware_version(&self) -> Result<(u8, u8, u8)> { Ok((1, 0, 0)) }
/// }
/// impl std::fmt::Debug for MockController {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "MockController") }
/// }
/// ```
#[async_trait]
pub trait FanController: Send + Sync + core::fmt::Debug {
    /// Initializes the controller hardware.
    async fn send_init(&self) -> Result<()>;

    /// Updates fan speed for a specific channel based on temperature.
    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()>;

    /// Updates multiple channels with a batch of (temperature, speed) pairs.
    async fn update_speed_batch(&self, batch: Vec<(usize, f32, u8)>) -> Result<()>;

    /// Sets RGB color for a specific channel.
    async fn update_channel_color(&self, _channel: u8, red: u8, green: u8, blue: u8) -> Result<()>;

    /// Updates multiple channels with a batch of (channel index, temperature, speed) tuples.
    async fn update_color_batch(&self, batch: Vec<(usize, Vec<(u8, u8, u8)>)>) -> Result<()>;

    /// Returns the firmware version as (major, minor, patch).
    async fn firmware_version(&self) -> Result<(u8, u8, u8)>;

    /// Returns the number of LEDs controlled by this controller.
    fn led_count(&self) -> usize;
}
