pub mod cfg;
pub mod fan_curve;
pub mod mappings;

#[cfg(test)]
pub mod tests;

pub use crate::core::event::ConfigChangeType;
pub use cfg::{
    Config, ConfigManager, ControllerCfg, CurveCfg, CurveMappingCfg, EffectCfg, EffectMappingCfg,
    FanCfg, FanTarget, MappingCfg, SensorCfg, UsbSelector,
};
pub use fan_curve::{FanCurve, Point};
pub use mappings::{CurveMapping, EffectMapping, EffectStore, FanRef, Mapping};
