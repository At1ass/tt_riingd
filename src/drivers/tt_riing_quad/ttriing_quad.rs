use crate::{config::ControllerCfg, drivers::fan_controller::FanController};
use std::sync::Arc;

use anyhow::{Context, Ok, Result};
use async_trait::async_trait;
use hidapi::{HidApi, HidDevice};
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
///
/// # async fn example() -> anyhow::Result<()> {
/// let api = HidApi::new()?;
/// let config = vec![ControllerCfg::RiingQuad {
///     id: "main".to_string(),
///     usb: UsbSelector { vid: 0x264a, pid: 0x2330, serial: None },
///     fans: vec![FanCfg { idx: 1, name: "Fan 1".to_string() }],
/// }];
/// let controllers = TTRiingQuad::find_controllers(&api, &config)?;
///
/// for controller in controllers {
///     controller.send_init().await?;
///     controller.update_channel(1, 45.0, 50).await?;
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
        self.read().await.init()
    }

    async fn update_channel(&self, channel: u8, temp: f32, speed: u8) -> Result<()> {
        self.process_fan((channel - 1) as usize, temp, speed).await
    }

    async fn update_speed_batch(&self, batch: Vec<(usize, f32, u8)>) -> Result<()> {
        debug!("Batch processing speed");
        let ctrl = self.0.clone();
        let result = tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            batch
                .into_iter()
                .map(|(idx, _temp, speed)| {
                    Self::proccess_fan_inner(&guard, idx, speed)
                        .map(|(speed, rpm)| (idx, speed, rpm))
                })
                .collect::<Result<Vec<_>>>()
        })
        .await??;

        let mut guard = self.0.lock().await;
        result.into_iter().for_each(|(channel, speed, rpm)| {
            guard.fans[channel - 1].update_stats(speed, rpm);
        });
        Ok(())
    }

    async fn update_channel_color(&self, channel: u8, red: u8, green: u8, blue: u8) -> Result<()> {
        self.process_fan_color((channel - 1) as usize, green, red, blue)
            .await
    }

    async fn update_color_batch(&self, batch: Vec<(usize, Vec<(u8, u8, u8)>)>) -> Result<()> {
        debug!("Batch processing speed");
        let ctrl = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            batch
                .into_iter()
                .try_fold((), |_, (idx, buffer)| {
                    Self::proccess_fan_inner_color(&guard, idx, buffer)
                        .map_err(|e| anyhow::anyhow!("Failed to set color for fan {}: {}", idx, e))
                })
       })
        .await?
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        self.read().await.get_firmware_version()
    }

    fn led_count(&self) -> usize {
        52
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
    ) -> Result<Vec<Box<dyn FanController>>> {
        Ok(ctrl_cfg
            .iter()
            .filter_map(|cfg| {
                if let ControllerCfg::RiingQuad { id, usb, fans } = cfg {
                    let dev = api
                        .open(usb.vid, usb.pid)
                        .context("Failed to open device")
                        .ok()?;
                    Some(Box::new(TTRiingQuad(Arc::new(Mutex::new(Controller {
                        name: format!("TTRiingQuad{id}"),
                        // dev: api.open(usb.vid, usb.pid).unwrap(),
                        dev,
                        fans: fans
                            .iter()
                            .map(|_| Fan {
                                current_speed: 0,
                                current_rpm: 0,
                            })
                            .collect(),
                    })))) as Box<dyn FanController>)
                } else {
                    None
                }
            })
            .collect())
    }

    async fn process_fan(&self, idx: usize, _temp: f32, speed: u8) -> Result<()> {
        let ctrl = self.0.clone();
        let (speed, rpm) = tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            Self::proccess_fan_inner(&guard, idx, speed)
        })
        .await??;

        self.0.lock().await.fans[idx].update_stats(speed, rpm);
        Ok(())
    }

    async fn process_fan_color(&self, idx: usize, green: u8, red: u8, blue: u8) -> Result<()> {
        let ctrl = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            Self::proccess_fan_inner_color(&guard, idx, vec![(red, green, blue)])
        })
        .await??;

        Ok(())
    }

    async fn read(&self) -> MutexGuard<'_, Controller<HidDevice>> {
        self.0.lock().await
    }

    fn proccess_fan_inner(
        guard: &MutexGuard<'_, Controller<HidDevice>>,
        idx: usize,
        speed: u8,
    ) -> Result<(u8, u16)> {
        guard.set_speed((idx + 1) as u8, speed)?;
        guard.get_data((idx + 1) as u8)
    }

    fn proccess_fan_inner_color(
        guard: &MutexGuard<'_, Controller<HidDevice>>,
        idx: usize,
        color_buffer: Vec<(u8, u8, u8)>,
    ) -> Result<()> {
        guard.set_rgb(idx as u8, 0x24, color_buffer)
    }
}
