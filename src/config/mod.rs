//! Configuration system for tt_riingd daemon.
//!
//! Provides YAML-based configuration with validation, hot-reload capability,
//! and strong typing for hardware controllers, sensors, and fan curves.
//!
//! See the [Configuration Guide](https://docs.rs/tt_riingd/latest/tt_riingd/docs/configuration.html)
//! for detailed architecture overview, examples, and usage patterns.

// Core configuration modules
pub mod curves;
pub mod location;
pub mod manager;
pub mod structures;
pub mod validation;

// Legacy modules for backward compatibility
pub mod cfg;
pub mod fan_curve;
pub mod mappings;

#[cfg(test)]
pub mod tests;

pub use crate::core::event::ConfigChangeType;

// Re-export from new modular structure
pub use location::locate_config;
pub use manager::ConfigManager;
pub use structures::{
    Config, ControllerCfg, CurveCfg, CurveMappingCfg, EffectCfg, EffectMappingCfg, FanCfg,
    FanTarget, MappingCfg, SensorCfg, UsbSelector,
};

// Legacy re-exports for backward compatibility
pub use fan_curve::{FanCurve, Point};
pub use mappings::{CurveMapping, EffectMapping, EffectStore, FanRef, Mapping};
