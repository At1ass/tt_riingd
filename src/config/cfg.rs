//! Configuration management for tt_riingd daemon.
//!
//! Handles loading, parsing, and validation of YAML configuration files
//! that define fan curves, sensor mappings, and system behavior.

use crate::{config::fan_curve::Point, effects::effect_runner::EffectRunner};
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
    pub effects: Vec<EffectCfg>,

    /// Mappings between colors and fan targets.
    #[serde(default)]
    pub effect_mappings: Vec<EffectMappingCfg>,
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
                spds.last()
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("Empty speeds vector"))
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
            effects: Vec::new(),
            effect_mappings: Vec::new(),
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
pub struct EffectMappingCfg {
    /// Color name to apply to target fans.
    pub effect: String,

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
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EffectCfg {
    ConstantColor {
        /// Unique identifier for this color.
        id: String,

        /// RGB color values [red, green, blue] (0-255 each).
        rgb: [u8; 3],
    },
    Rainbow {
        /// Unique identifier for this effect.
        id: String,

        /// Duration of the rainbow effect in seconds.
        duration: u16,
    },
    Breathing {
        /// Unique identifier for this effect.
        id: String,

        /// Color to use for breathing effect.
        color: [u8; 3],

        /// Duration of the breathing cycle in seconds.
        duration: u16,
    },
    Fade {
        /// Unique identifier for this effect.
        id: String,

        /// Color to use for fade effect.
        color: [u8; 3],

        /// Duration of the fade cycle in seconds.
        duration: u16,
    },
}

impl EffectCfg {
    /// Gets the unique identifier for this effect.
    ///
    /// # Returns
    ///
    /// The effect ID string.
    pub fn get_id(&self) -> String {
        match self {
            EffectCfg::ConstantColor { id, .. } => id.clone(),
            EffectCfg::Rainbow { id, .. } => id.clone(),
            EffectCfg::Breathing { id, .. } => id.clone(),
            EffectCfg::Fade { id, .. } => id.clone(),
        }
    }

    pub fn into_runner(self) -> Result<EffectRunner> {
        match self {
            EffectCfg::ConstantColor { rgb, .. } => Ok(EffectRunner::constant(rgb)),
            EffectCfg::Rainbow { duration, .. } => Ok(EffectRunner::rainbow(
                std::time::Duration::from_secs(duration as u64),
            )),
            EffectCfg::Breathing {
                color, duration, ..
            } => Ok(EffectRunner::breathe(
                color,
                0.2,
                1.0,
                std::time::Duration::from_secs(duration as u64),
            )),
            EffectCfg::Fade {
                color, duration, ..
            } => Ok(EffectRunner::breathe(
                color,
                0.0,
                1.0,
                std::time::Duration::from_secs(duration as u64),
            )),
        }
    }
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
