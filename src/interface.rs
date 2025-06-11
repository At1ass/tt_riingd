//! D-Bus interface for external control of the tt_riingd daemon.

use std::collections::HashMap;
use std::sync::Arc;

use log::error;
use serde_json::from_str;
use zbus::{interface, object_server::SignalEmitter};

use crate::app_context::AppState;
use crate::event::{ConfigChangeType, Event, EventBus};
use crate::fan_curve::FanCurve;

/// D-Bus interface for external control of the tt_riingd daemon.
///
/// Provides methods for querying sensor data and controlling fan settings
/// through the D-Bus session bus.
pub struct DBusInterface {
    pub app_state: Arc<AppState>,
    pub version: String,
    pub event_bus: EventBus,
}

impl DBusInterface {
    /// Creates a new D-Bus interface with the given state, version and event bus.
    pub fn new(app_state: Arc<AppState>, version: String, event_bus: EventBus) -> Self {
        Self {
            app_state,
            version,
            event_bus,
        }
    }
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

    /// Initiates a graceful shutdown of the daemon.
    async fn stop(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        emitter.stopped().await?;
        self.event_bus
            .publish(Event::SystemShutdown)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to publish shutdown event: {e}")))
    }

    /// Returns the daemon version.
    #[zbus(property)]
    async fn version(&self) -> String {
        self.version.clone()
    }

    /// Returns current temperature readings from all sensors.
    async fn get_temperatures(&self) -> zbus::fdo::Result<HashMap<String, f32>> {
        let sensor_data = self.app_state.sensor_data.read().await;
        Ok(sensor_data.clone())
    }

    /// Analyzes and applies configuration changes.
    ///
    /// This method analyzes the current configuration file for changes
    /// and applies them if they are hot-reloadable, or provides feedback
    /// if a restart is required.
    async fn reload_config(&self) -> zbus::fdo::Result<String> {
        match self
            .app_state
            .config_manager()
            .analyze_config_changes()
            .await
        {
            Ok(change_type) => {
                let result = match &change_type {
                    ConfigChangeType::HotReload => {
                        "Configuration changes applied successfully (hot reload)".to_string()
                    }
                    ConfigChangeType::ColdRestart { changed_sections } => {
                        format!(
                            "Hardware configuration changes detected in sections: {:?}. Daemon restart required: 'sudo systemctl restart tt_riingd'",
                            changed_sections
                        )
                    }
                };

                if let Err(e) = self
                    .event_bus
                    .publish(Event::ConfigChangeDetected(change_type))
                {
                    return Err(zbus::fdo::Error::Failed(format!(
                        "Failed to publish config change event: {e}"
                    )));
                }

                Ok(result)
            }
            Err(e) => Err(zbus::fdo::Error::Failed(format!(
                "Failed to analyze configuration changes: {e}"
            ))),
        }
    }

    /// Switches the active curve for a controller channel.
    async fn switch_active_curve(&self, controller: u8, channel: u8, curve: String) {
        if let Err(e) = self
            .app_state
            .controllers
            .read()
            .await
            .switch_curve(controller, channel, &curve)
            .await
        {
            error!("{e}")
        }
    }

    /// Gets the active curve name for a controller channel.
    async fn get_active_curve(&self, controller: u8, channel: u8) -> zbus::fdo::Result<String> {
        self.app_state
            .controllers
            .read()
            .await
            .get_active_curve(controller, channel)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Curve not found: {e}")))
    }

    /// Gets the firmware version for a controller.
    async fn get_firmware_version(&self, controller: u8) -> zbus::fdo::Result<String> {
        self.app_state
            .controllers
            .read()
            .await
            .get_firmware_version(controller)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Firmware version not found: {e}")))
            .map(|(mj, mi, pa)| format!("{mj}.{mi}.{pa}"))
    }

    /// Updates curve data for a specific curve.
    async fn update_curve_data(
        &self,
        controller: u8,
        channel: u8,
        curve: &str,
        curve_data: &str,
    ) -> zbus::fdo::Result<()> {
        let fan_curve: FanCurve = from_str(curve_data)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("Invalid curve data: {e}")))?;
        self.app_state
            .controllers
            .read()
            .await
            .update_curve_data(controller, channel, curve, &fan_curve)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to update curve data: {e}")))
    }
}
