//! Temperature sensor mappings for the tt_riingd daemon.
//!
//! Provides functionality for mapping temperature sensors to fan controllers
//! and color configurations based on temperature readings.

use std::sync::Arc;

use dashmap::{DashMap, DashSet};
use tracing::{debug, error};

use crate::config::cfg::{CurveMappingCfg, EffectCfg, EffectMappingCfg, MappingCfg};
use crate::effects::effect_runner::EffectInstance;

/// Type alias for sensor identifier keys.
pub type SensorKey = String;

/// Reference to a specific fan on a controller.
///
/// Uniquely identifies a fan channel by its controller and channel number.
/// Used as a key for mapping relationships between sensors and fans.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FanRef {
    /// Controller index (0-based).
    // pub controller_id: usize,
    pub controller_id: String,

    /// Fan channel on the controller (0-based).
    pub channel: usize,
}

/// Bidirectional mapping between temperature sensors and fans.
///
/// Maintains relationships that allow efficient lookup in both directions:
/// - Find which sensor controls a specific fan
/// - Find which fans are controlled by a specific sensor
///
/// Thread-safe using DashMap for concurrent access.
#[derive(Default, Debug)]
pub struct Mapping {
    /// Maps fan references to their controlling sensor.
    fans2sensor: DashMap<FanRef, SensorKey>,

    /// Maps sensors to the set of fans they control.
    sensor2fans: DashMap<SensorKey, DashSet<FanRef>>,
}

#[derive(Default, Debug)]
pub struct CurveMapping {
    /// Maps color names to the set of fans that display them.
    fan2curve: DashMap<FanRef, String>,
}

impl CurveMapping {
    pub fn load_mappings(curve_mapping_cfg: &[CurveMappingCfg]) -> Self {
        curve_mapping_cfg
            .iter()
            .flat_map(|c| {
                let ckey = c.curve.clone();
                c.targets.iter().map(move |t| (ckey.clone(), t))
            })
            .fold(Self::default(), |acc, (curve, target)| {
                let fan = FanRef {
                    controller_id: target.controller_id.clone(),
                    channel: target.fan_idx as usize,
                };

                acc.fan2curve.insert(fan, curve.clone());
                acc
            })
    }

    pub fn get_curve_for_fan(&self, fan: &FanRef) -> Option<String> {
        self.fan2curve.get(fan).map(|r| r.value().clone())
    }
}

#[derive(Default, Debug)]
pub struct EffectStore {
    pub runners: DashMap<String, Arc<EffectInstance>>,
}

impl EffectStore {
    pub fn build_effect_store(
        effect_cfg: &[EffectCfg],
        effect_mapping: &[EffectMappingCfg],
    ) -> Self {
        effect_cfg
            .iter()
            .filter_map(|e| {
                effect_mapping
                    .iter()
                    .find(|m| m.effect == e.get_id())
                    .map(|mapping| {
                        debug!(
                            "Creating effect runner for {} with mapping {:?}",
                            e.get_id(),
                            mapping
                        );
                        (
                            e.get_id(),
                            EffectInstance::new(e.clone(), Some(mapping.clone())),
                        )
                    })
            })
            .fold(Self::default(), |acc, (key, runner_instance)| {
                if let Ok(runner) = runner_instance {
                    acc.runners.insert(key.clone(), Arc::new(runner));
                } else {
                    error!("Failed to create effect runner for {}", key);
                }
                acc
            })
    }
}

/// Color mapping between temperature and RGB lighting.
///
/// Maps color names to the set of fans that should display those colors.
/// Used for temperature-based RGB lighting control.
#[derive(Default, Debug)]
pub struct EffectMapping {
    /// Maps color names to the set of fans that display them.
    effect2fans: DashMap<String, DashSet<FanRef>>,
}

impl EffectMapping {
    /// Builds color mapping from configuration array.
    ///
    /// Creates the mapping structure from color mapping configuration,
    /// establishing relationships between color names and fan targets.
    ///
    /// # Arguments
    ///
    /// * `color_cfg` - Array of color mapping configurations
    ///
    /// # Returns
    ///
    /// A new ColorMapping instance with configured relationships.
    pub fn build_color_mapping(color_cfg: &[EffectMappingCfg]) -> Self {
        color_cfg
            .iter()
            .flat_map(|c| {
                let ckey = c.effect.clone();
                c.targets.iter().map(move |t| (ckey.clone(), t))
            })
            .fold(Self::default(), |acc, (sensor, target)| {
                let fan = FanRef {
                    controller_id: target.controller_id.clone(),
                    channel: target.fan_idx as usize,
                };

                acc.effect2fans.entry(sensor).or_default().insert(fan);
                acc
            })
    }

    pub fn effect_to_fans_iter(&self) -> impl Iterator<Item = (String, DashSet<FanRef>)> {
        self.effect2fans
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
    }
}

impl Mapping {
    /// Loads mappings from configuration.
    ///
    /// Creates the bidirectional mapping structure from mapping configuration,
    /// establishing relationships between sensors and fan targets.
    ///
    /// # Arguments
    ///
    /// * `mapping_cfg` - Array of mapping configurations
    ///
    /// # Returns
    ///
    /// A new Mapping instance with configured relationships.
    pub fn load_mappings(mapping_cfg: &[MappingCfg]) -> Self {
        mapping_cfg
            .iter()
            .flat_map(|m| {
                let skey = m.sensor.clone();
                m.targets.iter().map(move |t| (skey.clone(), t))
            })
            .fold(Self::default(), |acc, (sensor, target)| {
                let fan = FanRef {
                    controller_id: target.controller_id.clone(),
                    channel: target.fan_idx as usize,
                };

                acc.fans2sensor.insert(fan.clone(), sensor.clone());
                acc.sensor2fans
                    .entry(sensor)
                    .or_default()
                    .insert(fan.clone());
                acc
            })
    }

    /// Gets all fans controlled by a specific sensor.
    ///
    /// Returns an iterator over fan references that are controlled by
    /// the specified sensor.
    ///
    /// # Arguments
    ///
    /// * `sensor` - Sensor key to query
    ///
    /// # Returns
    ///
    /// Iterator over FanRef instances controlled by the sensor.
    pub fn fans_for_sensor<'a>(
        &'a self,
        sensor: &'a SensorKey,
    ) -> impl Iterator<Item = FanRef> + 'a {
        self.sensor2fans
            .get(sensor)
            .into_iter()
            .flat_map(|set| set.iter().map(|r| r.clone()).collect::<Vec<_>>())
    }
}
