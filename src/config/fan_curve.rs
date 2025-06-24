//! Fan curve data structures and implementations.
//!
//! This module defines the core data structures used to represent fan speed curves
//! and temperature control points. These structures are used throughout the system
//! for converting temperature readings into appropriate fan speeds.

// No additional imports needed
use serde::{Deserialize, Serialize};

use crate::config::cfg::CurveCfg;

/// A point in 2D space representing a temperature/speed coordinate.
///
/// Used as a building block for defining fan curves, where x typically
/// represents temperature in Celsius and y represents fan speed percentage.
///
/// # Example
///
/// ```
/// use tt_riingd::config::fan_curve::Point;
///
/// // Create a point representing 50°C -> 75% fan speed
/// let point = Point { x: 50.0, y: 75.0 };
///
/// // Points can also be created from tuples
/// let point_from_tuple: Point = (30.0, 40.0).into();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Represents different types of fan speed curves.
///
/// Fan curves define how fan speed should change in response to temperature
/// readings. Different curve types provide varying levels of control and
/// smoothness in fan behavior.
///
/// # Variants
///
/// - **Constant**: Fixed fan speed regardless of temperature
/// - **StepCurve**: Linear interpolation between discrete temperature/speed points
/// - **BezierCurve**: Smooth curve interpolation using Bezier control points
///
/// # Example
///
/// ```
/// use tt_riingd::config::fan_curve::{FanCurve, Point};
///
/// // Constant 50% fan speed
/// let constant = FanCurve::Constant(50);
///
/// // Step curve: 30% at 30°C, 80% at 70°C
/// let step = FanCurve::StepCurve {
///     temps: vec![30.0, 70.0],
///     speeds: vec![30, 80],
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            CurveCfg::Constant { speed, .. } => FanCurve::Constant(*speed),
            CurveCfg::StepCurve { tmps, spds, .. } => FanCurve::StepCurve {
                temps: tmps.clone(),
                speeds: spds.clone(),
            },
            CurveCfg::Bezier { points, .. } => FanCurve::BezierCurve {
                points: points.clone(),
            },
        }
    }
}

impl FanCurve {}
