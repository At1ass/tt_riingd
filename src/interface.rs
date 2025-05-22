use std::collections::HashMap;

use event_listener::Event;
use log::error;
use serde_json::from_str;
use zbus::{interface, object_server::SignalEmitter};

use crate::controller::Controllers;
use crate::fan_curve::FanCurve;

pub struct DBusInterface {
    pub controllers: Controllers,

    // Events
    pub stop: Event,
    pub version: String,
}

#[interface(name = "io.github.tt_riingd1")]
impl DBusInterface {
    #[zbus(signal)]
    async fn stopped(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn temperature_changed(
        emitter: &SignalEmitter<'_>,
        sensor_data: HashMap<String, f32>,
    ) -> zbus::Result<()>;

    async fn stop(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        emitter.stopped().await?;
        self.stop.notify(1);

        Ok(())
    }

    #[zbus(property)]
    async fn version(&self) -> String {
        self.version.clone()
    }

    async fn switch_active_curve(&self, controller: u8, channel: u8, curve: String) {
        if let Err(e) = self
            .controllers
            .switch_curve(controller, channel, &curve)
            .await
        {
            error!("{e}")
        }
    }

    async fn get_active_curve(&self, controller: u8, channel: u8) -> zbus::fdo::Result<String> {
        self.controllers
            .get_active_curve(controller, channel)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Curve not found: {e}")))
    }

    async fn get_firmware_version(&self, controller: u8) -> zbus::fdo::Result<String> {
        self.controllers
            .get_firmware_version(controller)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Curve not found: {e}")))
            .map(|(mj, mi, pa)| format!("{mj}.{mi}.{pa}"))
    }
    async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: &str,
    ) -> zbus::fdo::Result<()> {
        let fan_curve: FanCurve = from_str(curve_data)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid curve data: {e}")))?;
        self.controllers
            .update_curve_data(controller, channel, curve, &fan_curve)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to update curve data: {e}")))
    }
}
