//! Hardware controller management for Thermaltake Riing fans.
//!
//! Provides high-level interface for controlling fan speed and RGB lighting
//! through HID communication with Thermaltake devices.

use std::{
    collections::HashMap,
    slice::Iter as SliceIter,
    sync::{Arc, LazyLock},
};

use anyhow::{Ok, Result, anyhow};
use futures::stream::{Iter as FutureIter, StreamExt, iter};
use hidapi::HidApi;

use crate::{config::Config, drivers, fan_controller::FanController, fan_curve::FanCurve};

/// Thread-safe collection of fan controllers.
///
/// Manages multiple hardware fan controllers and provides a unified interface
/// for controlling fan speeds, RGB lighting, and curve management across all
/// connected devices.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::controller::Controllers;
/// use tt_riingd::config::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::default();
/// let controllers = Controllers::init_from_cfg(&config)?;
///
/// // Initialize all controllers
/// controllers.send_init().await?;
///
/// // Update fan speed based on temperature
/// controllers.update_channel(1, 1, 45.0).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Controllers(Arc<Vec<Box<dyn FanController>>>);

static HIDAPI: LazyLock<Option<HidApi>> = LazyLock::new(|| match HidApi::new() {
    std::result::Result::Ok(api) => {
        log::info!("HID API initialized successfully");
        Some(api)
    }
    std::result::Result::Err(e) => {
        log::warn!(
            "HID API unavailable: {}. Hardware control will be disabled.",
            e
        );
        None
    }
});

impl Controllers {
    /// Creates a new Controllers instance with auto-detected hardware.
    ///
    /// Automatically detects and initializes all connected Thermaltake devices
    /// with the specified initial fan speed.
    ///
    /// # Arguments
    ///
    /// * `init_speed` - Initial fan speed percentage (0-100)
    ///
    /// # Errors
    ///
    /// Returns an error if HID initialization fails or no devices are found.
    #[allow(dead_code)]
    pub fn init(init_speed: u8) -> Result<Self> {
        let mut controllers = Vec::<Box<dyn FanController>>::new();

        match HIDAPI.as_ref() {
            Some(hidapi) => {
                controllers.extend(drivers::tt_riing_quad::TTRiingQuad::probe(
                    hidapi, init_speed,
                )?);
            }
            None => {
                log::warn!("HID API not available, no hardware controllers will be initialized");
            }
        }

        Ok(Self(Arc::new(controllers)))
    }

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
        let curve_map: HashMap<String, FanCurve> = cfg
            .curves
            .iter()
            .map(|c| (c.get_id(), FanCurve::from(c)))
            .collect();

        match HIDAPI.as_ref() {
            Some(hidapi) => {
                controllers.extend(drivers::tt_riing_quad::TTRiingQuad::find_controllers(
                    hidapi,
                    &cfg.controllers,
                    &curve_map,
                )?);
            }
            None => {
                log::warn!("HID API not available, no hardware controllers will be initialized");
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
    pub async fn update_channel(&self, controller: u8, channel: u8, temp: f32) -> Result<()> {
        self.get_device(controller)?
            .update_channel(channel, temp)
            .await
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

    /// Switches the active fan curve for a specific channel.
    ///
    /// # Arguments
    ///
    /// * `controller` - Controller index (1-based)
    /// * `channel` - Fan channel on the controller (1-based)  
    /// * `curve` - Name of the curve to activate
    ///
    /// # Errors
    ///
    /// Returns an error if the controller/channel is not found or curve doesn't exist.
    pub async fn switch_curve(&self, controller: u8, channel: u8, curve: &str) -> Result<()> {
        self.get_device(controller)?
            .switch_curve(channel, curve)
            .await
    }

    /// Gets the name of the currently active curve for a channel.
    ///
    /// # Arguments
    ///
    /// * `controller` - Controller index (1-based)
    /// * `channel` - Fan channel on the controller (1-based)
    ///
    /// # Returns
    ///
    /// The name of the currently active curve.
    ///
    /// # Errors
    ///
    /// Returns an error if the controller/channel is not found.
    pub async fn get_active_curve(&self, controller: u8, channel: u8) -> Result<String> {
        self.get_device(controller)?.get_active_curve(channel).await
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

    /// Updates curve data for a specific channel.
    ///
    /// # Arguments
    ///
    /// * `controller` - Controller index (1-based)
    /// * `channel` - Fan channel on the controller (1-based)
    /// * `curve` - Name of the curve to update
    /// * `curve_data` - New curve configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the controller/channel is not found or update fails.
    pub async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: &FanCurve,
    ) -> Result<()> {
        self.get_device(controller)?
            .update_curve_data(channel, curve, curve_data)
            .await
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
}
