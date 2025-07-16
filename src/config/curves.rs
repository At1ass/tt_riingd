//! Bezier curve mathematics and fan speed calculations.
//!
//! Contains all mathematical operations for Bezier curve interpolation
//! and fan speed calculation algorithms used by configuration curves.

use crate::{config::fan_curve::Point, config::structures::CurveCfg};
use anyhow::Result;

/// Maximum iterations for Bezier curve binary search.
const MAX_ITERATIONS: usize = 100;

/// Precision epsilon for Bezier curve calculations.
const EPSILON: f32 = 1e-2;

/// Computes a point on a Bezier curve at parameter t.
///
/// # Arguments
///
/// * `pts` - Array of 4 control points defining the Bezier curve
/// * `t` - Parameter value (0.0 to 1.0)
///
/// # Returns
///
/// The computed point on the curve.
pub fn compute_bezier_at_t(pts: &[Point], t: f32) -> Point {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    let x = uuu * pts[0].x + 3.0 * uu * t * pts[1].x + 3.0 * u * tt * pts[2].x + ttt * pts[3].x;
    let y = uuu * pts[0].y + 3.0 * uu * t * pts[1].y + 3.0 * u * tt * pts[2].y + ttt * pts[3].y;

    Point { x, y }
}

/// Finds the fan speed for a given temperature using Bezier curve interpolation.
///
/// Uses binary search to find the parameter t where the curve's x-coordinate
/// matches the given temperature, then returns the corresponding y-coordinate.
///
/// # Arguments
///
/// * `pts` - Array of 4 control points defining the Bezier curve
/// * `temp` - Temperature to find speed for
///
/// # Returns
///
/// The interpolated fan speed for the given temperature.
pub fn get_speed_for_temp(pts: &[Point], temp: f32) -> f32 {
    let mut t_low = 0.0_f32;
    let mut t_high = 1.0_f32;
    let mut t_mid = 0.0_f32;

    for _ in 0..MAX_ITERATIONS {
        t_mid = (t_low + t_high) * 0.5;
        let p = compute_bezier_at_t(pts, t_mid);

        if (p.x - temp).abs() < EPSILON {
            return p.y;
        }
        if p.x < temp {
            t_low = t_mid;
        } else {
            t_high = t_mid;
        }
    }

    let p = compute_bezier_at_t(pts, t_mid);
    p.y
}

impl CurveCfg {
    /// Calculates fan speed based on temperature for this curve configuration.
    ///
    /// # Arguments
    ///
    /// * `temperature` - Temperature in Celsius to calculate speed for
    ///
    /// # Returns
    ///
    /// Fan speed percentage (0-100) or error if calculation fails
    pub fn calculate_speed(&self, temperature: f32) -> Result<u8> {
        match self {
            CurveCfg::Constant { speed, .. } => Ok(*speed),
            CurveCfg::StepCurve { tmps, spds, .. } => {
                if tmps.len() != spds.len() {
                    return Err(anyhow::anyhow!(
                        "Temperature and speed arrays must have the same length".to_string()
                    ));
                }
                if tmps.is_empty() {
                    return Err(anyhow::anyhow!("Step curve cannot be empty".to_string()));
                }
                for i in 0..tmps.len() - 1 {
                    if temperature >= tmps[i] && temperature < tmps[i + 1] {
                        let t = (temperature - tmps[i]) / (tmps[i + 1] - tmps[i]);
                        let speed = (spds[i] as f32 * (1.0 - t) + spds[i + 1] as f32 * t) as u8;
                        return Ok(speed);
                    }
                }
                if temperature < tmps[0] {
                    return Ok(spds[0]);
                }
                spds.last()
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("Empty speeds vector"))
            }
            CurveCfg::Bezier { points, .. } => {
                if points.len() != 4 {
                    return Err(anyhow::anyhow!(
                        "Bezier curve must have exactly 4 control points"
                    ));
                }

                // Сортируем точки по x (температуре) для корректной обработки граничных случаев
                let mut sorted_points = points.clone();
                sorted_points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());

                // Обработка граничных случаев
                if temperature <= sorted_points[0].x {
                    return Ok(sorted_points[0].y.clamp(0.0, 100.0) as u8);
                }
                if temperature >= sorted_points[3].x {
                    return Ok(sorted_points[3].y.clamp(0.0, 100.0) as u8);
                }

                // Используем бинарный поиск для нахождения правильной скорости
                let speed = get_speed_for_temp(points, temperature);
                Ok(speed.clamp(0.0, 100.0) as u8)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::fan_curve::Point;

    #[test]
    fn test_bezier_computation() {
        let points = vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 25.0, y: 25.0 },
            Point { x: 75.0, y: 75.0 },
            Point { x: 100.0, y: 100.0 },
        ];

        let result = compute_bezier_at_t(&points, 0.5);
        assert!((result.x - 50.0).abs() < 1.0);
        assert!((result.y - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_speed_calculation_constant() {
        let curve = CurveCfg::Constant {
            id: "test".to_string(),
            speed: 50,
        };

        assert_eq!(curve.calculate_speed(30.0).unwrap(), 50);
        assert_eq!(curve.calculate_speed(70.0).unwrap(), 50);
    }

    #[test]
    fn test_speed_calculation_step_curve() {
        let curve = CurveCfg::StepCurve {
            id: "test".to_string(),
            tmps: vec![20.0, 40.0, 60.0, 80.0],
            spds: vec![30, 50, 70, 90],
        };

        assert_eq!(curve.calculate_speed(15.0).unwrap(), 30); // Below range
        assert_eq!(curve.calculate_speed(30.0).unwrap(), 40); // Interpolated
        assert_eq!(curve.calculate_speed(85.0).unwrap(), 90); // Above range
    }
}
