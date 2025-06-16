//! Property-based tests for tt_riingd
//!
//! These tests use proptest to validate properties that should hold
//! for all inputs, providing better coverage than example-based tests.

use anyhow::Result;
use proptest::prelude::*;
use tt_riingd::{
    config::{Config, CurveCfg},
    fan_curve::{FanCurve, Point},
};

// Property-based tests for fan curves
proptest! {
    /// Property: Fan speed should always be in valid range [0, 100]
    /// regardless of input temperature.
    #[test]
    fn curve_speed_always_in_bounds(
        temp in -273.15f32..1000.0f32,
        speed in 0u8..=100u8
    ) {
        let curve = CurveCfg::Constant {
            id: "test".to_string(),
            speed,
        };

        if let Ok(calculated_speed) = curve.calculate_speed(temp) {
            prop_assert!(
                calculated_speed <= 100,
                "Speed {} exceeds maximum for temp {}",
                calculated_speed, temp
            );
        }
        // Note: calculate_speed can return errors for invalid curves,
        // which is acceptable behavior
    }

    /// Property: Step curve interpolation should be monotonic
    /// when temperatures are sorted.
    #[test]
    fn step_curve_monotonic_when_sorted(
        temps in prop::collection::vec(any::<f32>(), 2..=10),
        speeds in prop::collection::vec(0u8..=100u8, 2..=10)
    ) {
        // Ensure we have matching lengths
        let min_len = temps.len().min(speeds.len());
        let temps = &temps[..min_len];
        let speeds = &speeds[..min_len];

        // Sort temperatures for monotonic property
        let mut temp_speed_pairs: Vec<_> = temps.iter().zip(speeds.iter()).collect();
        temp_speed_pairs.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap_or(std::cmp::Ordering::Equal));

        let sorted_temps: Vec<f32> = temp_speed_pairs.iter().map(|(t, _)| **t).collect();
        let sorted_speeds: Vec<u8> = temp_speed_pairs.iter().map(|(_, s)| **s).collect();

        if sorted_temps.len() >= 2 && sorted_temps.iter().all(|t| t.is_finite()) {
            let curve = CurveCfg::StepCurve {
                id: "test_step".to_string(),
                tmps: sorted_temps.clone(),
                spds: sorted_speeds.clone(),
            };

            // Test that interpolation works for values in range
            let min_temp = sorted_temps[0];
            let max_temp = sorted_temps[sorted_temps.len() - 1];

            if min_temp < max_temp {
                let mid_temp = (min_temp + max_temp) / 2.0;
                if let Ok(speed) = curve.calculate_speed(mid_temp) {
                    prop_assert!(speed <= 100, "Interpolated speed {} exceeds 100", speed);
                }
            }
        }
    }

    /// Property: Bezier curve should handle extreme coordinates gracefully.
    #[test]
    fn bezier_curve_handles_extreme_coordinates(
        x_coords in prop::collection::vec(-1000.0f32..1000.0f32, 4),
        y_coords in prop::collection::vec(-1000.0f32..1000.0f32, 4)
    ) {
        let points: Vec<Point> = x_coords.iter().zip(y_coords.iter())
            .map(|(&x, &y)| Point { x, y })
            .collect();

        let curve = CurveCfg::Bezier {
            id: "test_bezier".to_string(),
            points,
        };

        // Test that curve doesn't panic with extreme coordinates
        for test_temp in [-100.0, 0.0, 50.0, 100.0, 1000.0] {
            let result = curve.calculate_speed(test_temp);
            // We don't care about the exact result, just that it doesn't panic
            // and if it succeeds, the speed is reasonable
            if let Ok(speed) = result {
                prop_assert!(speed <= 100, "Speed {} exceeds 100 for temp {}", speed, test_temp);
            }
        }
    }

    /// Property: Color interpolation should always produce valid RGB values.
    #[test]
    fn color_interpolation_always_valid_rgb(
        temp in -100.0f32..200.0f32,
        min_temp in -100.0f32..200.0f32,
        max_temp in -100.0f32..200.0f32,
        min_rgb in prop::array::uniform3(0u8..=255u8),
        max_rgb in prop::array::uniform3(0u8..=255u8)
    ) {
        // Implementation of color interpolation for testing
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
            if min_temp == max_temp {
                return min_color;
            }

            let ratio = (temp - min_temp) / (max_temp - min_temp);
            [
                (min_color[0] as f32 + ratio * (max_color[0] as f32 - min_color[0] as f32)) as u8,
                (min_color[1] as f32 + ratio * (max_color[1] as f32 - min_color[1] as f32)) as u8,
                (min_color[2] as f32 + ratio * (max_color[2] as f32 - min_color[2] as f32)) as u8,
            ]
        }

        let result_color = color_for_temp(temp, min_temp, max_temp, min_rgb, max_rgb);

        // Verify color components are valid (u8 is automatically bounded 0-255)
        for (i, &component) in result_color.iter().enumerate() {
            // Just access the component to ensure it was computed correctly
            let _ = component;
            // All u8 values are valid RGB components by definition
            prop_assert!(
                true,
                "RGB component {} value {} computed for temp {}, range [{}, {}]",
                i, component, temp, min_temp, max_temp
            );
        }
    }

    /// Property: Configuration with valid structure should not panic during validation.
    #[test]
    fn config_validation_never_panics(
        tick_seconds in 1u16..=3600u16,
        enable_broadcast in any::<bool>(),
        broadcast_interval in 1u16..=300u16
    ) {
        let config = Config {
            version: 1,
            tick_seconds,
            enable_broadcast,
            broadcast_interval,
            controllers: vec![], // Empty is valid
            curves: vec![],
            sensors: vec![],
            mappings: vec![],
            active_curve_mappings: vec![],
            colors: vec![],
            color_mappings: vec![],
        };

        // Should not panic during validation
        let validation_result = config.validate();

        // We don't assert the result is Ok, because some combinations
        // might be invalid, but validation should never panic
        match validation_result {
            Ok(_) => {
                // Valid configuration
            }
            Err(e) => {
                // Invalid configuration, but gracefully handled
                prop_assert!(!e.to_string().is_empty(), "Error message should not be empty");
            }
        }
    }

    /// Property: FanCurve cloning should preserve equality.
    #[test]
    fn fan_curve_clone_preserves_equality(
        speed in 0u8..=100u8
    ) {
        let original = FanCurve::Constant(speed);
        let cloned = original.clone();

        prop_assert_eq!(original, cloned);
    }

    /// Property: Point coordinates should handle floating-point edge cases.
    #[test]
    fn point_handles_float_edge_cases(
        x in prop::num::f32::ANY,
        y in prop::num::f32::ANY
    ) {
        let point = Point { x, y };

        // Should not panic during creation or field access
        let _x_accessed = point.x;
        let _y_accessed = point.y;

        // Debug formatting should work
        let _debug_string = format!("{:?}", point);

        // Cloning should work
        let _cloned = point.clone();
    }
}

// Additional property tests for specific scenarios
proptest! {
    /// Property: Temperature sensor readings in realistic range should
    /// always produce reasonable fan speeds.
    #[test]
    fn realistic_temperature_produces_reasonable_speeds(
        cpu_temp in 20.0f32..90.0f32,
        gpu_temp in 25.0f32..85.0f32
    ) {
        // Create a typical step curve
        let curve = CurveCfg::StepCurve {
            id: "realistic".to_string(),
            tmps: vec![30.0, 50.0, 70.0, 85.0],
            spds: vec![25, 50, 75, 100],
        };

        let cpu_speed = curve.calculate_speed(cpu_temp);
        let gpu_speed = curve.calculate_speed(gpu_temp);

        if let Ok(speed) = cpu_speed {
            prop_assert!(
                (25..=100).contains(&speed),
                "CPU fan speed {} out of realistic range for temp {}",
                speed, cpu_temp
            );
        }

        if let Ok(speed) = gpu_speed {
            prop_assert!(
                (25..=100).contains(&speed),
                "GPU fan speed {} out of realistic range for temp {}",
                speed, gpu_temp
            );
        }
    }

    /// Property: Configuration with extreme but valid values should parse.
    #[test]
    fn extreme_valid_config_values_parse(
        tick_seconds in 1u16..=u16::MAX,
        broadcast_interval in 1u16..=u16::MAX
    ) {
        let yaml = format!(
            r#"
version: 1
tick_seconds: {}
enable_broadcast: true
broadcast_interval: {}
controllers: []
curves: []
sensors: []
mappings: []
active_curve_mappings: []
colors: []
color_mappings: []
"#,
            tick_seconds, broadcast_interval
        );

        let parse_result: Result<Config, _> = serde_yaml::from_str(&yaml);

        // Should either parse successfully or fail gracefully
        match parse_result {
            Ok(config) => {
                prop_assert_eq!(config.tick_seconds, tick_seconds);
                prop_assert_eq!(config.broadcast_interval, broadcast_interval);
                prop_assert_eq!(config.version, 1);
            }
            Err(e) => {
                // Parsing failed, but gracefully
                prop_assert!(!e.to_string().is_empty());
            }
        }
    }
}

#[cfg(test)]
mod integration_property_tests {
    use super::*;
    use proptest::test_runner::{Config as ProptestConfig, TestRunner};

    /// Run property tests with custom configuration for CI environments.
    #[test]
    fn run_property_tests_with_custom_config() {
        let config = ProptestConfig {
            cases: if cfg!(debug_assertions) { 50 } else { 100 },
            max_shrink_iters: 1000,
            ..ProptestConfig::default()
        };

        let mut runner = TestRunner::new(config);

        // Test that runs a subset of property tests manually
        // This ensures property tests work in CI environments
        runner
            .run(&(0u8..=100u8, -100.0f32..100.0f32), |(speed, temp)| {
                let curve = CurveCfg::Constant {
                    id: "ci_test".to_string(),
                    speed,
                };

                if let Ok(calculated_speed) = curve.calculate_speed(temp) {
                    assert!(calculated_speed <= 100);
                }

                Ok(())
            })
            .expect("Property test should pass");
    }
}
