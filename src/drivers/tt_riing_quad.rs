use crate::fan_curve::{FanCurve, Point};
use crate::{config::ControllerCfg, fan_controller::FanController};
use std::{collections::HashMap, sync::Arc};

use anyhow::{Ok, Result, anyhow};
use async_trait::async_trait;
use hidapi::{HidApi, HidDevice};
use log::info;
use tokio::sync::{Mutex, MutexGuard};

pub const VID: u16 = 0x264A; // Thermaltake
pub const DEFAULT_PERCENT: u8 = 50;
pub const INIT_PACKET: [u8; 3] = [0x00, 0xFE, 0x33];
pub const READ_TIMEOUT: i32 = 250;
const MAX_ITERATIONS: usize = 100;
const EPSILON: f32 = 1e-6;

#[derive(Debug)]
struct Fan {
    current_speed: u8,
    current_rpm: u32,
    active_curve: String,
    curve: HashMap<String, FanCurve>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Controller {
    name: String,
    dev: HidDevice,
    fans: Vec<Fan>,
}

#[derive(Debug)]
pub struct TTRiingQuad(Arc<Mutex<Controller>>);

#[async_trait]
impl FanController for TTRiingQuad {
    async fn send_init(&self) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            info!("Initializing TTRiingQuad controller");
        }
        self.read()
            .await
            .dev
            .write(&INIT_PACKET)
            .map(|_| ())
            .map_err(|e| anyhow!("{e}"))
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
                                            .inspect(|_| info!("Matched: {curve_str}"))
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
        let (ret_speed, rpm) = tokio::task::spawn_blocking(move || {
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
        .await?;
        self.0.lock().await.fans[idx].update_stats(ret_speed, rpm);
        Ok(())
    }

    async fn process_fan_color(&self, idx: usize, green: u8, red: u8, blue: u8) -> Result<()> {
        let ctrl = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let guard = ctrl.blocking_lock();
            #[cfg(debug_assertions)]
            {
                info!("Setting color fan {} on controller {}", idx + 1, guard.name,);
            }
            Self::proccess_fan_inner_color(guard, idx, green, red, blue)
        })
        .await?
    }
    async fn read(&self) -> MutexGuard<'_, Controller> {
        self.0.lock().await
    }

    #[inline(never)]
    fn proccess_fan_inner(guard: MutexGuard<'_, Controller>, idx: usize, speed: u8) -> (u8, u32) {
        let _ = guard.dev.write(&build_package((idx + 1) as u8, speed));

        let mut buf = [0u8; 193];
        let _ = guard.dev.read_timeout(&mut buf, READ_TIMEOUT);

        let s = buf[0x04];
        let rpm = ((buf[0x05] as u32) << 8) | buf[0x06] as u32;

        (s, rpm)
    }

    #[inline(never)]
    fn proccess_fan_inner_color(
        guard: MutexGuard<'_, Controller>,
        idx: usize,
        green: u8,
        red: u8,
        blue: u8,
    ) -> Result<()> {
        let _ = guard
            .dev
            .write(&build_color_package((idx + 1) as u8, green, red, blue));

        let mut buf = [0u8; 193];
        let _ = guard.dev.read_timeout(&mut buf, READ_TIMEOUT);

        Ok(())
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
            FanCurve::BezierCurve { points } => {
                if points.len() != 4 {
                    Err(anyhow!("Bezier curve must have 4 points"))
                } else {
                    Ok(get_speed_for_temp(&points[0..4], temp) as u8)
                }
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
            .ok_or(anyhow!("Curve {curve} not found"))?
    }

    fn update_curve_data(&mut self, curve: &str, curve_data: &FanCurve) -> Result<()> {
        self.curve
            .get_mut(curve)
            .filter(|c| c == &curve_data)
            .map(|c| {
                #[cfg(debug_assertions)]
                {
                    info!("Disc c: {c:?}");
                    info!("Disc curve_data: {c:?}");
                }

                *c = curve_data.clone();
            })
            .ok_or(anyhow!("Curve not found"))
    }

    fn get_active_curve(&self) -> Result<String> {
        Ok(self.active_curve.clone())
    }
}

pub fn build_package(channel: u8, value: u8) -> [u8; 6] {
    [0x00, 0x32, 0x51, channel, 0x01, value]
}

pub fn build_color_package(channel: u8, green: u8, red: u8, blue: u8) -> [u8; 193] {
    let mut package = [0u8; 193];

    package[0x00..=0x04].copy_from_slice(&[0x00, 0x32, 0x52, channel, 0x24]);
    for i in (0..52).step_by(3) {
        package[(0x05 + i)..(0x05 + i + 3)].copy_from_slice(&[green, red, blue]);
    }

    #[cfg(debug_assertions)]
    {
        info!("Built color package: {:?}", package);
    }

    package
}

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

fn compute_bezier_at_t(pts: &[Point], t: f32) -> Point {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    let x = uuu * pts[0].x + 3.0 * uu * t * pts[1].x + 3.0 * u * tt * pts[2].x + ttt * pts[3].x;

    let y = uuu * pts[0].y + 3.0 * uu * t * pts[1].y + 3.0 * u * tt * pts[2].y + ttt * pts[3].y;

    (x, y).into()
}

/// Ищет `y` по заданной `temp` (т.е. по `x`) на кривой Безье
pub fn get_speed_for_temp(pts: &[Point], temp: f32) -> f32 {
    let mut t_low = 0.0_f32;
    let mut t_high = 1.0_f32;
    let mut t_mid = 0.0_f32;

    for _ in 0..MAX_ITERATIONS {
        t_mid = (t_low + t_high) * 0.5;
        let p = compute_bezier_at_t(pts, t_mid);

        if (p.x - temp).abs() < EPSILON {
            return p.y;
        }
        if p.x < temp {
            t_low = t_mid;
        } else {
            t_high = t_mid;
        }
    }

    // по окончании итераций возвращаем последнее y
    let p = compute_bezier_at_t(pts, t_mid);
    p.y
}
