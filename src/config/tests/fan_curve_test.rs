use super::super::cfg::CurveCfg;
use super::super::fan_curve::{FanCurve, Point};

#[test]
fn point_creation_from_tuple() {
    let point: Point = (25.5, 42.3).into();
    std::assert_eq!(point.x, 25.5);
    std::assert_eq!(point.y, 42.3);
}

#[test]
fn point_creation_direct() {
    let point = Point { x: 60.0, y: 85.0 };
    std::assert_eq!(point.x, 60.0);
    std::assert_eq!(point.y, 85.0);
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
    std::assert_eq!(constant1, constant2);

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
        FanCurve::Constant(speed) => std::assert_eq!(speed, 65),
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
            std::assert_eq!(temps, vec![20.0, 40.0, 60.0, 80.0]);
            std::assert_eq!(speeds, vec![20, 40, 70, 100]);
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
            std::assert_eq!(curve_points.len(), 3);
            std::assert_eq!(curve_points[0].x, 0.0);
            std::assert_eq!(curve_points[0].y, 0.0);
            std::assert_eq!(curve_points[2].x, 100.0);
            std::assert_eq!(curve_points[2].y, 100.0);
        }
        _ => panic!("Expected BezierCurve"),
    }
}

#[test]
fn point_debug_format() {
    let point = Point { x: 42.5, y: 88.9 };
    let debug_output = format!("{point:?}");
    assert!(debug_output.contains("42.5"));
    assert!(debug_output.contains("88.9"));
}

#[test]
fn fan_curve_debug_format() {
    let curve = FanCurve::Constant(75);
    let debug_output = format!("{curve:?}");
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
            std::assert_eq!(t1, t2);
            std::assert_eq!(s1, s2);
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
        FanCurve::Constant(speed) => std::assert_eq!(speed, 42),
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
            std::assert_eq!(temps, vec![30.0, 70.0]);
            std::assert_eq!(speeds, vec![40, 90]);
        }
        _ => panic!("Deserialization should preserve curve type"),
    }
}

#[test]
fn serde_serialization_bezier_curve() {
    let curve = FanCurve::BezierCurve {
        points: vec![Point { x: 10.0, y: 20.0 }, Point { x: 90.0, y: 80.0 }],
    };
    let serialized = serde_json::to_string(&curve).unwrap();
    let deserialized: FanCurve = serde_json::from_str(&serialized).unwrap();

    match deserialized {
        FanCurve::BezierCurve { points } => {
            std::assert_eq!(points.len(), 2);
            std::assert_eq!(points[0].x, 10.0);
            std::assert_eq!(points[0].y, 20.0);
            std::assert_eq!(points[1].x, 90.0);
            std::assert_eq!(points[1].y, 80.0);
        }
        _ => panic!("Deserialization should preserve curve type"),
    }
}
