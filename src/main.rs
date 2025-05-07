use std::{fs::File, future::pending, sync::Arc};

use anyhow::{Context, Result, anyhow};
use daemonize::Daemonize;
use hidapi::{HidApi, HidDevice};
use log::{LevelFilter, error, info};
use syslog::{BasicLogger, Facility, Formatter3164};
use tokio::sync::Mutex;
use zbus::{connection, interface};

const VID: u16 = 0x264A; // Thermaltake
const DEFAULT_PERCENT: u8 = 50;
const INIT_PACKET: [u8; 3] = [0x00, 0xFE, 0x33];

struct Controller {
    dev: HidDevice,
}

struct Controllers(Arc<Mutex<Vec<Controller>>>);

impl Controllers {
    async fn send_init(&self) -> Result<()> {
        let r_guard = self.0.lock().await;

        for device in r_guard.as_slice() {
            let _ = device.dev.write(&INIT_PACKET); // Send init packet    
        }

        Ok(())
    }

    async fn set_pwm(&self, percent: u8) -> Result<()> {
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

impl From<Vec<HidDevice>> for Controllers {
    fn from(value: Vec<HidDevice>) -> Self {
        Self(Arc::new(Mutex::new(
            value.into_iter().map(|dev| Controller { dev }).collect(),
        )))
    }
}

struct DBusInterface {
    controllers: Controllers,
}

#[interface(name = "io.github.tt_riingd1")]
impl DBusInterface {
    async fn set_speed(&self, speed: u8) {
        if let Err(e) = self.controllers.set_pwm(speed).await {
            error!("{e}");
        }
    }
}

fn build_package(channel: u8, value: u8) -> [u8; 6] {
    [0x00, 0x32, 0x51, channel, 0x01, value]
}

fn open_devices(api: &HidApi) -> Vec<HidDevice> {
    api.device_list()
        .filter(|device| device.vendor_id() == VID)
        .inspect(|device| {
            println!(
                "{:?}, PID: {:04X}",
                device.product_string().unwrap_or("Unknown"),
                device.product_id()
            )
        })
        .filter_map(|dev| api.open(dev.vendor_id(), dev.product_id()).ok())
        .collect()
}

fn init_log() -> Result<()> {
    syslog::unix(Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: "tt_riing_rs".into(),
        pid: 0,
    })
    .map_err(|e| anyhow!("{e}"))
    .and_then(|logger| {
        log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
            .map(|_| log::set_max_level(LevelFilter::Info))
            .map_err(|e| anyhow!("{e}"))
    })
}

fn into_daemon() -> Result<()> {
    File::create("/var/tmp/tt_riingd.log")
        .and_then(|out| Ok((out.try_clone()?, out)))
        .map_err(|e| anyhow!("{e}"))
        .and_then(|(stderr, stdout)| {
            Daemonize::new()
                .stdout(stdout)
                .stderr(stderr)
                .start()
                .map_err(|e| anyhow!("{e}"))
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    init_log().and(into_daemon())?;

    let controllers: Controllers = HidApi::new()
        .context("hidapi init")
        .map(|api| open_devices(&api).into())?;

    // First set
    controllers
        .send_init()
        .await
        .and(controllers.set_pwm(DEFAULT_PERCENT).await)?;

    info!("старт — {DEFAULT_PERCENT} %",);

    let _conn = connection::Builder::session()?
        .name("io.github.tt_riingd")?
        .serve_at("/io/github/tt_riingd", DBusInterface { controllers })?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
