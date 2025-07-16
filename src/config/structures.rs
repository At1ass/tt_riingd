//! Configuration data structures for tt_riingd daemon.
//!
//! Contains all data structures used for configuration representation,
//! including controllers, fans, sensors, curves, and mappings.

use crate::{config::fan_curve::Point, effects::effect_runner::EffectRunner};
use anyhow::Result;
use serde::{Deserialize, Serialize};

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

impl ControllerCfg {
    /// Gets the unique identifier for this controller.
    ///
    /// # Returns
    ///
    /// The controller ID string.
    pub fn get_id(&self) -> String {
        match self {
            ControllerCfg::RiingQuad { id, .. } => id.clone(),
        }
    }

    pub fn get_fans_count(&self) -> u8 {
        match self {
            ControllerCfg::RiingQuad { fans, .. } => fans.len().try_into().unwrap_or(1),
        }
    }
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

impl MappingCfg {
    pub fn dummy() -> Self {
        Self {
            sensor: "dummy_sensor".to_string(),
            targets: Vec::new(),
        }
    }
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

impl CurveMappingCfg {
    pub fn dummy() -> Self {
        Self {
            curve: "dummy_curve".to_string(),
            targets: Vec::new(),
        }
    }
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

impl EffectMappingCfg {
    pub fn dummy() -> Self {
        Self {
            effect: "dummy_effect".to_string(),
            targets: Vec::new(),
        }
    }
}

/// Target fan specification for mappings.
///
/// Identifies a specific fan by controller and channel for mapping relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanTarget {
    /// Controller index (1-based).
    // pub controller: u8,
    pub controller_id: String,

    /// Fan index on the controller (1-based).
    pub fan_idx: u8,
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

pub(crate) mod defaults {
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
