//! Legacy configuration module for backward compatibility.
//!
//! This module re-exports types from the new modular configuration system.
//! New code should import directly from the specific modules in `crate::config`.

// Re-export from new modular structure for backward compatibility
pub use crate::config::{
    Config, ConfigManager, ControllerCfg, CurveCfg, CurveMappingCfg, EffectCfg, EffectMappingCfg,
    FanCfg, FanTarget, MappingCfg, SensorCfg, UsbSelector, locate_config,
};
