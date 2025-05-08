use std::sync::Arc;

use anyhow::{Result, anyhow};
use hidapi::{HidApi, HidDevice};
use log::info;
use tokio::sync::Mutex;

pub const VID: u16 = 0x264A; // Thermaltake
pub const DEFAULT_PERCENT: u8 = 50;
pub const INIT_PACKET: [u8; 3] = [0x00, 0xFE, 0x33];

#[derive(Debug)]
struct Controller {
    dev: HidDevice,
}

#[derive(Debug, Clone)]
pub struct Controllers(Arc<Mutex<Vec<Controller>>>);

impl Controllers {
    pub fn init() -> Result<Self> {
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
                        devices.into_iter().map(|dev| Controller { dev }).collect(),
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

    pub async fn set_pwm(&self, percent: u8) -> Result<()> {
        self.0
            .lock()
            .await
            .iter()
            .try_fold((), |_, device| device.set_speed(percent))
    }
}

impl Controller {
    fn set_speed(&self, speed: u8) -> Result<()> {
        (1..5).try_fold((), |_, channel| {
            self.dev
                .write(&build_package(channel, speed))
                .map(|_| ())
                .map_err(|e| anyhow!("{e}"))
        })
    }
}

pub fn build_package(channel: u8, value: u8) -> [u8; 6] {
    [0x00, 0x32, 0x51, channel, 0x01, value]
}
