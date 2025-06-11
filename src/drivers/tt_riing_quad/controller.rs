use anyhow::{Result, anyhow};
#[cfg(debug_assertions)]
use log::info;
use std::collections::HashMap;

use crate::fan_curve::{FanCurve, Point};

use super::{
    device_io::DeviceIO,
    protocol::{Command, Response},
};

/// HID communication timeout in milliseconds.
pub const READ_TIMEOUT: i32 = 250;

/// Maximum iterations for Bezier curve computation.
const MAX_ITERATIONS: usize = 100;

/// Precision epsilon for Bezier curve calculations.
const EPSILON: f32 = 1e-6;

/// Individual fan state and configuration.
///
/// Represents a single fan connected to a controller, including its current
/// status, active curve configuration, and available speed curves.
#[derive(Debug)]
pub struct Fan {
    /// Current fan speed percentage (0-100).
    pub current_speed: u8,

    /// Current fan RPM reading.
    pub current_rpm: u16,

    /// Name of the currently active speed curve.
    pub active_curve: String,

    /// Map of available speed curves by name.
    pub curve: HashMap<String, FanCurve>,
}

/// Hardware controller for managing multiple fans.
///
/// Provides low-level communication with fan controller hardware through
/// the DeviceIO abstraction. Handles protocol communication and fan management.
///
/// # Type Parameters
///
/// * `Io` - Device I/O implementation (typically HidDevice)
#[derive(Debug)]
#[allow(dead_code)]
pub struct Controller<Io: DeviceIO> {
    /// Human-readable controller name for identification.
    pub name: String,

    /// Device I/O interface for hardware communication.
    pub dev: Io,

    /// Vector of fans managed by this controller.
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

    /// Initializes the controller hardware.
    ///
    /// Sends initialization command to prepare the controller for operation.
    /// Must be called before other controller operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the controller fails to initialize or communication fails.
    pub fn init(&self) -> Result<()> {
        match self.request(Command::Init) {
            Ok(Response::Status(0xFC)) => Ok(()),
            Ok(_) => Err(anyhow!("Invalid init response: Expected status 0xFC")),
            Err(e) => Err(anyhow!("Invalid init response: {e}")),
        }
    }

    /// Retrieves the controller firmware version.
    ///
    /// # Returns
    ///
    /// A tuple containing (major, minor, patch) version numbers.
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails or response is invalid.
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

    /// Sets the speed for a specific fan port.
    ///
    /// # Arguments
    ///
    /// * `port` - Fan port number (1-based)
    /// * `speed` - Speed percentage (0-100)
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails or the port is invalid.
    pub fn set_speed(&self, port: u8, speed: u8) -> Result<()> {
        match self.request(Command::SetSpeed { port, speed }) {
            Ok(Response::Status(0xFC)) => Ok(()),
            Ok(_) => Err(anyhow!("Invalid set speed response: Expected status 0xFC")),
            Err(e) => Err(anyhow!("Invalid set speed responce: {e}")),
        }
    }

    /// Reads current speed and RPM data from a fan port.
    ///
    /// # Arguments
    ///
    /// * `port` - Fan port number (1-based)
    ///
    /// # Returns
    ///
    /// A tuple containing (speed_percentage, rpm).
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails or the port is invalid.
    pub fn get_data(&self, port: u8) -> Result<(u8, u16)> {
        match self.request(Command::GetData { port }) {
            Ok(Response::Data { speed, rpm }) => Ok((speed, rpm)),
            Ok(_) => Err(anyhow!("Invalid get data response: Expected Data")),
            Err(e) => Err(anyhow!("Invalid get speed responce: {e}")),
        }
    }

    /// Sets RGB lighting for a specific fan port.
    ///
    /// # Arguments
    ///
    /// * `port` - Fan port number (1-based)
    /// * `mode` - RGB mode (typically 0x24 for static color)
    /// * `colors` - Vector of RGB color tuples (red, green, blue)
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails or parameters are invalid.
    pub fn set_rgb(&self, port: u8, mode: u8, colors: Vec<(u8, u8, u8)>) -> Result<()> {
        match self.request(Command::SetRgb { port, mode, colors }) {
            Ok(Response::Status(0xFC)) => Ok(()),
            Ok(_) => Err(anyhow!("Invalid set rgb response: Expected status 0xFC")),
            Err(e) => Err(anyhow!("Invalid set rgb responce: {e}")),
        }
    }
}

impl Fan {
    /// Computes fan speed based on temperature using the active curve.
    ///
    /// # Arguments
    ///
    /// * `temp` - Current temperature in Celsius
    ///
    /// # Returns
    ///
    /// Computed fan speed percentage (0-100).
    ///
    /// # Errors
    ///
    /// Returns an error if the active curve is not found or computation fails.
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

    /// Updates the fan's current speed and RPM statistics.
    ///
    /// # Arguments
    ///
    /// * `speed` - Current speed percentage
    /// * `rpm` - Current RPM reading
    pub fn update_stats(&mut self, speed: u8, rpm: u16) {
        self.current_rpm = rpm;
        self.current_speed = speed;
    }

    /// Switches to a different speed curve.
    ///
    /// # Arguments
    ///
    /// * `curve` - Name of the curve to activate
    ///
    /// # Errors
    ///
    /// Returns an error if the specified curve is not available.
    pub fn update_curve(&mut self, curve: &str) -> Result<()> {
        self.curve
            .get(curve)
            .map(|_| {
                self.active_curve = curve.to_string();
                Ok(())
            })
            .ok_or(anyhow!("Curve {curve} not found"))?
    }

    /// Updates the data for a specific curve.
    ///
    /// # Arguments
    ///
    /// * `curve` - Name of the curve to update
    /// * `curve_data` - New curve configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the curve is not found.
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

    /// Gets the name of the currently active curve.
    ///
    /// # Returns
    ///
    /// The name of the active curve.
    pub fn get_active_curve(&self) -> Result<String> {
        Ok(self.active_curve.clone())
    }
}

/// Computes a point on a Bezier curve at parameter t.
///
/// # Arguments
///
/// * `pts` - Array of 4 control points defining the Bezier curve
/// * `t` - Parameter value (0.0 to 1.0)
///
/// # Returns
///
/// The computed point on the curve.
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

/// Finds the fan speed for a given temperature using Bezier curve interpolation.
///
/// Uses binary search to find the parameter t where the curve's x-coordinate
/// matches the given temperature, then returns the corresponding y-coordinate.
///
/// # Arguments
///
/// * `pts` - Array of 4 control points defining the Bezier curve
/// * `temp` - Temperature to find speed for
///
/// # Returns
///
/// The interpolated fan speed for the given temperature.
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
