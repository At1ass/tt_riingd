use crate::{
    buffer::{ColorBufferData, ControllerEntry, Rgb, SpeedBufferData},
    config::{ControllerCfg, FanCfg, UsbSelector},
    drivers::{HardwareFingerprint, fan_controller::FanController, registry::HardwareInfo},
};
use std::sync::Arc;

use anyhow::{Ok, Result, anyhow};
use async_trait::async_trait;
use hidapi::{DeviceInfo, HidApi, HidDevice};
use tokio::sync::{Mutex, MutexGuard};
#[allow(unused_imports)]
use tracing::{debug, info};

use super::controller::{Controller, Fan};

/// Thermaltake Riing Quad fan controller implementation.
///
/// Provides hardware control for Thermaltake Riing Quad fan controllers
/// via HID interface. Supports up to 5 fans per controller with independent
/// speed curves and RGB lighting control.
///
/// # Example
///
/// ```no_run
/// use hidapi::HidApi;
/// use tt_riingd::drivers::tt_riing_quad::TTRiingQuad;
/// use tt_riingd::drivers::fan_controller::FanController;
/// use tt_riingd::config::{ControllerCfg, UsbSelector, FanCfg};
/// use tt_riingd::buffer::{SpeedBufferData, ControllerEntry};
/// use std::sync::Arc;
///
/// # async fn example() -> anyhow::Result<()> {
/// let api = HidApi::new()?;
/// let config = vec![ControllerCfg::RiingQuad {
///     id: "main".to_string(),
///     usb: UsbSelector { vid: 0x264a, pid: 0x2330, serial: None },
///     fans: vec![FanCfg { idx: 1, name: "Fan 1".to_string() }],
/// }];
/// let controllers = TTRiingQuad::find_controllers(&api, &config)?;
/// let speed_data = Arc::new(SpeedBufferData::default());
/// let controller_entry = ControllerEntry::new_for_testing(
///     "main".to_string(),
///     0,
///     4,
///     13,
///     0b1111,
/// );
///
/// for controller in controllers {
///     controller.send_init().await?;
///     controller.update_speed_batch(speed_data.clone(), controller_entry.clone()).await?;
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct TTRiingQuad(Arc<Mutex<Controller<HidDevice>>>);

#[async_trait]
impl FanController for TTRiingQuad {
    async fn send_init(&self) -> Result<()> {
        debug!("Initializing TTRiingQuad controller");
        let ctrl = self.0.clone();
        Self::proccess_init(&ctrl.lock().await)
            .map_err(|e| anyhow!("Failed to initialize TTRiingQuad controller: {e}"))?;

        debug!("TTRiingQuad controller initialized successfully");
        Ok(())
    }

    async fn update_speed_batch(
        &self,
        batch: Arc<SpeedBufferData>,
        controller_entry: ControllerEntry,
    ) -> Result<()> {
        debug!("Batch processing speed");
        let mut guard = self.0.lock().await;
        let result = {
            (1..=5)
                .filter_map(|channel| {
                    if 1u32 << (channel - 1) & controller_entry.active_channels == 0 {
                        return None; // Skip channels not in the mask
                    }
                    let index = controller_entry.offset + (channel - 1);
                    Some(
                        Self::proccess_fan_inner(&guard, channel, batch.data[index])
                            .map(|(speed, rpm)| (channel, speed, rpm)),
                    )
                })
                .collect::<Result<Vec<_>>>()
        }?;

        result.into_iter().for_each(|(channel, speed, rpm)| {
            debug!(
                "Processing fan {}: speed = {}, rpm = {}",
                channel, speed, rpm
            );
            guard.fans[channel - 1].update_stats(speed, rpm);
        });
        debug!("Speed batch processed successfully for TTRiingQuad controller");
        debug!("Fan speeds: {:#?}", guard.fans);

        Ok(())
    }

    async fn update_color_batch(
        &self,
        batch: Arc<ColorBufferData>,
        controller_entry: ControllerEntry,
    ) -> Result<()> {
        debug!("Batch processing colors");
        let guard = self.0.lock().await;
        for channel in 1..=5 {
            if 1u32 << (channel - 1) & controller_entry.active_channels == 0 {
                continue;
            }
            let start = controller_entry.offset + (channel - 1) * 52;
            let end = start + 52;
            Self::proccess_fan_inner_color(&guard, channel, &batch.data[start..end])
                .map_err(|e| anyhow!("Failed to set color for fan {}: {}", channel, e))?;
        }
        Ok(())
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        self.read().await.get_firmware_version()
    }

    async fn get_id(&self) -> String {
        self.read().await.name.clone()
    }

    async fn get_device_info(&self) -> Result<DeviceInfo> {
        let guard = self.read().await;
        guard
            .dev
            .get_device_info()
            .map_err(|e| anyhow!("Failed to get device info for TTRiingQuad: {e}"))
    }

    async fn get_fingerprint(&self) -> Result<HardwareFingerprint> {
        let guard = self.read().await;
        let device_info = guard
            .dev
            .get_device_info()
            .map_err(|e| anyhow!("Failed to get device info for TTRiingQuad: {e}"))?;
        Ok(HardwareFingerprint {
            vendor_id: device_info.vendor_id(),
            product_id: device_info.product_id(),
            serial: device_info.serial_number().map(|s| s.to_string()),
        })
    }

    fn hardware_info() -> HardwareInfo {
        HardwareInfo {
            vid: 0x264a,
            pids: vec![0x232B, 0x232C, 0x232D, 0x232E],
            channel_count: 5,
            led_count: 52,
            name: "TTRiingQuad".to_string(),
            create_fallback_config: Self::create_fallback_config_internal,
        }
    }
}

impl TTRiingQuad {
    /// Creates controllers from configuration specifications.
    ///
    /// Initializes controllers based on the provided configuration, including
    /// specific device selection via USB identifiers and custom fan curves.
    ///
    /// # Arguments
    ///
    /// * `api` - HID API instance for device communication
    /// * `ctrl_cfg` - Array of controller configurations from config file
    /// * `curve_map` - Map of curve names to curve definitions
    ///
    /// # Returns
    ///
    /// A vector of boxed FanController trait objects matching the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if specified devices cannot be opened or configuration is invalid.
    #[allow(irrefutable_let_patterns)]
    pub fn find_controllers(
        api: &HidApi,
        ctrl_cfg: &[ControllerCfg],
    ) -> Result<Vec<Arc<dyn FanController>>> {
        Ok(ctrl_cfg
            .iter()
            .filter_map(|cfg| Self::create_one(api, cfg).ok())
            .collect())
    }

    pub fn create_one(api: &HidApi, cfg: &ControllerCfg) -> Result<Arc<dyn FanController>> {
        #[allow(irrefutable_let_patterns)]
        if let ControllerCfg::RiingQuad { id, usb, fans: _ } = cfg {
            let dev = api
                .open(usb.vid, usb.pid)
                .map_err(|e| anyhow!("Failed to open device: {e}"))?;
            Ok(Arc::new(TTRiingQuad(Arc::new(Mutex::new(Controller {
                name: id.clone(),
                dev,
                fans: (0..5)
                    .map(|_| Fan {
                        current_speed: 0,
                        current_rpm: 0,
                    })
                    .collect(),
            })))))
        } else {
            Err(anyhow!("Invalid configuration for TTRiingQuad"))
        }
    }

    fn create_fallback_config_internal(fingerprint: &HardwareFingerprint) -> ControllerCfg {
        ControllerCfg::RiingQuad {
            id: format!(
                "fallback_{}:{}",
                fingerprint.vendor_id, fingerprint.product_id
            ),
            usb: UsbSelector {
                vid: fingerprint.vendor_id,
                pid: fingerprint.product_id,
                serial: fingerprint.serial.clone(),
            },
            fans: (1..=5)
                .map(|idx| FanCfg {
                    idx,
                    name: format!("Fan {idx}"),
                })
                .collect(),
        }
    }

    async fn read(&self) -> MutexGuard<'_, Controller<HidDevice>> {
        self.0.lock().await
    }

    fn proccess_init(guard: &MutexGuard<'_, Controller<HidDevice>>) -> Result<()> {
        guard.init()
    }

    fn proccess_fan_inner(
        guard: &MutexGuard<'_, Controller<HidDevice>>,
        idx: usize,
        speed: u8,
    ) -> Result<(u8, u16)> {
        guard.set_speed(idx as u8, speed)?;
        guard.get_data(idx as u8)
    }

    fn proccess_fan_inner_color(
        guard: &MutexGuard<'_, Controller<HidDevice>>,
        idx: usize,
        color_buffer: &[Rgb],
    ) -> Result<()> {
        guard.set_rgb(idx as u8, 0x24, color_buffer)
    }
}
