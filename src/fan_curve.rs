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
}
