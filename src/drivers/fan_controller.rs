//! Fan controller abstraction and trait definitions.

use anyhow::Result;
use async_trait::async_trait;
use hidapi::DeviceInfo;

use super::registry::HardwareInfo;

/// Trait for fan controller hardware implementations.
///
/// Provides a unified interface for controlling fan speed, RGB lighting,
/// and curve management across different hardware types.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::drivers::fan_controller::FanController;
/// use tt_riingd::drivers::registry::HardwareInfo;
/// use anyhow::Result;
///
/// struct MockController;
///
/// #[async_trait::async_trait]
/// impl FanController for MockController {
///     async fn send_init(&self) -> Result<()> { Ok(()) }
///     async fn update_speed_batch(&self, batch: &[(usize, u8)]) -> Result<()> { Ok(()) }
///     async fn update_color_batch(&self, batch: &[(usize, &[(u8, u8, u8)])]) -> Result<()> { Ok(()) }
///     async fn firmware_version(&self) -> Result<(u8, u8, u8)> { Ok((1, 0, 0)) }
///     async fn get_device_info(&self) -> Result<hidapi::DeviceInfo> { todo!() }
///     fn led_count(&self) -> usize { 4 }
///     async fn get_id(&self) -> String { "mock".to_string() }
///     async fn get_fingerprint(&self) -> Result<tt_riingd::drivers::HardwareFingerprint> { todo!() }
///     fn hardware_info() -> tt_riingd::drivers::registry::HardwareInfo
///     where Self: Sized {
///         tt_riingd::drivers::registry::HardwareInfo { 
///             vid: 0x1234, pids: vec![0x5678], channel_count: 4, 
///             name: "Mock".to_string(),
///             create_fallback_config: |_| todo!(),
///         }
///     }
/// }
/// impl std::fmt::Debug for MockController {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "MockController") }
/// }
/// ```
#[async_trait]
pub trait FanController: Send + Sync + core::fmt::Debug {
    /// Initializes the controller hardware.
    async fn send_init(&self) -> Result<()>;

    /// Updates multiple channels with a batch of (temperature, speed) pairs.
    async fn update_speed_batch(&self, batch: &[(usize, u8)]) -> Result<()>;

    /// Updates multiple channels with a batch of (channel index, temperature, speed) tuples.
    async fn update_color_batch(&self, batch: &[(usize, &[(u8, u8, u8)])]) -> Result<()>;

    /// Returns the firmware version as (major, minor, patch).
    async fn firmware_version(&self) -> Result<(u8, u8, u8)>;

    async fn get_device_info(&self) -> Result<DeviceInfo>;

    /// Returns the number of LEDs controlled by this controller.
    fn led_count(&self) -> usize;

    /// Get controller ID
    async fn get_id(&self) -> String;

    async fn get_fingerprint(&self) -> Result<super::HardwareFingerprint>;

    fn hardware_info() -> HardwareInfo
    where
        Self: Sized;
}
