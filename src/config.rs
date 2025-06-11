//! Configuration management for tt_riingd daemon.
//!
//! Handles loading, parsing, and validation of YAML configuration files
//! that define fan curves, sensor mappings, and system behavior.

use crate::fan_curve::Point;
use anyhow::{Context, Result};
use log::info;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;

use crate::event::ConfigChangeType;

/// Main configuration structure for the tt_riingd daemon.
///
/// Contains all configuration parameters including controllers, curves,
/// sensors, and operational settings. This structure is deserialized
/// from the YAML configuration file.
///
/// # Example
///
/// ```yaml
/// version: 1
/// tick_seconds: 2
/// enable_broadcast: false
/// broadcast_interval: 2
///
/// controllers:
///   - kind: riing-quad
///     id: "controller1"
///     usb:
///       vid: 0x264a
///       pid: 0x2330
///     fans:
///       - idx: 1
///         name: "CPU Fan"
///         active_curve: "cpu_curve"
///         curve: ["cpu_curve"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Configuration version for compatibility checking.
    pub version: u8,

    /// Monitoring interval in seconds.
    #[serde(default = "defaults::tick_seconds")]
    pub tick_seconds: u16,

    /// Whether to enable periodic temperature broadcasts.
    #[serde(default = "defaults::enable_broadcast")]
    pub enable_broadcast: bool,

    /// Interval between broadcasts in seconds.
    #[serde(default = "defaults::broadcast_interval")]
    pub broadcast_interval: u16,

    /// List of hardware controllers to manage.
    #[serde(default)]
    pub controllers: Vec<ControllerCfg>,

    /// List of fan speed curves.
    #[serde(default)]
    pub curves: Vec<CurveCfg>,

    /// List of temperature sensors.
    #[serde(default)]
    pub sensors: Vec<SensorCfg>,

    /// Mappings between sensors and fan targets.
    #[serde(default)]
    pub mappings: Vec<MappingCfg>,

    /// Available RGB color definitions.
    #[serde(default)]
    pub colors: Vec<ColorCfg>,

    /// Mappings between colors and fan targets.
    #[serde(default)]
    pub color_mappings: Vec<ColorMappingCfg>,
}

/// Hardware controller configuration variants.
///
/// Defines different types of hardware controllers that can be managed
/// by the daemon. Currently supports Thermaltake Riing Quad controllers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ControllerCfg {
    /// Thermaltake Riing Quad controller configuration.
    RiingQuad {
        /// Unique identifier for this controller.
        id: String,

        /// USB device selector for hardware identification.
        usb: UsbSelector,

        /// List of fans connected to this controller.
        #[serde(default)]
        fans: Vec<FanCfg>,
    },
}

/// Individual fan configuration within a controller.
///
/// Defines the settings for a specific fan including its identification,
/// active curve, and available curves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FanCfg {
    /// Fan index on the controller (1-based).
    pub idx: u8,

    /// Human-readable name for this fan.
    pub name: String,

    /// Name of the currently active speed curve.
    pub active_curve: String,

    /// List of available curve names for this fan.
    /// Note: This is a simple Vec<String> for curve references.
    /// A future enhancement could use HashMap<String, CurveCfg> for direct curve storage.
    pub curve: Vec<String>,
}

/// Fan curve configuration variants for temperature-based control.
///
/// Defines different algorithms for controlling fan speed based on temperature:
/// - Constant: Fixed speed regardless of temperature
/// - StepCurve: Linear interpolation between temperature-speed points
/// - Bezier: Smooth curve using Bezier interpolation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CurveCfg {
    /// Constant speed curve (fixed percentage).
    Constant {
        /// Unique identifier for this curve.
        id: String,
        /// Fixed speed percentage (0-100).
        speed: u8,
    },
    /// Step-based linear interpolation curve.
    StepCurve {
        /// Unique identifier for this curve.
        id: String,
        /// Temperature points in Celsius.
        tmps: Vec<f32>,
        /// Speed percentages (0-100) corresponding to temperatures.
        spds: Vec<u8>,
    },
    /// Smooth Bezier curve interpolation.
    Bezier {
        /// Unique identifier for this curve.
        id: String,
        /// Control points defining the Bezier curve.
        points: Vec<Point>,
    },
}

impl CurveCfg {
    /// Gets the unique identifier for this curve.
    ///
    /// # Returns
    ///
    /// The curve ID string.
    pub fn get_id(&self) -> String {
        match self {
            CurveCfg::Constant { id, .. } => id.clone(),
            CurveCfg::StepCurve { id, .. } => id.clone(),
            CurveCfg::Bezier { id, .. } => id.clone(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            tick_seconds: defaults::tick_seconds(),
            enable_broadcast: defaults::enable_broadcast(),
            broadcast_interval: defaults::broadcast_interval(),
            controllers: Vec::new(),
            curves: Vec::new(),
            sensors: Vec::new(),
            mappings: Vec::new(),
            colors: Vec::new(),
            color_mappings: Vec::new(),
        }
    }
}

impl Config {
    /// Basic configuration validation.
    ///
    /// Performs minimal validation required by the ConfigManager.
    pub fn validate(&self) -> anyhow::Result<()> {
        // Basic validation - could be extended in the future if needed
        Ok(())
    }

    /// Analyzes differences between this config and another to determine reload type.
    ///
    /// Returns ConfigChangeType indicating whether changes can be hot-reloaded
    /// or require a daemon restart.
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

/// Mapping configuration between sensors and fan targets.
///
/// Defines which temperature sensor controls which fans, enabling
/// temperature-based fan speed control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingCfg {
    /// Sensor identifier to read temperature from.
    pub sensor: String,

    /// List of fan targets controlled by this sensor.
    pub targets: Vec<FanTarget>,
}

/// RGB color mapping configuration for fan lighting.
///
/// Associates a color name with specific fan targets for RGB lighting control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorMappingCfg {
    /// Color name to apply to target fans.
    pub color: String,

    /// List of fan targets that should display this color.
    pub targets: Vec<FanTarget>,
}

/// Target fan specification for mappings.
///
/// Identifies a specific fan by controller and channel for mapping relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanTarget {
    /// Controller index (1-based).
    pub controller: u8,

    /// Fan index on the controller (1-based).
    pub fan_idx: u8,
}

mod defaults {
    /// Default monitoring interval in seconds.
    pub fn tick_seconds() -> u16 {
        2
    }

    /// Default broadcast enable state.
    pub fn enable_broadcast() -> bool {
        false
    }

    /// Default broadcast interval in seconds.
    pub fn broadcast_interval() -> u16 {
        2
    }
}

/// USB device selector for hardware identification.
///
/// Specifies USB vendor/product IDs and optional serial number
/// for identifying specific hardware controllers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsbSelector {
    /// USB Vendor ID.
    pub vid: u16,

    /// USB Product ID.
    pub pid: u16,

    /// Optional serial number for device identification.
    #[serde(default)]
    pub serial: Option<String>,
}

/// Temperature sensor configuration variants.
///
/// Defines different types of temperature sensors that can be monitored.
/// Currently supports lm-sensors hardware monitoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SensorCfg {
    /// lm-sensors hardware monitoring configuration.
    LmSensors {
        /// Unique identifier for this sensor.
        id: String,

        /// Hardware chip identifier (e.g., "k10temp-pci-00c3").
        chip: String,

        /// Sensor feature name (e.g., "Tctl").
        feature: String,
    },
}

/// RGB color definition.
///
/// Associates a color name with its RGB values for lighting control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorCfg {
    /// Human-readable color name.
    pub color: String,

    /// RGB color values [red, green, blue] (0-255 each).
    pub rgb: [u8; 3],
}

fn locate_config() -> Result<PathBuf> {
    // 2) ENV
    if let Ok(env_path) = env::var("TT_RIINGD_CONFIG") {
        return Ok(PathBuf::from(env_path));
    }

    // 3) XDG_CONFIG_HOME or $HOME/.config
    if let Some(mut cfg_dir) = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
    {
        cfg_dir.push("tt_riingd/config.yml");
        if cfg_dir.exists() {
            return Ok(cfg_dir.clone());
        }
    }

    // 4) /etc
    let etc = Path::new("/etc/tt_riingd/config.yml");
    if etc.exists() {
        return Ok(etc.to_path_buf());
    }

    anyhow::bail!("Configuration file not found in any standard location")
}

/// Configuration manager that handles both config data and file operations.
///
/// Provides a unified interface for loading, reloading, and managing configuration
/// without exposing the underlying file path to the rest of the application.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::config::ConfigManager;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Load from specific path
/// let config_manager = ConfigManager::load(Some(PathBuf::from("config.yml"))).await?;
///
/// // Load from standard locations
/// let config_manager = ConfigManager::load(None).await?;
///
/// // Access configuration
/// let tick_seconds = config_manager.get().await.tick_seconds;
///
/// // Reload configuration
/// config_manager.reload().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    path: PathBuf,
}

#[allow(dead_code)]
impl ConfigManager {
    /// Creates a new ConfigManager with the given config and path.
    pub fn new(config: Config, path: PathBuf) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            path,
        }
    }

    /// Loads configuration from file or standard locations.
    ///
    /// Searches for configuration in the following order:
    /// 1. Provided path parameter
    /// 2. TT_RIINGD_CONFIG environment variable
    /// 3. XDG_CONFIG_HOME/tt_riingd/config.yml or ~/.config/tt_riingd/config.yml
    /// 4. /etc/tt_riingd/config.yml
    pub async fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = match path {
            Some(p) => p,
            None => locate_config().context("No configuration file found")?,
        };

        info!("Loading config from: {}", config_path.display());
        let config = Self::load_config_from_path(&config_path).await?;

        Ok(Self::new(config, config_path))
    }

    /// Gets a read-only reference to the current configuration.
    pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config.read().await
    }

    /// Gets a mutable reference to the current configuration.
    pub async fn get_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, Config> {
        self.config.write().await
    }

    /// Returns the path to the configuration file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reloads configuration from the same file.
    ///
    /// This is useful for hot-reloading configuration changes.
    pub async fn reload(&self) -> Result<()> {
        info!("Reloading config from: {}", self.path.display());
        let new_config = Self::load_config_from_path(&self.path).await?;

        *self.config.write().await = new_config;
        info!("Configuration reloaded successfully");
        Ok(())
    }

    /// Analyzes configuration changes and returns the type of reload required.
    ///
    /// Compares the current configuration with a new one from file
    /// to determine if hot-reload is possible or restart is required.
    pub async fn analyze_config_changes(&self) -> Result<ConfigChangeType> {
        let current_config = self.config.read().await;
        let new_config = Self::load_config_from_path(&self.path).await?;

        Ok(current_config.analyze_changes(&new_config))
    }

    /// Saves the current configuration to file.
    pub async fn save(&self) -> Result<()> {
        let config = self.config.read().await;
        self.save_to_path(&config, &self.path).await
    }

    /// Saves configuration to a specific path.
    pub async fn save_to_path(&self, config: &Config, path: &Path) -> Result<()> {
        let config_yaml =
            serde_yaml::to_string(config).context("Failed to serialize configuration")?;

        let tmp_path = path.with_extension("yml.tmp");
        fs::write(&tmp_path, config_yaml).with_context(|| {
            format!("Failed to write temporary config to {}", tmp_path.display())
        })?;

        fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to move config to {}", path.display()))?;

        info!("Configuration saved to: {}", path.display());
        Ok(())
    }

    /// Validates the current configuration.
    pub async fn validate(&self) -> Result<()> {
        let config = self.config.read().await;
        config.validate()
    }

    /// Clones the current configuration.
    ///
    /// Useful when you need to work with a snapshot of the config.
    pub async fn clone_config(&self) -> Config {
        self.config.read().await.clone()
    }

    /// Updates the configuration with a new one.
    ///
    /// This validates the new configuration before applying it.
    pub async fn update_config(&self, new_config: Config) -> Result<()> {
        new_config
            .validate()
            .context("New configuration is invalid")?;
        *self.config.write().await = new_config;
        info!("Configuration updated in memory");
        Ok(())
    }

    /// Returns an `Arc<RwLock<Config>>` for sharing between services.
    ///
    /// This allows multiple services to access the same configuration
    /// instance without cloning the entire config.
    pub fn as_shared(&self) -> Arc<RwLock<Config>> {
        self.config.clone()
    }

    /// Loads configuration from a specific path (internal helper).
    async fn load_config_from_path(path: &Path) -> Result<Config> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML in: {}", path.display()))?;

        if config.version != 1 {
            anyhow::bail!(
                "Unsupported config version {} in file: {}",
                config.version,
                path.display()
            );
        }

        config
            .validate()
            .with_context(|| format!("Configuration validation failed for: {}", path.display()))?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper function to create temporary config file
    fn create_temp_config(content: &str) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        temp_file
    }

    #[test]
    fn config_load_valid_yaml() {
        let yaml_content = r#"
version: 1
tick_seconds: 3
enable_broadcast: true
broadcast_interval: 5
controllers:
  - kind: "riing-quad"
    id: "controller1"
    usb:
      vid: 0x264a
      pid: 0x2330
    fans:
      - idx: 1
        name: "CPU Fan"
        active_curve: "cpu_curve"
        curve: ["cpu_curve"]

curves:
  - kind: "constant"
    id: "cpu_curve"
    speed: 50

sensors:
  - kind: "lm-sensors"
    id: "cpu_sensor"
    chip: "k10temp-pci-00c3"
    feature: "Tctl"

mappings:
  - sensor: "cpu_sensor"
    targets:
      - controller: 0
        fan_idx: 1

colors:
  - color: "blue"
    rgb: [0, 0, 255]

color_mappings:
  - color: "blue"
    targets:
      - controller: 0
        fan_idx: 1
"#;

        let temp_file = create_temp_config(yaml_content);

        // Use ConfigManager to load the config
        let rt = tokio::runtime::Runtime::new().unwrap();
        let config_manager = rt
            .block_on(ConfigManager::load(Some(temp_file.path().to_path_buf())))
            .unwrap();
        let config = rt.block_on(config_manager.clone_config());

        assert_eq!(config.version, 1);
        assert_eq!(config.tick_seconds, 3);
        assert_eq!(config.enable_broadcast, true);
        assert_eq!(config.broadcast_interval, 5);
        assert_eq!(config.controllers.len(), 1);
        assert_eq!(config.curves.len(), 1);
        assert_eq!(config.sensors.len(), 1);
        assert_eq!(config.mappings.len(), 1);
        assert_eq!(config.colors.len(), 1);
        assert_eq!(config.color_mappings.len(), 1);
    }

    #[test]
    fn curve_cfg_get_id() {
        let constant = CurveCfg::Constant {
            id: "test_constant".to_string(),
            speed: 50,
        };
        assert_eq!(constant.get_id(), "test_constant");

        let step = CurveCfg::StepCurve {
            id: "test_step".to_string(),
            tmps: vec![30.0, 60.0],
            spds: vec![20, 80],
        };
        assert_eq!(step.get_id(), "test_step");

        let bezier = CurveCfg::Bezier {
            id: "test_bezier".to_string(),
            points: vec![Point { x: 0.0, y: 0.0 }],
        };
        assert_eq!(bezier.get_id(), "test_bezier");
    }

    #[test]
    fn analyze_changes_hot_reload_for_curves() {
        let mut config1 = Config::default();
        let mut config2 = Config::default();

        // Add different curves
        config1.curves = vec![CurveCfg::Constant {
            id: "test".to_string(),
            speed: 50,
        }];

        config2.curves = vec![CurveCfg::Constant {
            id: "test".to_string(),
            speed: 75, // Changed speed
        }];

        let change_type = config1.analyze_changes(&config2);
        match change_type {
            ConfigChangeType::HotReload => {
                // Expected - curves can be hot-reloaded
            }
            _ => panic!("Expected HotReload for curve changes"),
        }
    }

    #[test]
    fn analyze_changes_cold_restart_for_controllers() {
        let config1 = Config::default();
        let config2 = Config {
            controllers: vec![ControllerCfg::RiingQuad {
                id: "test_controller".to_string(),
                usb: UsbSelector {
                    vid: 0x264a,
                    pid: 0x2330,
                    serial: None,
                },
                fans: vec![],
            }],
            ..Default::default()
        };

        let change_type = config1.analyze_changes(&config2);
        match change_type {
            ConfigChangeType::ColdRestart { changed_sections } => {
                assert!(changed_sections.contains(&"controllers".to_string()));
            }
            _ => panic!("Expected ColdRestart for controller changes"),
        }
    }

    #[test]
    fn analyze_changes_cold_restart_for_sensors() {
        let config1 = Config::default();
        let config2 = Config {
            sensors: vec![SensorCfg::LmSensors {
                id: "test_sensor".to_string(),
                chip: "k10temp-pci-00c3".to_string(),
                feature: "Tctl".to_string(),
            }],
            ..Default::default()
        };

        let change_type = config1.analyze_changes(&config2);
        match change_type {
            ConfigChangeType::ColdRestart { changed_sections } => {
                assert!(changed_sections.contains(&"sensors".to_string()));
            }
            _ => panic!("Expected ColdRestart for sensor changes"),
        }
    }

    #[test]
    fn analyze_changes_hot_reload_for_mappings() {
        let config1 = Config::default();
        let config2 = Config {
            mappings: vec![MappingCfg {
                sensor: "cpu_temp".to_string(),
                targets: vec![FanTarget {
                    controller: 1,
                    fan_idx: 1,
                }],
            }],
            ..Default::default()
        };

        let change_type = config1.analyze_changes(&config2);
        match change_type {
            ConfigChangeType::HotReload => {
                // Expected - mappings can be hot-reloaded
            }
            _ => panic!("Expected HotReload for mapping changes"),
        }
    }

    #[test]
    fn analyze_changes_no_changes() {
        let config1 = Config::default();
        let config2 = Config::default();

        let change_type = config1.analyze_changes(&config2);
        match change_type {
            ConfigChangeType::HotReload => {
                // Expected - no changes means hot reload is safe
            }
            _ => panic!("Expected HotReload for identical configs"),
        }
    }
}
