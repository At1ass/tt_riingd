use std::{collections::HashMap, sync::Arc};

use anyhow::{Ok, Result, anyhow};
use hidapi::{HidApi, HidDevice};
use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

pub const VID: u16 = 0x264A; // Thermaltake
pub const DEFAULT_PERCENT: u8 = 50;
pub const INIT_PACKET: [u8; 3] = [0x00, 0xFE, 0x33];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum FanCurve {
    Constant(u8),
    StepCurve { temps: Vec<f32>, speeds: Vec<u8> },
    BezierCurve { points: Vec<(f32, f32)> },
}

#[derive(Debug)]
struct Fan {
    current_speed: u8,
    current_rpm: u32,
    active_curve: String,
    curve: HashMap<String, FanCurve>,
}

#[derive(Debug)]
struct Controller {
    dev: HidDevice,
    fans: Vec<Fan>,
}

#[derive(Debug, Clone)]
pub struct Controllers(Arc<Mutex<Vec<Controller>>>);

impl Controllers {
    pub fn init(speed: u8) -> Result<Self> {
        Ok(Self(
            HidApi::new()
                .map(|api| {
                    api.device_list()
                        .filter(|device| device.vendor_id() == VID)
                        .inspect(|device| {
                            info!(
                                "{:?}, PID: {:04X}",
                                device.product_string().unwrap_or("Unknown"),
                                device.product_id()
                            )
                        })
                        .filter_map(|dev| api.open(dev.vendor_id(), dev.product_id()).ok())
                        .collect::<Vec<_>>()
                })
                .map(|devices| {
                    Arc::new(Mutex::new(
                        devices
                            .into_iter()
                            .map(|device| Controller {
                                dev: device,
                                fans: (1..=5)
                                    .map(|_| Fan {
                                        current_speed: speed,
                                        current_rpm: 0,
                                        active_curve: String::from("Constant"),
                                        curve: build_default_curves(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    ))
                })?,
        ))
    }

    pub async fn send_init(&self) -> Result<()> {
        let r_guard = self.0.lock().await;

        for device in r_guard.as_slice() {
            let _ = device.dev.write(&INIT_PACKET); // Send init packet
        }

        Ok(())
    }

    pub async fn update_speeds(&self, temp: f32) -> Result<()> {
        self.0
            .lock()
            .await
            .iter_mut()
            .try_for_each(|device| device.update_speeds(temp))
    }

    pub async fn switch_curve(&self, controller: u8, channel: u8, curve: &str) -> Result<()> {
        self.0
            .lock()
            .await
            .get_mut(controller as usize)
            .map(|device| device.switch_curve(channel, curve))
            .ok_or(anyhow!("Controllers not found"))?
    }

    pub async fn get_active_curve(&self, controller: u8, channel: u8) -> Result<String> {
        self.0
            .lock()
            .await
            .get(controller as usize)
            .map(|device| device.get_active_curve(channel))
            .ok_or(anyhow!("Controllers not found"))?
    }

    pub async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: FanCurve,
    ) -> Result<()> {
        self.0
            .lock()
            .await
            .get_mut(controller as usize)
            .map(|device| device.update_curve_data(channel, curve, curve_data))
            .ok_or(anyhow!("Controllers not found"))?
    }
}

impl Controller {
    fn update_speeds(&mut self, temp: f32) -> Result<()> {
        (0..5).try_for_each(|idx| self.process_fan(idx, temp))
    }

    fn process_fan(&mut self, idx: usize, temp: f32) -> Result<()> {
        let speed = self.fans[idx].compute_speed(temp)?;
        info!("Processing fan {}: {}°C", idx + 1, temp);
        self.send_request((idx + 1) as u8, speed)
            .and_then(|_| Ok(self.read_response()))
            .and_then(|(s, rpm)| {
                self.fans[idx].update_stats(s, rpm);
                Ok(())
            })
    }

    fn switch_curve(&mut self, channel: u8, curve: &str) -> Result<()> {
        self.fans
            .get_mut((channel - 1) as usize)
            .map(|fan| fan.update_curve(curve))
            .ok_or(anyhow! {"Fan not found"})?
    }

    fn get_active_curve(&self, channel: u8) -> Result<String> {
        self.fans
            .get((channel - 1) as usize)
            .map(|fan| fan.get_active_curve())
            .ok_or(anyhow!("Fans not found"))?
    }

    fn update_curve_data(&mut self, channel: u8, curve: &str, curve_data: FanCurve) -> Result<()> {
        self.fans
            .get_mut((channel - 1) as usize)
            .map(|fan| fan.update_curve_data(curve, curve_data))
            .ok_or(anyhow!("Fans not found"))?
    }

    fn send_request(&self, channel: u8, speed: u8) -> Result<()> {
        let _ = self.dev.write(&build_package(channel, speed))?;
        info!("Sent request: channel={}, speed={}", channel, speed);

        Ok(())
    }

    fn read_response(&self) -> (u8, u32) {
        let mut response = [0u8; 193];
        let _ = self.dev.read_timeout(&mut response, 250);
        info!("Received response");
        let speed = response[0x04_usize];
        let rpm: u32 = (response[0x05_usize] as u32) << 8 | response[0x06_usize] as u32;
        (speed, rpm)
    }
}

impl Fan {
    fn compute_speed(&self, temp: f32) -> Result<u8> {
        match self
            .curve
            .get(&self.active_curve)
            .ok_or(anyhow!("Curve not found"))?
        {
            FanCurve::Constant(speed) => Ok(*speed),
            FanCurve::StepCurve { temps, speeds } => temps
                .windows(2)
                .zip(speeds.windows(2))
                .find_map(|(t, w)| {
                    let (t0, t1) = (t[0], t[1]);
                    let (s0, s1) = (w[0], w[1]);
                    if (t0..=t1).contains(&temp) {
                        let ratio = (temp - t0) / (t1 - t0);
                        let speed = s0 as f32 * (1.0 - ratio) + s1 as f32 * ratio;
                        Some(speed.round().clamp(0.0, 100.0) as u8)
                    } else {
                        None
                    }
                })
                .ok_or(anyhow!("Temperature not found in curve")),
            FanCurve::BezierCurve { points: _ } => {
                // Implement Bezier curve interpolation
                Err(anyhow!("Bezier curve interpolation not implemented"))
            }
        }
    }

    fn update_stats(&mut self, speed: u8, rpm: u32) {
        self.current_rpm = rpm;
        self.current_speed = speed;
    }

    fn update_curve(&mut self, curve: &str) -> Result<()> {
        self.curve
            .get(curve)
            .map(|_| {
                self.active_curve = curve.to_string();
                Ok(())
            })
            .ok_or(anyhow!("Curve not found"))?
    }

    fn update_curve_data(&mut self, curve: &str, curve_data: FanCurve) -> Result<()> {
        self.curve
            .get_mut(curve)
            .map(|c| {
                if std::mem::discriminant(c) == std::mem::discriminant(&curve_data) {
                    info!("Disc c: {:?}", std::mem::discriminant(c));
                    info!("Disc curve_data: {:?}", std::mem::discriminant(&curve_data));

                    *c = curve_data;
                    Ok(())
                } else {
                    Err(anyhow!("Incompatible curve data"))
                }
            })
            .ok_or(anyhow!("Curve not found"))?
    }

    fn get_active_curve(&self) -> Result<String> {
        Ok(self.active_curve.clone())
    }
}

pub fn build_package(channel: u8, value: u8) -> [u8; 6] {
    [0x00, 0x32, 0x51, channel, 0x01, value]
}

fn build_default_curves() -> HashMap<String, FanCurve> {
    let mut curves = HashMap::new();
    curves.insert(
        String::from("Constant"),
        FanCurve::Constant(DEFAULT_PERCENT),
    );
    curves.insert(
        String::from("StepCurve"),
        FanCurve::StepCurve {
            temps: (0..=100).step_by(5).map(|t| t as f32).collect(),
            speeds: (0..=100).step_by(5).map(|s| s as u8).collect(),
        },
    );
    curves.insert(
        String::from("BezierCurve"),
        FanCurve::BezierCurve {
            points: vec![(0.0, 0.0), (40.0, 60.0), (60.0, 40.0), (100.0, 100.0)],
        },
    );
    curves
}
