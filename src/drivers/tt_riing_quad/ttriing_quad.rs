use crate::fan_curve::FanCurve;
use crate::{config::ControllerCfg, fan_controller::FanController};
use std::{collections::HashMap, sync::Arc};

use anyhow::{Ok, Result, anyhow};
use async_trait::async_trait;
use hidapi::{HidApi, HidDevice};
use log::info;
use tokio::sync::{Mutex, MutexGuard};

use super::controller::{Controller, Fan};

/// Thermaltake Vendor ID for HID devices.
pub const VID: u16 = 0x264A; // Thermaltake

/// Default fan speed percentage used during initialization.
pub const DEFAULT_PERCENT: u8 = 50;

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
/// use tt_riingd::fan_controller::FanController;
///
/// # async fn example() -> anyhow::Result<()> {
/// let api = HidApi::new()?;
/// let controllers = TTRiingQuad::probe(&api, 50)?;
///
/// for controller in controllers {
///     controller.send_init().await?;
///     controller.update_speeds(45.0).await?;
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct TTRiingQuad(Arc<Mutex<Controller<HidDevice>>>);

#[async_trait]
impl FanController for TTRiingQuad {
    async fn send_init(&self) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            info!("Initializing TTRiingQuad controller");
        }
        self.read().await.init()
    }

    async fn update_speeds(&self, temp: f32) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            info!("Updating speeds for TTRiingQuad controller");
        }
        for idx in 0..5 {
            self.process_fan(idx, temp).await?;
        }
        Ok(())
    }

    async fn update_channel(&self, channel: u8, temp: f32) -> Result<()> {
        self.process_fan((channel - 1) as usize, temp).await
    }

    async fn update_channel_color(&self, channel: u8, red: u8, green: u8, blue: u8) -> Result<()> {
        self.process_fan_color((channel - 1) as usize, green, red, blue)
            .await
    }
    async fn switch_curve(&self, channel: u8, curve: &str) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            info!(
                "Switching curve for TTRiingQuad controller on channel {}",
                channel
            );
        }
        self.read()
            .await
            .fans
            .get_mut((channel - 1) as usize)
            .map(|fan| fan.update_curve(curve))
            .ok_or(anyhow! {"Fan not found"})?
    }

    async fn get_active_curve(&self, channel: u8) -> Result<String> {
        #[cfg(debug_assertions)]
        {
            info!(
                "Getting active curve for TTRiingQuad controller on channel {}",
                channel
            );
        }
        self.read()
            .await
            .fans
            .get((channel - 1) as usize)
            .map(|fan| fan.get_active_curve())
            .ok_or(anyhow!("Fans not found"))?
    }

    async fn firmware_version(&self) -> Result<(u8, u8, u8)> {
        self.read().await.get_firmware_version()
    }

    async fn update_curve_data(
        &self,
        channel: u8,
        curve: &str,
        curve_data: &FanCurve,
    ) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            info!(
                "Updating curve data for TTRiingQuad controller on channel {}",
                channel
            );
        }
        self.read()
            .await
            .fans
            .get_mut((channel - 1) as usize)
            .map(|fan| fan.update_curve_data(curve, curve_data))
            .ok_or(anyhow!("Fans not found"))?
    }
}

impl TTRiingQuad {
    /// Auto-detects and creates controllers for all connected Thermaltake devices.
    ///
    /// Scans for all HID devices with Thermaltake vendor ID and creates
    /// controller instances with default configuration.
    ///
    /// # Arguments
    ///
    /// * `api` - HID API instance for device communication
    /// * `speed` - Initial fan speed percentage (0-100)
    ///
    /// # Returns
    ///
    /// A vector of boxed FanController trait objects for each detected device.
    ///
    /// # Errors
    ///
    /// Returns an error if HID communication fails during device enumeration.
    pub fn probe(api: &HidApi, speed: u8) -> Result<Vec<Box<dyn FanController>>> {
        Ok(api
            .device_list()
            .filter(|d| d.vendor_id() == VID)
            .inspect(|d| info!("{:?} device PID={:04X}", d.product_string(), d.product_id()))
            .enumerate()
            .filter_map(|(idx, d)| {
                api.open(d.vendor_id(), d.product_id()).ok().map(|device| {
                    Box::new(TTRiingQuad(Arc::new(Mutex::new(Controller {
                        name: format!("TTRiingQuad: {}", idx + 1),
                        dev: device,
                        fans: (0..5)
                            .map(|_| Fan {
                                current_speed: speed,
                                current_rpm: 0,
                                active_curve: String::from("Constant"),
                                curve: build_default_curves(),
                            })
                            .collect(),
                    })))) as Box<dyn FanController>
                })
            })
            .collect())
    }

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
        curve_map: &HashMap<String, FanCurve>,
    ) -> Result<Vec<Box<dyn FanController>>> {
        Ok(ctrl_cfg
            .iter()
            .filter_map(|cfg| {
                if let ControllerCfg::RiingQuad { id, usb, fans } = cfg {
                    Some(Box::new(TTRiingQuad(Arc::new(Mutex::new(Controller {
                        name: format!("TTRiingQuad{}", id),
                        dev: api.open(usb.vid, usb.pid).unwrap(),
                        fans: fans
                            .iter()
                            .map(|fan| Fan {
                                current_speed: 0,
                                current_rpm: 0,
                                active_curve: fan.active_curve.clone(),
                                curve: fan
                                    .curve
                                    .iter()
                                    .filter_map(|curve_str| {
                                        curve_map
                                            .get(curve_str)
                                            .map(|curve| (curve_str.clone(), curve.clone()))
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })))) as Box<dyn FanController>)
                } else {
                    None
                }
            })
            .collect())
    }

    async fn process_fan(&self, idx: usize, temp: f32) -> Result<()> {
        let speed = {
            let guard = self.0.lock().await;
            guard.fans[idx].compute_speed(temp)?
        };
        #[cfg(debug_assertions)]
        {
            info!("Computed speed for fan {}: {}", idx + 1, speed);
        }
        let ctrl = self.0.clone();
        // let (speed, rpm) = tokio::time::timeout(std::time::Duration::from_millis(READ_TIMEOUT as _), async move {
        let (speed, rpm) = tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            #[cfg(debug_assertions)]
            {
                info!(
                    "Processing fan {} on controller {}: {}°C",
                    idx + 1,
                    guard.name,
                    temp
                );
            }
            Self::proccess_fan_inner(guard, idx, speed)
        })
        .await??;

        self.0.lock().await.fans[idx].update_stats(speed, rpm);
        Ok(())
    }

    async fn process_fan_color(&self, idx: usize, green: u8, red: u8, blue: u8) -> Result<()> {
        let ctrl = self.0.clone();

        tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            Self::proccess_fan_inner_color(guard, idx, green, red, blue)
        })
        .await??;

        Ok(())
    }

    async fn read(&self) -> MutexGuard<'_, Controller<HidDevice>> {
        self.0.lock().await
    }

    fn proccess_fan_inner(
        guard: MutexGuard<'_, Controller<HidDevice>>,
        idx: usize,
        speed: u8,
    ) -> Result<(u8, u16)> {
        guard.set_speed((idx + 1) as u8, speed)?;
        guard.get_data((idx + 1) as u8)
    }

    fn proccess_fan_inner_color(
        guard: MutexGuard<'_, Controller<HidDevice>>,
        idx: usize,
        green: u8,
        red: u8,
        blue: u8,
    ) -> Result<()> {
        guard.set_rgb((idx + 1) as u8, 0x24, vec![(green, red, blue); 52])
    }
}

/// Creates default fan curves for controller initialization.
///
/// Provides standard curve definitions including constant speed,
/// step-based curves, and Bezier curves for initial controller setup.
fn build_default_curves() -> HashMap<String, FanCurve> {
    HashMap::from([
        (
            String::from("Constant"),
            FanCurve::Constant(DEFAULT_PERCENT),
        ),
        (
            String::from("StepCurve"),
            FanCurve::StepCurve {
                temps: (0..=100).step_by(5).map(|t| t as f32).collect(),
                speeds: (0..=100).step_by(5).map(|s| s as u8).collect(),
            },
        ),
        (
            String::from("BezierCurve"),
            FanCurve::BezierCurve {
                points: [(0., 0.), (40., 60.), (60., 40.), (100., 100.)]
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            },
        ),
    ])
}
