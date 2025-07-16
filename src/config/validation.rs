//! Configuration validation and change analysis.
//!
//! Contains validation logic for configuration structures and
//! analysis of configuration changes to determine reload types.

use crate::{
    config::structures::Config, core::event::ConfigChangeType, drivers::registry::Registry,
};
use anyhow::Result;

impl Config {
    /// Basic configuration validation.
    ///
    /// Performs minimal validation required by the ConfigManager.
    /// Validates that all configured controllers are supported by the system.
    ///
    /// # Returns
    ///
    /// Ok(()) if validation passes, or an error describing validation failures.
    pub fn validate(&self) -> Result<()> {
        let unsupported = Registry::validate_supported_controllers(self);

        if !unsupported.is_empty() {
            return Err(anyhow::anyhow!(
                "Unsupported controllers found: \n{}",
                unsupported.join("\n")
            ));
        }

        Ok(())
    }

    /// Analyzes differences between this config and another to determine reload type.
    ///
    /// Compares configurations to determine whether changes can be hot-reloaded
    /// or require a daemon restart. Hardware-related changes (controllers, sensors)
    /// require restart, while curve and mapping changes can be hot-reloaded.
    ///
    /// # Arguments
    ///
    /// * `other` - The new configuration to compare against
    ///
    /// # Returns
    ///
    /// ConfigChangeType indicating whether changes can be hot-reloaded
    /// or require a daemon restart with details of changed sections.
    pub fn analyze_changes(&self, other: &Config) -> ConfigChangeType {
        let mut changed_sections = Vec::new();

        // Hardware controller changes always require restart
        // This includes any controller addition, removal, or configuration change
        if self.controllers != other.controllers {
            changed_sections.push("controllers".to_string());
        }

        // Hardware sensor changes require restart
        if self.sensors != other.sensors {
            changed_sections.push("sensors".to_string());
        }

        if changed_sections.is_empty() {
            // Only hot-reloadable settings changed:
            // - Fan curves (curves)
            // - Sensor-to-fan mappings (mappings)
            // - RGB color definitions (colors)
            // - Color-to-fan mappings (color_mappings)
            // - Operational settings (tick_seconds, enable_broadcast, broadcast_interval)
            ConfigChangeType::HotReload
        } else {
            ConfigChangeType::ColdRestart { changed_sections }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::structures::{Config, ControllerCfg, SensorCfg, UsbSelector};

    #[test]
    fn test_analyze_changes_hot_reload() {
        let config1 = Config::default();
        let mut config2 = config1.clone();

        // Change only hot-reloadable settings
        config2.tick_seconds = 5;
        config2.enable_broadcast = true;

        let change_type = config1.analyze_changes(&config2);
        assert!(matches!(change_type, ConfigChangeType::HotReload));
    }

    #[test]
    fn test_analyze_changes_cold_restart() {
        let config1 = Config::default();
        let mut config2 = config1.clone();

        // Add a controller (requires restart)
        config2.controllers.push(ControllerCfg::RiingQuad {
            id: "test_controller".to_string(),
            usb: UsbSelector {
                vid: 0x264a,
                pid: 0x2330,
                serial: None,
            },
            fans: vec![],
        });

        let change_type = config1.analyze_changes(&config2);
        assert!(matches!(change_type, ConfigChangeType::ColdRestart { .. }));
    }

    #[test]
    fn test_analyze_changes_sensor_modification() {
        let config1 = Config::default();
        let mut config2 = config1.clone();

        // Add a sensor (requires restart)
        config2.sensors.push(SensorCfg::LmSensors {
            id: "test_sensor".to_string(),
            chip: "test_chip".to_string(),
            feature: "test_feature".to_string(),
        });

        let change_type = config1.analyze_changes(&config2);
        if let ConfigChangeType::ColdRestart { changed_sections } = change_type {
            assert!(changed_sections.contains(&"sensors".to_string()));
        } else {
            panic!("Expected ColdRestart for sensor changes");
        }
    }
}
