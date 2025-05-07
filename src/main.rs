extern crate log;

use anyhow::{Context, Result};
use daemonize::Daemonize;
use hidapi::{DeviceInfo, HidApi, HidDevice};
use log::{LevelFilter, info};
use std::{env, error::Error, fs::File, future::pending, sync::{Arc, Mutex}};
use syslog::{BasicLogger, Facility, Formatter3164};
use zbus::{connection, interface};

const VID: u16 = 0x264A; // Thermaltake

#[derive(Clone)]
struct Controller {
    dev: Arc<Mutex<HidDevice>>,
}
impl Controller{
    fn set_speed(&self, speed: u8) -> Result<()> {
        let dev = self.dev.lock().unwrap();
        for channel in 1..5 {
            dev.write(&build_package(channel, speed))?;
        }
        Ok(())
    }
}

struct DBusInterface {
    controller: Vec<Controller>,
}

#[interface(name = "io.github.tt_riingd1")]
impl DBusInterface {
    fn set_speed(&self, speed: u8) {
        self.controller
            .iter()
            .for_each(|device| device.set_speed(speed).unwrap());
        info!(target: "tt_riing_rs", "SetSpeed: {}", speed);
    }
}

fn build_package(channel: u8, value: u8) -> [u8; 6] {
    [0x00, 0x32, 0x51, channel, 0x01, value]
}

fn all_devices_by_pid(api: &HidApi) -> Vec<DeviceInfo> {
    let devices = api.device_list();
    let mut tt_devices: Vec<DeviceInfo> = Vec::new();
    devices.for_each(|device| {
        if device.vendor_id() == VID {
            println!(
                "{:?}, PID: {:04X}",
                device.product_string().unwrap_or("Unknown"),
                device.product_id()
            );
            tt_devices.push(device.clone());
        }
    });

    tt_devices
}

fn open_devices(api: &HidApi, devs: &[DeviceInfo]) -> Result<Vec<HidDevice>> {
    let mut tt_devices: Vec<HidDevice> = Vec::new();
    devs.iter().for_each(|dev| {
        let opened_dev = match api.open(dev.vendor_id(), dev.product_id()) {
            Ok(device) => device,
            Err(_) => panic!("Failed to open device: {:?}", dev),
        };

        tt_devices.push(opened_dev);
    });

    Ok(tt_devices)
}

fn send_init(devs: &[Controller]) -> Result<()> {
    devs.iter().for_each(|device| {
        let dev = device.dev.lock().unwrap();
        let _ = dev.write(&[0x00, 0xFE, 0x33]); // Send init packet
    });
    Ok(())
}

fn set_pwm(dev: &[Controller], percent: u8) -> Result<()> {
    dev.iter().for_each(|device| {
        for i in 1..5 {
            let _ = device.dev.lock().unwrap().write(&build_package(i, percent));
        }
    });
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let percent: u8 = env::var("TT_PERCENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    let formatter = Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: None,
        process: "tt_riing_rs".into(),
        pid: 0,
    };
    let logger = syslog::unix(formatter).context("open syslog")?;

    log::set_boxed_logger(Box::new(BasicLogger::new(logger)))
        .map(|()| log::set_max_level(LevelFilter::Info))?;

    let stdout = File::create("/var/tmp/tt_riingd.log")?;
    let stderr = stdout.try_clone()?;
    Daemonize::new()
        .stdout(stdout)
        .stderr(stderr)
        .start()?;

    let api = HidApi::new().context("hidapi init")?;
    let tt_devs_info = all_devices_by_pid(&api);
    let devs = open_devices(&api, &tt_devs_info)?;
    let mut controllers: Vec<Controller> = Vec::new();
    for info in devs {
        let ctrl = Controller { dev: Arc::new(Mutex::new(info)) };
        controllers.push(ctrl);

    }
    send_init(&controllers)?;
    set_pwm(&controllers, percent)?;

    info!("старт — {} %", percent);

    let dbus = DBusInterface { controller: controllers.clone() };
    let _conn = connection::Builder::session()?
        .name("io.github.tt_riingd")?
        .serve_at("/io/github/tt_riingd", dbus)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
