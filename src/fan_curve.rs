use serde::{Deserialize, Serialize};

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
