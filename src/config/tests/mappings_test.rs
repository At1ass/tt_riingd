use super::super::cfg::{FanTarget, MappingCfg};
use std::collections::HashMap;

/// Temperature-based color mapping logic for testing.
fn color_for_temp(
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

/// Resolves sensor mappings to target channels for testing.
fn resolve_mappings(
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
        }
    }

    result
}

#[test]
fn color_for_temp_below_min() {
    let color = color_for_temp(10.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    std::assert_eq!(color, [0, 0, 255]); // Should return min_color (blue)
}

#[test]
fn color_for_temp_above_max() {
    let color = color_for_temp(90.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    std::assert_eq!(color, [255, 0, 0]); // Should return max_color (red)
}

#[test]
fn color_for_temp_at_min() {
    let color = color_for_temp(30.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    std::assert_eq!(color, [0, 0, 255]); // Should return min_color (blue)
}

#[test]
fn color_for_temp_at_max() {
    let color = color_for_temp(80.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    std::assert_eq!(color, [255, 0, 0]); // Should return max_color (red)
}

#[test]
fn color_for_temp_midpoint() {
    let color = color_for_temp(55.0, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    // At midpoint (55°C), should be halfway between blue and red
    // (55 - 30) / (80 - 30) = 25 / 50 = 0.5
    // Red: 0 + 0.5 * (255 - 0) = 127.5 ≈ 127
    // Green: 0 + 0.5 * (0 - 0) = 0
    // Blue: 255 + 0.5 * (0 - 255) = 127.5 ≈ 127
    std::assert_eq!(color, [127, 0, 127]);
}

#[test]
fn color_for_temp_quarter_point() {
    let color = color_for_temp(42.5, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    // At quarter point (42.5°C)
    // (42.5 - 30) / (80 - 30) = 12.5 / 50 = 0.25
    // Red: 0 + 0.25 * 255 = 63.75 ≈ 63
    // Blue: 255 + 0.25 * (0 - 255) = 191.25 ≈ 191
    std::assert_eq!(color, [63, 0, 191]);
}

#[test]
fn color_for_temp_three_quarter_point() {
    let color = color_for_temp(67.5, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    // At three-quarter point (67.5°C)
    // (67.5 - 30) / (80 - 30) = 37.5 / 50 = 0.75
    // Red: 0 + 0.75 * 255 = 191.25 ≈ 191
    // Blue: 255 + 0.75 * (0 - 255) = 63.75 ≈ 63
    std::assert_eq!(color, [191, 0, 63]);
}

#[test]
fn color_for_temp_reverse_range() {
    // Test with higher colors at lower temps (reverse mapping)
    let color = color_for_temp(55.0, 30.0, 80.0, [255, 0, 0], [0, 0, 255]);
    // At midpoint should be halfway from red to blue
    std::assert_eq!(color, [127, 0, 127]);
}

#[test]
fn color_for_temp_all_channels_different() {
    // Test with all RGB channels having different start/end values
    let color = color_for_temp(40.0, 20.0, 60.0, [100, 50, 200], [200, 150, 50]);
    // (40 - 20) / (60 - 20) = 20 / 40 = 0.5
    // Red: 100 + 0.5 * (200 - 100) = 150
    // Green: 50 + 0.5 * (150 - 50) = 100
    // Blue: 200 + 0.5 * (50 - 200) = 125
    std::assert_eq!(color, [150, 100, 125]);
}

#[test]
fn color_for_temp_zero_range() {
    // Edge case: min_temp == max_temp
    let color = color_for_temp(50.0, 50.0, 50.0, [0, 0, 255], [255, 0, 0]);
    // When range is zero, should return min_color
    std::assert_eq!(color, [0, 0, 255]);
}

#[test]
fn color_for_temp_negative_temperatures() {
    let color = color_for_temp(-10.0, -20.0, 0.0, [0, 255, 0], [255, 255, 0]);
    // (-10 - (-20)) / (0 - (-20)) = 10 / 20 = 0.5
    // Red: 0 + 0.5 * 255 = 127.5 ≈ 127
    // Green: 255 + 0.5 * 0 = 255
    // Blue: 0 + 0.5 * 0 = 0
    std::assert_eq!(color, [127, 255, 0]);
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

    std::assert_eq!(result.len(), 1);
    std::assert_eq!(result.get(&(0, 1)), Some(&65.0));
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

    std::assert_eq!(result.len(), 3);
    std::assert_eq!(result.get(&(0, 1)), Some(&72.5));
    std::assert_eq!(result.get(&(0, 2)), Some(&72.5));
    std::assert_eq!(result.get(&(1, 1)), Some(&72.5));
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

    std::assert_eq!(result.len(), 2);
    std::assert_eq!(result.get(&(0, 1)), Some(&65.0));
    std::assert_eq!(result.get(&(0, 2)), Some(&78.0));
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
    std::assert_eq!(result.len(), 0);
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
    std::assert_eq!(result.len(), 1);
    std::assert_eq!(result.get(&(0, 1)), Some(&78.0));
}

#[test]
fn resolve_mappings_empty_inputs() {
    let temperatures = HashMap::new();
    let mappings = vec![];

    let result = resolve_mappings(&temperatures, &mappings);

    std::assert_eq!(result.len(), 0);
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

    std::assert_eq!(result.len(), 0);
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

    std::assert_eq!(result.len(), 1);
    std::assert_eq!(result.get(&(2, 3)), Some(&42.123456));
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

    std::assert_eq!(result.len(), 2);
    std::assert_eq!(result.get(&(255, 255)), Some(&50.0));
    std::assert_eq!(result.get(&(0, 0)), Some(&50.0));
}

#[test]
fn color_for_temp_floating_point_precision() {
    // Test floating point precision handling
    let color = color_for_temp(33.333333, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);

    // (33.333333 - 30) / (80 - 30) = 3.333333 / 50 = 0.06666666
    // Red: 0 + 0.06666666 * 255 ≈ 16 (actual result due to f32 precision)
    // Blue: 255 + 0.06666666 * (0 - 255) ≈ 238 (actual result due to f32 precision)
    std::assert_eq!(color, [16, 0, 238]);
}

#[test]
fn color_for_temp_boundary_precision() {
    // Test near-boundary values for precision
    let color1 = color_for_temp(29.999999, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);
    let color2 = color_for_temp(30.000001, 30.0, 80.0, [0, 0, 255], [255, 0, 0]);

    // Just below min should return min_color
    std::assert_eq!(color1, [0, 0, 255]);

    // Just above min should return almost min_color
    // Due to f32 precision, even tiny differences can result in [0, 0, 254]
    std::assert_eq!(color2, [0, 0, 254]); // Very close to minimum with slight change
}
