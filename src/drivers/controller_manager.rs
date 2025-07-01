//! Hardware controller management for Thermaltake Riing fans.
//!
//! Provides high-level interface for controlling fan speed and RGB lighting
//! through HID communication with Thermaltake devices.

use std::{
    slice::Iter as SliceIter,
    sync::{Arc, LazyLock},
};

use anyhow::{Ok, Result, anyhow};
use futures::stream::{Iter as FutureIter, StreamExt, iter};
use hidapi::HidApi;
use tracing::{info, warn};

use crate::{config::Config, drivers, drivers::fan_controller::FanController};

/// Thread-safe collection of fan controllers.
///
/// Manages multiple hardware fan controllers and provides a unified interface
/// for controlling fan speeds, RGB lighting, and curve management across all
/// connected devices.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::drivers::controller_manager::ControllerManager;
/// use tt_riingd::config::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::default();
/// let controllers = ControllerManager::init_from_cfg(&config)?;
///
/// // Initialize all controllers
/// controllers.send_init().await?;
///
/// // Update fan speed based on temperature
/// controllers.update_channel(1, 1, 45.0, 50).await?;
/// # Ok(())
/// # }
/// ```
type ControllerColorBuffer = Vec<(usize, Vec<(u8, u8, u8)>)>;

#[derive(Debug, Clone)]
pub struct ControllerManager(Arc<Vec<Box<dyn FanController>>>);

static HIDAPI: LazyLock<Option<HidApi>> = LazyLock::new(|| match HidApi::new() {
    std::result::Result::Ok(api) => {
        info!("HID API initialized successfully");
        Some(api)
    }
    std::result::Result::Err(e) => {
        warn!(
            "HID API unavailable: {}. Hardware control will be disabled.",
            e
        );
        None
    }
});

impl ControllerManager {
    /// Creates empty Controllers for testing purposes.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self(Arc::new(vec![]))
    }

    /// Creates Controllers from configuration file.
    ///
    /// Initializes controllers based on the provided configuration, including
    /// device selection, fan curves, and initial settings.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Configuration containing controller and curve definitions
    ///
    /// # Errors
    ///
    /// Returns an error if device initialization fails or configuration is invalid.
    pub fn init_from_cfg(cfg: &Config) -> Result<Self> {
        let mut controllers = Vec::<Box<dyn FanController>>::new();

        match HIDAPI.as_ref() {
            Some(hidapi) => {
                controllers.extend(drivers::tt_riing_quad::TTRiingQuad::find_controllers(
                    hidapi,
                    &cfg.controllers,
                )?);
            }
            None => {
                warn!("HID API not available, no hardware controllers will be initialized");
            }
        }

        Ok(Self(Arc::new(controllers)))
    }

    /// Initializes all connected controllers.
    ///
    /// Sends initialization commands to all hardware controllers.
    /// Must be called before other operations.
    ///
    /// # Errors
    ///
    /// Returns an error if any controller fails to initialize.
    pub async fn send_init(&self) -> Result<()> {
        self.async_iter()
            .fold(Ok(()), |acc, device| async {
                acc.and(device.send_init().await)
            })
            .await
    }

    /// Updates fan speed for a specific channel based on temperature.
    ///
    /// # Arguments
    ///
    /// * `controller` - Controller index (1-based)
    /// * `channel` - Fan channel on the controller (1-based)
    /// * `temp` - Current temperature in Celsius
    ///
    /// # Errors
    ///
    /// Returns an error if the controller/channel is not found or update fails.
    pub async fn update_channel(
        &self,
        controller: u8,
        channel: u8,
        temp: f32,
        speed: u8,
    ) -> Result<()> {
        self.get_device(controller)?
            .update_channel(channel, temp, speed)
            .await
    }

    pub async fn update_channel_batch(
        &self,
        controller: u8,
        batch: Vec<(usize, f32, u8)>,
    ) -> Result<()> {
        self.get_device(controller)?.update_speed_batch(batch).await
    }
    /// Updates RGB color for a specific fan channel.
    ///
    /// # Arguments
    ///
    /// * `controller` - Controller index (1-based)
    /// * `channel` - Fan channel on the controller (1-based)
    /// * `red` - Red component (0-255)
    /// * `green` - Green component (0-255)
    /// * `blue` - Blue component (0-255)
    ///
    /// # Errors
    ///
    /// Returns an error if the controller/channel is not found or update fails.
    pub async fn update_channel_color(
        &self,
        controller: u8,
        channel: u8,
        red: u8,
        green: u8,
        blue: u8,
    ) -> Result<()> {
        self.get_device(controller)?
            .update_channel_color(channel, red, green, blue)
            .await
    }

    pub async fn update_channel_color_batch(
        &self,
        controller: u8,
        batch: ControllerColorBuffer,
    ) -> Result<()> {
        self.get_device(controller)?.update_color_batch(batch).await
    }
    /// Gets the firmware version of a specific controller.
    ///
    /// # Arguments
    ///
    /// * `controller` - Controller index (1-based)
    ///
    /// # Returns
    ///
    /// A tuple containing (major, minor, patch) version numbers.
    ///
    /// # Errors
    ///
    /// Returns an error if the controller is not found or communication fails.
    pub async fn get_firmware_version(&self, controller: u8) -> Result<(u8, u8, u8)> {
        self.get_device(controller)?.firmware_version().await
    }

    #[allow(clippy::borrowed_box)]
    fn get_device(&self, controller: u8) -> Result<&Box<dyn FanController>> {
        self.0
            .iter()
            .enumerate()
            .find(|(idx, _)| idx + 1 == controller as usize)
            .map(|(_, device)| device)
            .ok_or(anyhow!("Device `{controller}` not found"))
    }

    fn async_iter(&self) -> FutureIter<SliceIter<'_, Box<dyn FanController>>> {
        iter(self.0.iter())
    }

    pub fn controller_led_count(&self, controller: u8) -> Result<usize> {
        self.get_device(controller)
            .map(|dev| dev.led_count())
            .map_err(|e| anyhow!("Failed to get LED count for controller {}: {}", controller, e))
    }
}
