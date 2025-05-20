use serde::{Deserialize, Serialize};

use crate::config::CurveCfg;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    x: f32,
    y: f32,
}

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

impl From<CurveCfg> for FanCurve {
    fn from(curve_cfg: CurveCfg) -> Self {
        match curve_cfg {
            CurveCfg::Constant { id: _, speed } => FanCurve::Constant(speed),
            CurveCfg::StepCurve { id: _, tmps, spds } => FanCurve::StepCurve { temps: tmps.clone(), speeds: spds.clone() },
            CurveCfg::Bezier { id: _, points } => FanCurve::BezierCurve { points: points.clone()},
        }
    }
}
