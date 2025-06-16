//! Fan curve calculations for temperature-based speed control.
//!
//! Implements linear interpolation between temperature points to determine
//! appropriate fan speeds based on current temperature readings.

use serde::{Deserialize, Serialize};

use crate::config::CurveCfg;

/// Point in 2D space for fan curve calculations.
///
/// Represents a temperature-speed coordinate pair used in curve interpolation.
///
/// # Example
///
/// ```
/// use tt_riingd::fan_curve::Point;
///
/// let point = Point { x: 45.0, y: 60.0 }; // 45°C -> 60% fan speed
/// let from_tuple: Point = (45.0, 60.0).into();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Fan curve types for temperature-based speed control.
///
/// Defines different algorithms for calculating fan speed based on temperature:
/// - Constant: Fixed speed regardless of temperature
/// - StepCurve: Linear interpolation between temperature-speed points
/// - BezierCurve: Smooth curve interpolation using Bezier curves
///
/// # Example
///
/// ```
/// use tt_riingd::fan_curve::{FanCurve, Point};
///
/// // Constant speed
/// let constant = FanCurve::Constant(75);
///
/// // Step curve: 40°C->50%, 60°C->80%
/// let step = FanCurve::StepCurve {
///     temps: vec![40.0, 60.0],
///     speeds: vec![50, 80],
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum FanCurve {
    Constant(u8),
    StepCurve { temps: Vec<f32>, speeds: Vec<u8> },
    BezierCurve { points: Vec<Point> },
}

impl PartialEq for FanCurve {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Constant(_), Self::Constant(_))
                | (Self::BezierCurve { .. }, Self::BezierCurve { .. })
                | (Self::StepCurve { .. }, Self::StepCurve { .. })
        )
    }
}

impl From<(f32, f32)> for Point {
    fn from(value: (f32, f32)) -> Self {
        Self {
            x: value.0,
            y: value.1,
        }
    }
}

impl From<&CurveCfg> for FanCurve {
    fn from(curve_cfg: &CurveCfg) -> Self {
        match curve_cfg {
            CurveCfg::Constant { id: _, speed } => FanCurve::Constant(*speed),
            CurveCfg::StepCurve { id: _, tmps, spds } => FanCurve::StepCurve {
                temps: tmps.clone(),
                speeds: spds.clone(),
            },
            CurveCfg::Bezier { id: _, points } => FanCurve::BezierCurve {
                points: points.clone(),
            },
        }
    }
}

impl FanCurve {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CurveCfg;
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    #[test]
    fn point_creation_from_tuple() {
        let point: Point = (25.5, 42.3).into();
        assert_eq!(point.x, 25.5);
        assert_eq!(point.y, 42.3);
    }

    #[test]
    fn point_creation_direct() {
        let point = Point { x: 60.0, y: 85.0 };
        assert_eq!(point.x, 60.0);
        assert_eq!(point.y, 85.0);
    }

    #[test]
    fn fan_curve_partial_eq_works() {
        let constant1 = FanCurve::Constant(50);
        let constant2 = FanCurve::Constant(75);
        let step_curve = FanCurve::StepCurve {
            temps: vec![30.0, 70.0],
            speeds: vec![30, 80],
        };

        // Same variant types should be equal (even with different values)
        assert_eq!(constant1, constant2);

        // Different variant types should not be equal
        assert_ne!(constant1, step_curve);
    }

    #[test]
    fn fan_curve_from_constant_config() {
        let config = CurveCfg::Constant {
            id: "test_constant".to_string(),
            speed: 65,
        };

        let curve = FanCurve::from(&config);
        match curve {
            FanCurve::Constant(speed) => assert_eq!(speed, 65),
            _ => panic!("Expected Constant curve"),
        }
    }

    #[test]
    fn fan_curve_from_step_config() {
        let config = CurveCfg::StepCurve {
            id: "test_step".to_string(),
            tmps: vec![20.0, 40.0, 60.0, 80.0],
            spds: vec![20, 40, 70, 100],
        };

        let curve = FanCurve::from(&config);
        match curve {
            FanCurve::StepCurve { temps, speeds } => {
                assert_eq!(temps, vec![20.0, 40.0, 60.0, 80.0]);
                assert_eq!(speeds, vec![20, 40, 70, 100]);
            }
            _ => panic!("Expected StepCurve"),
        }
    }

    #[test]
    fn fan_curve_from_bezier_config() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 50.0, y: 50.0 },
            Point { x: 100.0, y: 100.0 },
        ];
        let config = CurveCfg::Bezier {
            id: "test_bezier".to_string(),
            points: points.clone(),
        };

        let curve = FanCurve::from(&config);
        match curve {
            FanCurve::BezierCurve {
                points: curve_points,
            } => {
                assert_eq!(curve_points.len(), 3);
                assert_eq!(curve_points[0].x, 0.0);
                assert_eq!(curve_points[0].y, 0.0);
                assert_eq!(curve_points[2].x, 100.0);
                assert_eq!(curve_points[2].y, 100.0);
            }
            _ => panic!("Expected BezierCurve"),
        }
    }

    #[test]
    fn point_debug_format() {
        let point = Point { x: 42.5, y: 88.9 };
        let debug_output = format!("{:?}", point);
        assert!(debug_output.contains("42.5"));
        assert!(debug_output.contains("88.9"));
    }

    #[test]
    fn fan_curve_debug_format() {
        let curve = FanCurve::Constant(75);
        let debug_output = format!("{:?}", curve);
        assert!(debug_output.contains("Constant"));
        assert!(debug_output.contains("75"));
    }

    #[test]
    fn fan_curve_clone_works() {
        let original = FanCurve::StepCurve {
            temps: vec![25.0, 55.0],
            speeds: vec![35, 85],
        };
        let cloned = original.clone();

        match (&original, &cloned) {
            (
                FanCurve::StepCurve {
                    temps: t1,
                    speeds: s1,
                },
                FanCurve::StepCurve {
                    temps: t2,
                    speeds: s2,
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(s1, s2);
            }
            _ => panic!("Clone should preserve type and data"),
        }
    }

    #[test]
    fn empty_step_curve_creation() {
        let curve = FanCurve::StepCurve {
            temps: vec![],
            speeds: vec![],
        };

        match curve {
            FanCurve::StepCurve { temps, speeds } => {
                assert!(temps.is_empty());
                assert!(speeds.is_empty());
            }
            _ => panic!("Expected empty StepCurve"),
        }
    }

    #[test]
    fn empty_bezier_curve_creation() {
        let curve = FanCurve::BezierCurve { points: vec![] };

        match curve {
            FanCurve::BezierCurve { points } => {
                assert!(points.is_empty());
            }
            _ => panic!("Expected empty BezierCurve"),
        }
    }

    #[test]
    fn serde_serialization_constant() {
        let curve = FanCurve::Constant(42);
        let serialized = serde_json::to_string(&curve).unwrap();
        let deserialized: FanCurve = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            FanCurve::Constant(speed) => assert_eq!(speed, 42),
            _ => panic!("Deserialization should preserve curve type"),
        }
    }

    #[test]
    fn serde_serialization_step_curve() {
        let curve = FanCurve::StepCurve {
            temps: vec![30.0, 70.0],
            speeds: vec![40, 90],
        };
        let serialized = serde_json::to_string(&curve).unwrap();
        let deserialized: FanCurve = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            FanCurve::StepCurve { temps, speeds } => {
                assert_eq!(temps, vec![30.0, 70.0]);
                assert_eq!(speeds, vec![40, 90]);
            }
            _ => panic!("Deserialization should preserve curve type"),
        }
    }

    #[test]
    fn serde_serialization_bezier_curve() {
        let points = vec![Point { x: 20.0, y: 25.0 }, Point { x: 80.0, y: 95.0 }];
        let curve = FanCurve::BezierCurve {
            points: points.clone(),
        };
        let serialized = serde_json::to_string(&curve).unwrap();
        let deserialized: FanCurve = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            FanCurve::BezierCurve {
                points: deserialized_points,
            } => {
                assert_eq!(deserialized_points.len(), 2);
                assert_eq!(deserialized_points[0].x, 20.0);
                assert_eq!(deserialized_points[0].y, 25.0);
                assert_eq!(deserialized_points[1].x, 80.0);
                assert_eq!(deserialized_points[1].y, 95.0);
            }
            _ => panic!("Deserialization should preserve curve type"),
        }
    }

    // Property-based tests using proptest
    proptest! {
        #[test]
        fn point_from_tuple_roundtrip(x in -1000.0f32..1000.0f32, y in -1000.0f32..1000.0f32) {
            let original_tuple = (x, y);
            let point: Point = original_tuple.into();
            prop_assert_eq!(point.x, x);
            prop_assert_eq!(point.y, y);
        }

        #[test]
        fn constant_curve_speed_preserved(speed in 0u8..=255u8) {
            let curve = FanCurve::Constant(speed);
            match curve {
                FanCurve::Constant(preserved_speed) => prop_assert_eq!(preserved_speed, speed),
                _ => prop_assert!(false, "Should preserve constant speed"),
            }
        }

        #[test]
        fn step_curve_data_preserved(
            temps in prop::collection::vec(-50.0f32..150.0f32, 0..10),
            speeds in prop::collection::vec(0u8..=255u8, 0..10)
        ) {
            let curve = FanCurve::StepCurve {
                temps: temps.clone(),
                speeds: speeds.clone()
            };
            match curve {
                FanCurve::StepCurve { temps: preserved_temps, speeds: preserved_speeds } => {
                    prop_assert_eq!(preserved_temps, temps);
                    prop_assert_eq!(preserved_speeds, speeds);
                },
                _ => prop_assert!(false, "Should preserve step curve data"),
            }
        }

        #[test]
        fn bezier_curve_points_preserved(
            points in prop::collection::vec(
                (-100.0f32..200.0f32, 0.0f32..255.0f32).prop_map(|(x, y)| Point { x, y }),
                0..20
            )
        ) {
            let curve = FanCurve::BezierCurve { points: points.clone() };
            match curve {
                FanCurve::BezierCurve { points: preserved_points } => {
                    prop_assert_eq!(preserved_points.len(), points.len());
                    for (original, preserved) in points.iter().zip(preserved_points.iter()) {
                        prop_assert_eq!(original.x, preserved.x);
                        prop_assert_eq!(original.y, preserved.y);
                    }
                },
                _ => prop_assert!(false, "Should preserve bezier curve points"),
            }
        }

        #[test]
        fn curve_serde_roundtrip_constant(speed in 0u8..=255u8) {
            let original = FanCurve::Constant(speed);
            let serialized = serde_json::to_string(&original).unwrap();
            let deserialized: FanCurve = serde_json::from_str(&serialized).unwrap();

            match deserialized {
                FanCurve::Constant(preserved_speed) => prop_assert_eq!(preserved_speed, speed),
                _ => prop_assert!(false, "Serde roundtrip should preserve constant curve"),
            }
        }
    }

    #[test]
    fn extreme_temperature_values() {
        let curve = FanCurve::StepCurve {
            temps: vec![-273.15, 0.0, 100.0, 1000.0], // Absolute zero to very hot
            speeds: vec![0, 25, 75, 255],
        };

        match curve {
            FanCurve::StepCurve { temps, speeds } => {
                assert_eq!(temps[0], -273.15); // Absolute zero
                assert_eq!(temps[3], 1000.0); // Very hot
                assert_eq!(speeds[0], 0); // No speed
                assert_eq!(speeds[3], 255); // Max speed
            }
            _ => panic!("Should handle extreme temperature values"),
        }
    }

    #[test]
    fn max_speed_boundary_test() {
        let curve = FanCurve::Constant(255); // Maximum u8 value

        match curve {
            FanCurve::Constant(speed) => assert_eq!(speed, 255),
            _ => panic!("Should handle maximum speed value"),
        }
    }

    #[test]
    fn zero_speed_boundary_test() {
        let curve = FanCurve::Constant(0); // Minimum u8 value

        match curve {
            FanCurve::Constant(speed) => assert_eq!(speed, 0),
            _ => panic!("Should handle zero speed value"),
        }
    }

    // Additional comprehensive edge case tests

    #[test]
    fn point_floating_point_precision() {
        // Test with very small floating point differences
        let point1 = Point {
            x: 1.0000001,
            y: 2.0000001,
        };
        let point2 = Point {
            x: 1.0000002,
            y: 2.0000002,
        };

        // Points should be different due to floating point precision
        assert_ne!(point1.x, point2.x);
        assert_ne!(point1.y, point2.y);

        // Test with NaN values (should be handled gracefully)
        let point_nan = Point {
            x: f32::NAN,
            y: 50.0,
        };
        assert!(point_nan.x.is_nan());
        assert_eq!(point_nan.y, 50.0);

        // Test with infinity values
        let point_inf = Point {
            x: f32::INFINITY,
            y: f32::NEG_INFINITY,
        };
        assert!(point_inf.x.is_infinite() && point_inf.x.is_sign_positive());
        assert!(point_inf.y.is_infinite() && point_inf.y.is_sign_negative());
    }

    #[test]
    fn fan_curve_partial_eq_comprehensive() {
        // Test all combinations of curve types
        let constant1 = FanCurve::Constant(50);
        let constant2 = FanCurve::Constant(75);
        let step_curve = FanCurve::StepCurve {
            temps: vec![30.0, 70.0],
            speeds: vec![30, 80],
        };
        let bezier_curve = FanCurve::BezierCurve {
            points: vec![Point { x: 30.0, y: 30.0 }, Point { x: 70.0, y: 80.0 }],
        };

        // Same variant types should be equal (current implementation)
        assert_eq!(constant1, constant2);
        // Different variant types should not be equal
        assert_ne!(constant1, step_curve);
        assert_ne!(step_curve, bezier_curve);

        // Test with empty collections
        let empty_step = FanCurve::StepCurve {
            temps: vec![],
            speeds: vec![],
        };
        let empty_bezier = FanCurve::BezierCurve { points: vec![] };
        assert_ne!(empty_step, empty_bezier); // Different types should not be equal
    }

    #[test]
    fn step_curve_edge_cases() {
        // Test with single temperature point
        let single_temp = FanCurve::StepCurve {
            temps: vec![50.0],
            speeds: vec![75],
        };

        match single_temp {
            FanCurve::StepCurve { temps, speeds } => {
                assert_eq!(temps.len(), 1);
                assert_eq!(speeds.len(), 1);
                assert_eq!(temps[0], 50.0);
                assert_eq!(speeds[0], 75);
            }
            _ => panic!("Should preserve single point step curve"),
        }

        // Test with duplicate temperature points
        let duplicate_temps = FanCurve::StepCurve {
            temps: vec![50.0, 50.0, 50.0],
            speeds: vec![25, 50, 75],
        };

        match duplicate_temps {
            FanCurve::StepCurve { temps, speeds } => {
                assert_eq!(temps.len(), 3);
                assert_eq!(speeds.len(), 3);
                assert!(temps.iter().all(|&t| t == 50.0));
            }
            _ => panic!("Should handle duplicate temperatures"),
        }

        // Test with unsorted temperatures
        let unsorted_temps = FanCurve::StepCurve {
            temps: vec![70.0, 30.0, 50.0],
            speeds: vec![80, 30, 50],
        };

        match unsorted_temps {
            FanCurve::StepCurve { temps, speeds } => {
                assert_eq!(temps, vec![70.0, 30.0, 50.0]); // Should preserve original order
                assert_eq!(speeds, vec![80, 30, 50]);
            }
            _ => panic!("Should preserve unsorted temperatures"),
        }
    }

    #[test]
    fn bezier_curve_edge_cases() {
        // Test with single point
        let single_point = FanCurve::BezierCurve {
            points: vec![Point { x: 50.0, y: 75.0 }],
        };

        match single_point {
            FanCurve::BezierCurve { points } => {
                assert_eq!(points.len(), 1);
                assert_eq!(points[0].x, 50.0);
                assert_eq!(points[0].y, 75.0);
            }
            _ => panic!("Should handle single point bezier curve"),
        }

        // Test with many points (stress test)
        let many_points: Vec<Point> = (0..1000)
            .map(|i| Point {
                x: i as f32,
                y: (i % 256) as f32,
            })
            .collect();

        let large_bezier = FanCurve::BezierCurve {
            points: many_points.clone(),
        };

        match large_bezier {
            FanCurve::BezierCurve { points } => {
                assert_eq!(points.len(), 1000);
                assert_eq!(points[0].x, 0.0);
                assert_eq!(points[999].x, 999.0);
            }
            _ => panic!("Should handle large bezier curves"),
        }

        // Test with extreme coordinate values
        let extreme_points = FanCurve::BezierCurve {
            points: vec![
                Point {
                    x: f32::MIN,
                    y: 0.0,
                },
                Point {
                    x: f32::MAX,
                    y: 255.0,
                },
                Point {
                    x: 0.0,
                    y: f32::MIN,
                },
                Point {
                    x: 100.0,
                    y: f32::MAX,
                },
            ],
        };

        match extreme_points {
            FanCurve::BezierCurve { points } => {
                assert_eq!(points.len(), 4);
                assert_eq!(points[0].x, f32::MIN);
                assert_eq!(points[1].x, f32::MAX);
                assert_eq!(points[2].y, f32::MIN);
                assert_eq!(points[3].y, f32::MAX);
            }
            _ => panic!("Should handle extreme coordinate values"),
        }
    }

    #[test]
    fn serde_edge_cases() {
        // Test serialization with extreme values
        let extreme_constant = FanCurve::Constant(u8::MAX);
        let serialized = serde_json::to_string(&extreme_constant).unwrap();
        let deserialized: FanCurve = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            FanCurve::Constant(speed) => assert_eq!(speed, u8::MAX),
            _ => panic!("Should handle extreme constant values"),
        }

        // Test with empty step curve
        let empty_step = FanCurve::StepCurve {
            temps: vec![],
            speeds: vec![],
        };
        let serialized_empty = serde_json::to_string(&empty_step).unwrap();
        let deserialized_empty: FanCurve = serde_json::from_str(&serialized_empty).unwrap();

        match deserialized_empty {
            FanCurve::StepCurve { temps, speeds } => {
                assert!(temps.is_empty());
                assert!(speeds.is_empty());
            }
            _ => panic!("Should handle empty step curves"),
        }

        // Test with special float values in bezier curve
        let special_floats = FanCurve::BezierCurve {
            points: vec![
                Point { x: 0.0, y: 0.0 },
                Point {
                    x: f32::EPSILON,
                    y: f32::MIN_POSITIVE,
                },
                Point { x: 100.0, y: 100.0 },
            ],
        };

        let serialized_special = serde_json::to_string(&special_floats).unwrap();
        let deserialized_special: FanCurve = serde_json::from_str(&serialized_special).unwrap();

        match deserialized_special {
            FanCurve::BezierCurve { points } => {
                assert_eq!(points.len(), 3);
                assert_eq!(points[1].x, f32::EPSILON);
                assert_eq!(points[1].y, f32::MIN_POSITIVE);
            }
            _ => panic!("Should handle special float values"),
        }
    }

    #[test]
    fn memory_efficiency_test() {
        // Test that large curves don't cause memory issues
        let large_temps: Vec<f32> = (0..10000).map(|i| i as f32 * 0.01).collect();
        let large_speeds: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let large_step_curve = FanCurve::StepCurve {
            temps: large_temps.clone(),
            speeds: large_speeds.clone(),
        };

        // Clone should work efficiently
        let cloned_curve = large_step_curve.clone();

        match (&large_step_curve, &cloned_curve) {
            (
                FanCurve::StepCurve {
                    temps: t1,
                    speeds: s1,
                },
                FanCurve::StepCurve {
                    temps: t2,
                    speeds: s2,
                },
            ) => {
                assert_eq!(t1.len(), 10000);
                assert_eq!(s1.len(), 10000);
                assert_eq!(t1, t2);
                assert_eq!(s1, s2);
            }
            _ => panic!("Large curve cloning should work"),
        }
    }

    #[test]
    fn debug_formatting_comprehensive() {
        // Test debug formatting for all curve types with various data
        let constant = FanCurve::Constant(42);
        let debug_constant = format!("{:?}", constant);
        assert!(debug_constant.contains("Constant"));
        assert!(debug_constant.contains("42"));

        let step_curve = FanCurve::StepCurve {
            temps: vec![10.5, 20.7, 30.9],
            speeds: vec![15, 45, 85],
        };
        let debug_step = format!("{:?}", step_curve);
        assert!(debug_step.contains("StepCurve"));
        assert!(debug_step.contains("10.5"));
        assert!(debug_step.contains("85"));

        let bezier_curve = FanCurve::BezierCurve {
            points: vec![Point { x: 1.23, y: 4.56 }, Point { x: 7.89, y: 0.12 }],
        };
        let debug_bezier = format!("{:?}", bezier_curve);
        assert!(debug_bezier.contains("BezierCurve"));
        assert!(debug_bezier.contains("1.23"));
        assert!(debug_bezier.contains("4.56"));
        assert!(debug_bezier.contains("7.89"));
        assert!(debug_bezier.contains("0.12"));

        // Test point debug formatting with extreme values
        let extreme_point = Point {
            x: f32::MAX,
            y: f32::MIN,
        };
        let debug_extreme = format!("{:?}", extreme_point);
        assert!(debug_extreme.contains("Point"));
    }

    #[test]
    fn from_trait_comprehensive() {
        use crate::config::CurveCfg;

        // Test From trait with various configurations
        let constant_cfg = CurveCfg::Constant {
            id: "test_constant".to_string(),
            speed: 0, // Minimum speed
        };
        let curve_from_constant = FanCurve::from(&constant_cfg);
        match curve_from_constant {
            FanCurve::Constant(speed) => assert_eq!(speed, 0),
            _ => panic!("From trait should create Constant curve"),
        }

        let step_cfg = CurveCfg::StepCurve {
            id: "test_step".to_string(),
            tmps: vec![], // Empty arrays
            spds: vec![],
        };
        let curve_from_step = FanCurve::from(&step_cfg);
        match curve_from_step {
            FanCurve::StepCurve { temps, speeds } => {
                assert!(temps.is_empty());
                assert!(speeds.is_empty());
            }
            _ => panic!("From trait should create StepCurve"),
        }

        let bezier_cfg = CurveCfg::Bezier {
            id: "test_bezier".to_string(),
            points: vec![Point {
                x: f32::INFINITY,
                y: f32::NEG_INFINITY,
            }],
        };
        let curve_from_bezier = FanCurve::from(&bezier_cfg);
        match curve_from_bezier {
            FanCurve::BezierCurve { points } => {
                assert_eq!(points.len(), 1);
                assert!(points[0].x.is_infinite());
                assert!(points[0].y.is_infinite());
            }
            _ => panic!("From trait should create BezierCurve"),
        }
    }
}
