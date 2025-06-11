//! Temperature sensor mappings for the tt_riingd daemon.
//!
//! Provides functionality for mapping temperature sensors to fan controllers
//! and color configurations based on temperature readings.

use dashmap::{DashMap, DashSet};
use log::warn;
use std::collections::HashMap;

use crate::config::{ColorMappingCfg, MappingCfg};

/// Type alias for sensor identifier keys.
pub type SensorKey = String;

/// Reference to a specific fan on a controller.
///
/// Uniquely identifies a fan channel by its controller and channel number.
/// Used as a key for mapping relationships between sensors and fans.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct FanRef {
    /// Controller index (0-based).
    pub controller_id: usize,

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

/// Color mapping between temperature and RGB lighting.
///
/// Maps color names to the set of fans that should display those colors.
/// Used for temperature-based RGB lighting control.
#[derive(Default, Debug)]
pub struct ColorMapping {
    /// Maps color names to the set of fans that display them.
    color2fans: DashMap<String, DashSet<FanRef>>,
}

impl ColorMapping {
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
    pub fn build_color_mapping(color_cfg: &[ColorMappingCfg]) -> Self {
        color_cfg
            .iter()
            .flat_map(|c| {
                let ckey = c.color.clone();
                c.targets.iter().map(move |t| (ckey.clone(), t))
            })
            .fold(Self::default(), |acc, (sensor, target)| {
                let fan = FanRef {
                    controller_id: target.controller as usize,
                    channel: target.fan_idx as usize,
                };

                acc.color2fans.entry(sensor).or_default().insert(fan);
                acc
            })
    }

    pub fn color_to_fans_iter(&self) -> impl Iterator<Item = (String, DashSet<FanRef>)> {
        self.color2fans
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
                    controller_id: target.controller as usize,
                    channel: target.fan_idx as usize,
                };

                acc.fans2sensor.insert(fan, sensor.clone());
                acc.sensor2fans.entry(sensor).or_default().insert(fan);
                acc
            })
    }

    /// Attaches a fan to a sensor dynamically.
    ///
    /// Updates the mapping to associate a fan with a specific sensor,
    /// removing any previous association for that fan.
    ///
    /// # Arguments
    ///
    /// * `fan` - Fan reference to attach
    /// * `sensor` - Sensor key to associate with the fan
    #[allow(dead_code)]
    pub fn attach(&self, fan: FanRef, sensor: SensorKey) {
        if let Some(old) = self.fans2sensor.insert(fan, sensor.clone()) {
            if let Some(set) = self.sensor2fans.get(&old) {
                set.remove(&fan);
            }
        }
        self.sensor2fans.entry(sensor).or_default().insert(fan);
    }

    /// Detaches a fan from its current sensor.
    ///
    /// Removes the mapping relationship for the specified fan.
    ///
    /// # Arguments
    ///
    /// * `fan` - Fan reference to detach
    #[allow(dead_code)]
    pub fn detach(&self, fan: FanRef) {
        if let Some((_, key)) = self.fans2sensor.remove(&fan) {
            if let Some(set) = self.sensor2fans.get(&key) {
                set.remove(&fan);
            }
        }
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
            .flat_map(|set| set.iter().map(|r| *r).collect::<Vec<_>>())
    }
}

/// Temperature-based color mapping logic.
///
/// Maps temperature values to RGB colors based on minimum and maximum
/// temperature thresholds and their corresponding color values.
///
/// # Example
///
/// ```
/// use tt_riingd::mappings::color_for_temp;
///
/// // Map temperature to color: 30°C (blue) to 80°C (red)
/// let color = color_for_temp(55.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
/// // Returns interpolated color between blue and red
/// ```
#[allow(dead_code)]
pub fn color_for_temp(
    temp: f32,
    min_temp: f32,
    max_temp: f32,
    min_color: [u8; 3],
    max_color: [u8; 3],
) -> [u8; 3] {
    if temp <= min_temp {
        return min_color;
    }
    if temp >= max_temp {
        return max_color;
    }

    let ratio = (temp - min_temp) / (max_temp - min_temp);
    [
        (min_color[0] as f32 + ratio * (max_color[0] as f32 - min_color[0] as f32)) as u8,
        (min_color[1] as f32 + ratio * (max_color[1] as f32 - min_color[1] as f32)) as u8,
        (min_color[2] as f32 + ratio * (max_color[2] as f32 - min_color[2] as f32)) as u8,
    ]
}

/// Resolves sensor mappings to target channels.
///
/// Takes sensor readings and mapping configuration to determine which
/// fan channels should be controlled based on sensor values.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::mappings::resolve_mappings;
/// use tt_riingd::config::MappingCfg;
/// use std::collections::HashMap;
///
/// let mut temps = HashMap::new();
/// temps.insert("cpu_temp".to_string(), 65.0);
///
/// let mappings = vec![]; // Your mapping configuration
/// let resolved = resolve_mappings(&temps, &mappings);
/// ```
#[allow(dead_code)]
pub fn resolve_mappings(
    temperatures: &HashMap<String, f32>,
    mappings: &[MappingCfg],
) -> HashMap<(u8, u8), f32> {
    let mut result = HashMap::new();

    for mapping in mappings {
        if let Some(&temp) = temperatures.get(&mapping.sensor) {
            for target in &mapping.targets {
                let key = (target.controller, target.fan_idx);
                result.insert(key, temp);
            }
        } else {
            warn!("Temperature sensor '{}' not found", mapping.sensor);
        }
    }

    result
}

/// Resolves color mappings based on temperature readings.
///
/// Maps temperature sensors to color values for RGB lighting control.
///
/// # Example
///
/// ```no_run
/// use tt_riingd::mappings::resolve_color_mappings;
/// use tt_riingd::config::ColorMappingCfg;
/// use std::collections::HashMap;
///
/// let mut temps = HashMap::new();
/// temps.insert("gpu_temp".to_string(), 70.0);
///
/// let color_mappings = vec![]; // Your color mapping configuration
/// let colors = HashMap::new(); // Available colors
/// let resolved = resolve_color_mappings(&temps, &color_mappings, &colors);
/// ```
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FanTarget, MappingCfg};
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    #[test]
    fn color_for_temp_below_min() {
        let color = color_for_temp(10.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        assert_eq!(color, [0, 0, 255]); // Should return min_color (blue)
    }

    #[test]
    fn color_for_temp_above_max() {
        let color = color_for_temp(90.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        assert_eq!(color, [255, 0, 0]); // Should return max_color (red)
    }

    #[test]
    fn color_for_temp_at_min() {
        let color = color_for_temp(30.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        assert_eq!(color, [0, 0, 255]); // Should return min_color (blue)
    }

    #[test]
    fn color_for_temp_at_max() {
        let color = color_for_temp(80.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        assert_eq!(color, [255, 0, 0]); // Should return max_color (red)
    }

    #[test]
    fn color_for_temp_midpoint() {
        let color = color_for_temp(55.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        // At midpoint (55°C), should be halfway between blue and red
        // (55 - 30) / (80 - 30) = 25 / 50 = 0.5
        // Red: 0 + 0.5 * (255 - 0) = 127.5 ≈ 127
        // Green: 0 + 0.5 * (0 - 0) = 0
        // Blue: 255 + 0.5 * (0 - 255) = 127.5 ≈ 127
        assert_eq!(color, [127, 0, 127]);
    }

    #[test]
    fn color_for_temp_quarter_point() {
        let color = color_for_temp(42.5, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        // At quarter point (42.5°C)
        // (42.5 - 30) / (80 - 30) = 12.5 / 50 = 0.25
        // Red: 0 + 0.25 * 255 = 63.75 ≈ 63
        // Blue: 255 + 0.25 * (0 - 255) = 191.25 ≈ 191
        assert_eq!(color, [63, 0, 191]);
    }

    #[test]
    fn color_for_temp_three_quarter_point() {
        let color = color_for_temp(67.5, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        // At three-quarter point (67.5°C)
        // (67.5 - 30) / (80 - 30) = 37.5 / 50 = 0.75
        // Red: 0 + 0.75 * 255 = 191.25 ≈ 191
        // Blue: 255 + 0.75 * (0 - 255) = 63.75 ≈ 63
        assert_eq!(color, [191, 0, 63]);
    }

    #[test]
    fn color_for_temp_reverse_range() {
        // Test with higher colors at lower temps (reverse mapping)
        let color = color_for_temp(55.0, 30.0, 80.0, [255, 0, 0], [0, 0, 255]);
        // At midpoint should be halfway from red to blue
        assert_eq!(color, [127, 0, 127]);
    }

    #[test]
    fn color_for_temp_all_channels_different() {
        // Test with all RGB channels having different start/end values
        let color = color_for_temp(40.0, 20.0, 60.0, [100, 50, 200], [200, 150, 50]);
        // (40 - 20) / (60 - 20) = 20 / 40 = 0.5
        // Red: 100 + 0.5 * (200 - 100) = 150
        // Green: 50 + 0.5 * (150 - 50) = 100
        // Blue: 200 + 0.5 * (50 - 200) = 125
        assert_eq!(color, [150, 100, 125]);
    }

    #[test]
    fn color_for_temp_zero_range() {
        // Edge case: min_temp == max_temp
        let color = color_for_temp(50.0, 50.0, 50.0, [0, 0, 255], [255, 0, 0]);
        // When range is zero, should return min_color
        assert_eq!(color, [0, 0, 255]);
    }

    #[test]
    fn color_for_temp_negative_temperatures() {
        let color = color_for_temp(-10.0, -20.0, 0.0, [0, 255, 0], [255, 255, 0]);
        // (-10 - (-20)) / (0 - (-20)) = 10 / 20 = 0.5
        // Red: 0 + 0.5 * 255 = 127.5 ≈ 127
        // Green: 255 + 0.5 * 0 = 255
        // Blue: 0 + 0.5 * 0 = 0
        assert_eq!(color, [127, 255, 0]);
    }

    #[test]
    fn resolve_mappings_single_sensor_single_target() {
        let mut temperatures = HashMap::new();
        temperatures.insert("cpu_temp".to_string(), 65.0);

        let mappings = vec![MappingCfg {
            sensor: "cpu_temp".to_string(),
            targets: vec![FanTarget {
                controller: 0,
                fan_idx: 1,
            }],
        }];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&(0, 1)), Some(&65.0));
    }

    #[test]
    fn resolve_mappings_single_sensor_multiple_targets() {
        let mut temperatures = HashMap::new();
        temperatures.insert("cpu_temp".to_string(), 72.5);

        let mappings = vec![MappingCfg {
            sensor: "cpu_temp".to_string(),
            targets: vec![
                FanTarget {
                    controller: 0,
                    fan_idx: 1,
                },
                FanTarget {
                    controller: 0,
                    fan_idx: 2,
                },
                FanTarget {
                    controller: 1,
                    fan_idx: 1,
                },
            ],
        }];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 3);
        assert_eq!(result.get(&(0, 1)), Some(&72.5));
        assert_eq!(result.get(&(0, 2)), Some(&72.5));
        assert_eq!(result.get(&(1, 1)), Some(&72.5));
    }

    #[test]
    fn resolve_mappings_multiple_sensors() {
        let mut temperatures = HashMap::new();
        temperatures.insert("cpu_temp".to_string(), 65.0);
        temperatures.insert("gpu_temp".to_string(), 78.0);

        let mappings = vec![
            MappingCfg {
                sensor: "cpu_temp".to_string(),
                targets: vec![FanTarget {
                    controller: 0,
                    fan_idx: 1,
                }],
            },
            MappingCfg {
                sensor: "gpu_temp".to_string(),
                targets: vec![FanTarget {
                    controller: 0,
                    fan_idx: 2,
                }],
            },
        ];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&(0, 1)), Some(&65.0));
        assert_eq!(result.get(&(0, 2)), Some(&78.0));
    }

    #[test]
    fn resolve_mappings_missing_sensor() {
        let temperatures = HashMap::new(); // Empty temperatures

        let mappings = vec![MappingCfg {
            sensor: "nonexistent_sensor".to_string(),
            targets: vec![FanTarget {
                controller: 0,
                fan_idx: 1,
            }],
        }];

        let result = resolve_mappings(&temperatures, &mappings);

        // Should be empty since sensor doesn't exist
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn resolve_mappings_overlapping_targets() {
        let mut temperatures = HashMap::new();
        temperatures.insert("cpu_temp".to_string(), 65.0);
        temperatures.insert("gpu_temp".to_string(), 78.0);

        let mappings = vec![
            MappingCfg {
                sensor: "cpu_temp".to_string(),
                targets: vec![FanTarget {
                    controller: 0,
                    fan_idx: 1,
                }],
            },
            MappingCfg {
                sensor: "gpu_temp".to_string(),
                targets: vec![
                    FanTarget {
                        controller: 0,
                        fan_idx: 1,
                    }, // Same target as CPU
                ],
            },
        ];

        let result = resolve_mappings(&temperatures, &mappings);

        // Should have one entry, with the last mapping value (GPU temp)
        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&(0, 1)), Some(&78.0));
    }

    #[test]
    fn resolve_mappings_empty_inputs() {
        let temperatures = HashMap::new();
        let mappings = vec![];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn resolve_mappings_empty_targets() {
        let mut temperatures = HashMap::new();
        temperatures.insert("cpu_temp".to_string(), 65.0);

        let mappings = vec![MappingCfg {
            sensor: "cpu_temp".to_string(),
            targets: vec![], // No targets
        }];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn resolve_mappings_high_precision_temperature() {
        let mut temperatures = HashMap::new();
        temperatures.insert("precise_sensor".to_string(), 42.123456);

        let mappings = vec![MappingCfg {
            sensor: "precise_sensor".to_string(),
            targets: vec![FanTarget {
                controller: 2,
                fan_idx: 3,
            }],
        }];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(&(2, 3)), Some(&42.123456));
    }

    #[test]
    fn resolve_mappings_extreme_controller_fan_indices() {
        let mut temperatures = HashMap::new();
        temperatures.insert("test_sensor".to_string(), 50.0);

        let mappings = vec![MappingCfg {
            sensor: "test_sensor".to_string(),
            targets: vec![
                FanTarget {
                    controller: 255,
                    fan_idx: 255,
                }, // Max u8 values
                FanTarget {
                    controller: 0,
                    fan_idx: 0,
                }, // Min u8 values
            ],
        }];

        let result = resolve_mappings(&temperatures, &mappings);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get(&(255, 255)), Some(&50.0));
        assert_eq!(result.get(&(0, 0)), Some(&50.0));
    }

    #[test]
    fn color_for_temp_floating_point_precision() {
        // Test floating point precision handling
        let color = color_for_temp(33.333333, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);

        // (33.333333 - 30) / (80 - 30) = 3.333333 / 50 = 0.06666666
        // Red: 0 + 0.06666666 * 255 ≈ 16 (actual result due to f32 precision)
        // Blue: 255 + 0.06666666 * (0 - 255) ≈ 238 (actual result due to f32 precision)
        assert_eq!(color, [16, 0, 238]);
    }

    #[test]
    fn color_for_temp_boundary_precision() {
        // Test near-boundary values for precision
        let color1 = color_for_temp(29.999999, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
        let color2 = color_for_temp(30.000001, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);

        // Just below min should return min_color
        assert_eq!(color1, [0, 0, 255]);

        // Just above min should return almost min_color
        // Due to f32 precision, even tiny differences can result in [0, 0, 254]
        assert_eq!(color2, [0, 0, 254]); // Very close to minimum with slight change
    }
}
