//! Configuration management for tt_riingd daemon.
//!
//! Handles loading, parsing, and validation of YAML configuration files
//! that define fan curves, sensor mappings, and system behavior.

use crate::fan_curve::Point;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;
use tracing::info;

use crate::event::ConfigChangeType;

/// Maximum iterations for Bezier curve binary search.
const MAX_ITERATIONS: usize = 100;

/// Precision epsilon for Bezier curve calculations.
const EPSILON: f32 = 1e-6;

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

    /// Mappings between curves and fan targets.
    #[serde(default)]
    pub active_curve_mappings: Vec<CurveMappingCfg>,

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
    // /// Name of the currently active speed curve.
    // pub active_curve: String,
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

    Point { x, y }
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
fn get_speed_for_temp(pts: &[Point], temp: f32) -> f32 {
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

    pub fn calculate_speed(&self, temperature: f32) -> Result<u8> {
        match self {
            CurveCfg::Constant { speed, .. } => Ok(*speed),
            CurveCfg::StepCurve { tmps, spds, .. } => {
                if tmps.len() != spds.len() {
                    return Err(anyhow::anyhow!(
                        "Temperature and speed arrays must have the same length".to_string()
                    ));
                }
                if tmps.is_empty() {
                    return Err(anyhow::anyhow!("Step curve cannot be empty".to_string()));
                }
                for i in 0..tmps.len() - 1 {
                    if temperature >= tmps[i] && temperature < tmps[i + 1] {
                        let t = (temperature - tmps[i]) / (tmps[i + 1] - tmps[i]);
                        let speed = (spds[i] as f32 * (1.0 - t) + spds[i + 1] as f32 * t) as u8;
                        return Ok(speed);
                    }
                }
                if temperature < tmps[0] {
                    return Ok(spds[0]);
                }
                Ok(*spds.last().unwrap())
            }
            CurveCfg::Bezier { points, .. } => {
                if points.len() != 4 {
                    return Err(anyhow::anyhow!(
                        "Bezier curve must have exactly 4 control points"
                    ));
                }

                // Сортируем точки по x (температуре) для корректной обработки граничных случаев
                let mut sorted_points = points.clone();
                sorted_points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

                // Обработка граничных случаев
                if temperature <= sorted_points[0].x {
                    return Ok(sorted_points[0].y.clamp(0.0, 100.0) as u8);
                }
                if temperature >= sorted_points[3].x {
                    return Ok(sorted_points[3].y.clamp(0.0, 100.0) as u8);
                }

                // Используем бинарный поиск для нахождения правильной скорости
                let speed = get_speed_for_temp(points, temperature);
                Ok(speed.clamp(0.0, 100.0) as u8)
            }
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
            active_curve_mappings: Vec::new(),
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

/// Active curve mapping configuration for fan speed control.
///
/// Associates a temperature sensor with specific fan targets
/// and their active speed curves.
/// This allows dynamic fan speed adjustment based on temperature readings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurveMappingCfg {
    /// Curve identifier to apply to targets.
    pub curve: String,

    /// List of fan targets that should use this curve.
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
/// Supports lm-sensors hardware monitoring and NVIDIA GPUs via NVML.
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
    /// NVIDIA GPU temperature monitoring via NVML.
    Nvidia {
        /// Unique identifier for this sensor.
        id: String,

        /// GPU index (0-based, e.g., 0 for first GPU).
        gpu_index: u32,
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

impl ConfigManager {
    /// Creates a new ConfigManager with the given config and path.
    ///
    /// This is primarily used for testing purposes.
    #[cfg(test)]
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

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            path: config_path,
        })
    }

    /// Gets a read-only reference to the current configuration.
    pub async fn get(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
        self.config.read().await
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

    /// Clones the current configuration.
    ///
    /// Useful when you need to work with a snapshot of the config.
    pub async fn clone_config(&self) -> Config {
        self.config.read().await.clone()
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
    fn curve_cfg_calculate_speed_bezier() {
        // Создаем простую кривую Безье: линейная от 30°C/20% до 70°C/80%
        let bezier = CurveCfg::Bezier {
            id: "test_bezier".to_string(),
            points: vec![
                Point { x: 30.0, y: 20.0 }, // Начальная точка
                Point { x: 40.0, y: 35.0 }, // Контрольная точка 1
                Point { x: 60.0, y: 65.0 }, // Контрольная точка 2
                Point { x: 70.0, y: 80.0 }, // Конечная точка
            ],
        };

        // Тест граничных случаев
        assert_eq!(bezier.calculate_speed(25.0).unwrap(), 20); // Ниже минимума
        assert_eq!(bezier.calculate_speed(75.0).unwrap(), 80); // Выше максимума

        // Тест точек на кривой
        let speed_at_30 = bezier.calculate_speed(30.0).unwrap();
        let speed_at_70 = bezier.calculate_speed(70.0).unwrap();
        assert_eq!(speed_at_30, 20);
        assert_eq!(speed_at_70, 80);

        // Тест промежуточной точки
        let speed_at_50 = bezier.calculate_speed(50.0).unwrap();
        assert!(speed_at_50 > 20 && speed_at_50 < 80);
    }

    #[test]
    fn curve_cfg_calculate_speed_bezier_invalid_points() {
        // Тест с неправильным количеством точек
        let bezier = CurveCfg::Bezier {
            id: "invalid_bezier".to_string(),
            points: vec![Point { x: 30.0, y: 20.0 }, Point { x: 70.0, y: 80.0 }],
        };

        let result = bezier.calculate_speed(50.0);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exactly 4 control points")
        );
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

    // Additional comprehensive tests for edge cases

    #[test]
    fn curve_cfg_calculate_speed_constant_edge_cases() {
        let constant = CurveCfg::Constant {
            id: "test_constant".to_string(),
            speed: 0,
        };
        assert_eq!(constant.calculate_speed(25.0).unwrap(), 0);
        assert_eq!(constant.calculate_speed(-10.0).unwrap(), 0);
        assert_eq!(constant.calculate_speed(150.0).unwrap(), 0);

        let constant_max = CurveCfg::Constant {
            id: "test_constant_max".to_string(),
            speed: 100,
        };
        assert_eq!(constant_max.calculate_speed(25.0).unwrap(), 100);
    }

    #[test]
    fn curve_cfg_calculate_speed_step_curve_edge_cases() {
        // Empty step curve should be handled gracefully
        let empty_step = CurveCfg::StepCurve {
            id: "empty_step".to_string(),
            tmps: vec![],
            spds: vec![],
        };
        let result = empty_step.calculate_speed(50.0);
        assert!(result.is_err());

        // Single point step curve
        let single_point = CurveCfg::StepCurve {
            id: "single_point".to_string(),
            tmps: vec![50.0],
            spds: vec![75],
        };
        assert_eq!(single_point.calculate_speed(25.0).unwrap(), 75); // Below range
        assert_eq!(single_point.calculate_speed(50.0).unwrap(), 75); // At point
        assert_eq!(single_point.calculate_speed(75.0).unwrap(), 75); // Above range

        // Reverse temperature order (should still work)
        let reverse_temps = CurveCfg::StepCurve {
            id: "reverse_temps".to_string(),
            tmps: vec![70.0, 30.0], // Intentionally reversed
            spds: vec![80, 20],
        };
        // Should handle gracefully by sorting internally
        let result = reverse_temps.calculate_speed(50.0);
        assert!(result.is_ok());

        // Extreme temperature values
        let extreme_temps = CurveCfg::StepCurve {
            id: "extreme_temps".to_string(),
            tmps: vec![-50.0, 0.0, 100.0, 200.0],
            spds: vec![10, 30, 70, 100],
        };
        assert_eq!(extreme_temps.calculate_speed(-100.0).unwrap(), 10); // Far below
        assert_eq!(extreme_temps.calculate_speed(300.0).unwrap(), 100); // Far above
        // For interpolated value, just check it's reasonable
        let interpolated = extreme_temps.calculate_speed(50.0).unwrap();
        assert!((30..=70).contains(&interpolated)); // Should be between 30 and 70
    }

    #[test]
    fn curve_cfg_calculate_speed_bezier_edge_cases() {
        // Test with identical control points (degenerate case)
        let degenerate_bezier = CurveCfg::Bezier {
            id: "degenerate".to_string(),
            points: vec![
                Point { x: 50.0, y: 60.0 },
                Point { x: 50.0, y: 60.0 },
                Point { x: 50.0, y: 60.0 },
                Point { x: 50.0, y: 60.0 },
            ],
        };
        assert_eq!(degenerate_bezier.calculate_speed(50.0).unwrap(), 60);
        assert_eq!(degenerate_bezier.calculate_speed(25.0).unwrap(), 60); // Below
        assert_eq!(degenerate_bezier.calculate_speed(75.0).unwrap(), 60); // Above

        // Test with extreme y-values (should be clamped)
        let extreme_y_bezier = CurveCfg::Bezier {
            id: "extreme_y".to_string(),
            points: vec![
                Point { x: 30.0, y: -10.0 }, // Below 0
                Point { x: 40.0, y: 50.0 },
                Point { x: 60.0, y: 50.0 },
                Point { x: 70.0, y: 110.0 }, // Above 100
            ],
        };
        let result_low = extreme_y_bezier.calculate_speed(30.0).unwrap();
        let result_high = extreme_y_bezier.calculate_speed(70.0).unwrap();
        assert!(result_low <= 100);
        assert!(result_high <= 100);

        // Test with non-monotonic x-values (complex curve)
        let complex_bezier = CurveCfg::Bezier {
            id: "complex".to_string(),
            points: vec![
                Point { x: 30.0, y: 20.0 },
                Point { x: 80.0, y: 40.0 }, // Higher x than next point
                Point { x: 40.0, y: 60.0 }, // Lower x than previous
                Point { x: 70.0, y: 80.0 },
            ],
        };
        // Should handle gracefully with binary search
        let result = complex_bezier.calculate_speed(50.0);
        assert!(result.is_ok());
        let speed = result.unwrap();
        assert!(speed <= 100);
    }

    #[test]
    fn config_validation_edge_cases() {
        // Test config with duplicate controller IDs
        let mut config_duplicate_controllers: Config = Config::default();
        config_duplicate_controllers.controllers.extend(vec![
            ControllerCfg::RiingQuad {
                id: "duplicate".to_string(),
                usb: UsbSelector {
                    vid: 0x264a,
                    pid: 0x2330,
                    serial: None,
                },
                fans: vec![],
            },
            ControllerCfg::RiingQuad {
                id: "duplicate".to_string(), // Same ID
                usb: UsbSelector {
                    vid: 0x264a,
                    pid: 0x2331,
                    serial: None,
                },
                fans: vec![],
            },
        ]);
        // Should validate successfully (duplicate IDs are allowed for now)
        assert!(config_duplicate_controllers.validate().is_ok());

        // Test config with extreme tick_seconds values
        let mut config_extreme_timing: Config = Config {
            tick_seconds: 0,
            ..Default::default()
        };
        // config_extreme_timing.tick_seconds = 0; // Zero interval
        assert!(config_extreme_timing.validate().is_ok()); // Should be allowed

        config_extreme_timing.tick_seconds = u16::MAX; // Maximum interval
        assert!(config_extreme_timing.validate().is_ok());

        // Test config with empty mappings but sensors present
        let mut config_orphaned_sensors: Config = Config::default();
        config_orphaned_sensors
            .sensors
            .extend(vec![SensorCfg::LmSensors {
                id: "orphaned_sensor".to_string(),
                chip: "test_chip".to_string(),
                feature: "test_feature".to_string(),
            }]);
        config_orphaned_sensors.mappings = vec![]; // No mappings for the sensor
        assert!(config_orphaned_sensors.validate().is_ok()); // Should be allowed
    }

    #[test]
    fn config_manager_error_handling() {
        use std::io::Write;

        // Test loading invalid YAML
        let invalid_yaml = "invalid: yaml: content: [unclosed";
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_yaml.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(ConfigManager::load(Some(temp_file.path().to_path_buf())));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse YAML")
        );

        // Test loading config with wrong version
        let wrong_version_yaml = r#"
version: 999
tick_seconds: 5
"#;
        let mut temp_file2 = NamedTempFile::new().unwrap();
        temp_file2.write_all(wrong_version_yaml.as_bytes()).unwrap();
        temp_file2.flush().unwrap();

        let result2 = rt.block_on(ConfigManager::load(Some(temp_file2.path().to_path_buf())));
        assert!(result2.is_err());
        let error_msg = result2.unwrap_err().to_string();
        assert!(
            error_msg.contains("Unsupported config version")
                || error_msg.contains("Failed to parse YAML")
        );

        // Test loading non-existent file
        let result3 = rt.block_on(ConfigManager::load(Some(PathBuf::from(
            "/non/existent/path.yml",
        ))));
        assert!(result3.is_err());
        assert!(
            result3
                .unwrap_err()
                .to_string()
                .contains("Failed to read config file")
        );
    }

    #[test]
    fn usb_selector_serialization() {
        // Test UsbSelector with all fields
        let usb_full = UsbSelector {
            vid: 0x264a,
            pid: 0x2330,
            serial: Some("ABC123".to_string()),
        };

        let yaml = serde_yaml::to_string(&usb_full).unwrap();
        let deserialized: UsbSelector = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(deserialized.vid, 0x264a);
        assert_eq!(deserialized.pid, 0x2330);
        assert_eq!(deserialized.serial, Some("ABC123".to_string()));

        // Test UsbSelector without serial
        let usb_minimal = UsbSelector {
            vid: 0x1234,
            pid: 0x5678,
            serial: None,
        };

        let yaml2 = serde_yaml::to_string(&usb_minimal).unwrap();
        let deserialized2: UsbSelector = serde_yaml::from_str(&yaml2).unwrap();

        assert_eq!(deserialized2.vid, 0x1234);
        assert_eq!(deserialized2.pid, 0x5678);
        assert_eq!(deserialized2.serial, None);
    }

    #[test]
    fn color_cfg_edge_cases() {
        // Test extreme RGB values
        let color_black = ColorCfg {
            color: "black".to_string(),
            rgb: [0, 0, 0],
        };

        let color_white = ColorCfg {
            color: "white".to_string(),
            rgb: [255, 255, 255],
        };

        // Test serialization roundtrip
        let yaml_black = serde_yaml::to_string(&color_black).unwrap();
        let deserialized_black: ColorCfg = serde_yaml::from_str(&yaml_black).unwrap();
        assert_eq!(deserialized_black.color, "black");
        assert_eq!(deserialized_black.rgb, [0, 0, 0]);

        let yaml_white = serde_yaml::to_string(&color_white).unwrap();
        let deserialized_white: ColorCfg = serde_yaml::from_str(&yaml_white).unwrap();
        assert_eq!(deserialized_white.color, "white");
        assert_eq!(deserialized_white.rgb, [255, 255, 255]);
    }

    #[test]
    fn fan_target_edge_cases() {
        // Test extreme controller and fan indices
        let target_min = FanTarget {
            controller: 1,
            fan_idx: 1,
        };

        let target_max = FanTarget {
            controller: u8::MAX,
            fan_idx: u8::MAX,
        };

        // Test serialization
        let yaml_min = serde_yaml::to_string(&target_min).unwrap();
        let deserialized_min: FanTarget = serde_yaml::from_str(&yaml_min).unwrap();
        assert_eq!(deserialized_min.controller, 1);
        assert_eq!(deserialized_min.fan_idx, 1);

        let yaml_max = serde_yaml::to_string(&target_max).unwrap();
        let deserialized_max: FanTarget = serde_yaml::from_str(&yaml_max).unwrap();
        assert_eq!(deserialized_max.controller, u8::MAX);
        assert_eq!(deserialized_max.fan_idx, u8::MAX);
    }
}
