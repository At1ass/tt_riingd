use anyhow::{Result, anyhow};
use arrayvec::ArrayVec;

use crate::buffer::Rgb;

use super::{
    device_io::DeviceIO,
    protocol::{Command, Response},
};

/// HID communication timeout in milliseconds.
pub const READ_TIMEOUT: i32 = 250;

type PacketBuffer = ArrayVec<u8, 193>;

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
pub struct Controller<Io: DeviceIO> {
    /// Human-readable controller name for identification.
    #[allow(dead_code)]
    pub name: String,

    /// Device I/O interface for hardware communication.
    pub dev: Io,

    /// Vector of fans managed by this controller.
    pub fans: Vec<Fan>,
}

impl<Io: DeviceIO> Controller<Io> {
    fn request(&self, cmd: Command) -> Result<Response> {
        let mut pkt = PacketBuffer::new();
        cmd.encode(&mut pkt)
            .map_err(|e| anyhow!("Failed to encode command: {e}"))?;
        self.dev.write(&pkt)?;
        let mut buf = [0u8; 193];
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
    pub fn set_rgb(&self, port: u8, mode: u8, colors: &[Rgb]) -> Result<()> {
        match self.request(Command::SetRgb { port, mode, colors }) {
            Ok(Response::Status(0xFC)) => Ok(()),
            Ok(_) => Err(anyhow!("Invalid set rgb response: Expected status 0xFC")),
            Err(e) => Err(anyhow!("Invalid set rgb responce: {e}")),
        }
    }
}

impl Fan {
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
}
