use std::collections::HashMap;
use anyhow::{anyhow, Result};
#[cfg(debug_assertions)]
use log::info;

use crate::fan_curve::{FanCurve, Point};

use super::{device_io::DeviceIO, protocol::{Command, Response}};

pub const READ_TIMEOUT: i32 = 250;
const MAX_ITERATIONS: usize = 100;
const EPSILON: f32 = 1e-6;

#[derive(Debug)]
pub struct Fan {
    pub current_speed: u8,
    pub current_rpm: u16,
    pub active_curve: String,
    pub curve: HashMap<String, FanCurve>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Controller<Io: DeviceIO> {
    pub name: String,
    pub dev: Io,
    pub fans: Vec<Fan>,
}

impl<Io: DeviceIO> Controller<Io> {
    fn request(&self, cmd: Command) -> Result<Response> {
        let pkt = cmd.to_bytes();
        self.dev.write(&pkt)?;
        let mut buf = vec![0u8; cmd.expected_response_len()];
        self.dev
            .read(&mut buf, READ_TIMEOUT)
            .map_err(|e| anyhow!("{e}"))?;
        Response::parse(cmd, &buf)
    }

    pub fn init(&self) -> Result<()> {
        match self.request(Command::Init)? {
            Response::Status(0xFC) => Ok(()),
            _ => Err(anyhow!("Invalid init response")),
        }
    }

    pub fn get_firmware_version(&self) -> Result<(u8, u8, u8)> {
        match self.request(Command::GetFirmwareVersion)? {
            Response::FirmwareVersion {
                major,
                minor,
                patch,
            } => Ok((major, minor, patch)),
            _ => Err(anyhow!("Invalid firmware version responce")),
        }
    }

    pub fn set_speed(&self, port: u8, speed: u8) -> Result<()> {
        match self.request(Command::SetSpeed { port, speed })? {
            Response::Status(0xFC) => Ok(()),
            _ => Err(anyhow!("Invalid set speed responce")),
        }
    }

    pub fn get_data(&self, port: u8) -> Result<(u8, u16)> {
        match self.request(Command::GetData { port })? {
            Response::Data { speed, rpm } => Ok((speed, rpm)),
            _ => Err(anyhow!("Invalid get speed responce")),
        }
    }

    pub fn set_rgb(&self, port: u8, mode: u8, colors: Vec<(u8, u8, u8)>) -> Result<()> {
        match self.request(Command::SetRgb { port, mode, colors })? {
            Response::Status(0xFC) => Ok(()),
            _ => Err(anyhow!("Invalid set rgb responce")),
        }
    }
}

impl Fan {
    pub fn compute_speed(&self, temp: f32) -> Result<u8> {
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

    pub fn update_stats(&mut self, speed: u8, rpm: u16) {
        self.current_rpm = rpm;
        self.current_speed = speed;
    }

    pub fn update_curve(&mut self, curve: &str) -> Result<()> {
        self.curve
            .get(curve)
            .map(|_| {
                self.active_curve = curve.to_string();
                Ok(())
            })
            .ok_or(anyhow!("Curve {curve} not found"))?
    }

    pub fn update_curve_data(&mut self, curve: &str, curve_data: &FanCurve) -> Result<()> {
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

    pub fn get_active_curve(&self) -> Result<String> {
        Ok(self.active_curve.clone())
    }
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

    let p = compute_bezier_at_t(pts, t_mid);
    p.y
}
