use serde::{Deserialize, Serialize};

use crate::PatternError;

/// Physical millimeters, x right and y up. Never stores screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MmPoint {
    pub x: f64,
    pub y: f64,
}

impl MmPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Evaluate a cubic Bézier at `t` in the inclusive interval `[0, 1]`.
///
/// Controls are ordered as start, first handle, second handle, end, in millimeters.
/// Uses de Casteljau interpolation; rejects nonfinite controls and invalid parameters.
///
/// ```
/// use pattern_core::{MmPoint, cubic_bezier_point};
///
/// let controls = [
///     MmPoint::new(0.0, 0.0),
///     MmPoint::new(0.0, 100.0),
///     MmPoint::new(100.0, 100.0),
///     MmPoint::new(100.0, 0.0),
/// ];
/// assert_eq!(cubic_bezier_point(controls, 0.5).unwrap(), MmPoint::new(50.0, 75.0));
/// ```
pub fn cubic_bezier_point(controls: [MmPoint; 4], t: f64) -> Result<MmPoint, PatternError> {
    if !controls.iter().all(|point| point.is_finite()) {
        return Err(PatternError::Invalid(
            "Bezier controls must be finite millimeters".into(),
        ));
    }
    if !t.is_finite() || !(0.0..=1.0).contains(&t) {
        return Err(PatternError::Invalid(
            "Bezier parameter must be finite and between 0 and 1".into(),
        ));
    }

    // Weighted interpolation avoids overflowing b - a for opposite large coordinates.
    let blend =
        |a: MmPoint, b: MmPoint| MmPoint::new((1.0 - t) * a.x + t * b.x, (1.0 - t) * a.y + t * b.y);
    let [a, b, c, d] = controls;
    let point = blend(
        blend(blend(a, b), blend(b, c)),
        blend(blend(b, c), blend(c, d)),
    );
    if !point.is_finite() {
        return Err(PatternError::Invalid(
            "Bezier evaluation must produce finite millimeters".into(),
        ));
    }
    Ok(point)
}

/// Canvas-local logical display units, x right and y down (egui points, not hardware pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasPoint {
    pub x: f64,
    pub y: f64,
}

impl CanvasPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// View state only: screen = origin + (x, -y) * scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    origin: CanvasPoint,
    scale: f64,
}

impl ViewTransform {
    pub const MIN_SCALE: f64 = 0.01;
    pub const MAX_SCALE: f64 = 100.0;

    pub fn new(origin: CanvasPoint, scale: f64) -> Result<Self, PatternError> {
        if !origin.is_finite()
            || !scale.is_finite()
            || !(Self::MIN_SCALE..=Self::MAX_SCALE).contains(&scale)
        {
            return Err(PatternError::Invalid("Invalid view origin or scale".into()));
        }
        Ok(Self { origin, scale })
    }

    pub fn scale(self) -> f64 {
        self.scale
    }

    pub fn model_to_canvas(self, point: MmPoint) -> CanvasPoint {
        CanvasPoint::new(
            self.origin.x + point.x * self.scale,
            self.origin.y - point.y * self.scale,
        )
    }

    pub fn canvas_to_model(self, point: CanvasPoint) -> MmPoint {
        MmPoint::new(
            (point.x - self.origin.x) / self.scale,
            (self.origin.y - point.y) / self.scale,
        )
    }

    pub fn pan(&mut self, delta: CanvasPoint) -> Result<(), PatternError> {
        let next = Self::new(
            CanvasPoint::new(self.origin.x + delta.x, self.origin.y + delta.y),
            self.scale,
        )?;
        *self = next;
        Ok(())
    }

    /// Keep the physical location beneath the cursor fixed, including at zoom limits.
    pub fn zoom_at(&mut self, anchor: CanvasPoint, factor: f64) -> Result<(), PatternError> {
        if !anchor.is_finite() || !factor.is_finite() || factor <= 0.0 {
            return Err(PatternError::Invalid(
                "Zoom requires a finite anchor and positive factor".into(),
            ));
        }
        let model = self.canvas_to_model(anchor);
        let scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        let next = Self::new(
            CanvasPoint::new(anchor.x - model.x * scale, anchor.y + model.y * scale),
            scale,
        )?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn arch() -> [MmPoint; 4] {
        [
            MmPoint::new(0.0, 0.0),
            MmPoint::new(0.0, 100.0),
            MmPoint::new(100.0, 100.0),
            MmPoint::new(100.0, 0.0),
        ]
    }

    #[test]
    fn cubic_bezier_preserves_endpoints_and_matches_known_interior_points() {
        let controls = arch();
        assert_eq!(cubic_bezier_point(controls, 0.0).unwrap(), controls[0]);
        assert_eq!(cubic_bezier_point(controls, 1.0).unwrap(), controls[3]);
        for (t, expected) in [
            (0.25, MmPoint::new(15.625, 56.25)),
            (0.5, MmPoint::new(50.0, 75.0)),
            (0.75, MmPoint::new(84.375, 56.25)),
        ] {
            let point = cubic_bezier_point(controls, t).unwrap();
            close(point.x, expected.x);
            close(point.y, expected.y);
        }
    }

    #[test]
    fn cubic_bezier_collinear_controls_give_constant_speed_line() {
        let controls = [
            MmPoint::new(-30.0, 12.0),
            MmPoint::new(-10.0, 2.0),
            MmPoint::new(10.0, -8.0),
            MmPoint::new(30.0, -18.0),
        ];
        for t in [0.0, 0.1, 0.25, 0.7, 1.0] {
            let point = cubic_bezier_point(controls, t).unwrap();
            close(point.x, -30.0 + 60.0 * t);
            close(point.y, 12.0 - 30.0 * t);
        }
    }

    #[test]
    fn cubic_bezier_reversing_controls_reverses_parameter_direction() {
        let controls = arch();
        let reversed = [controls[3], controls[2], controls[1], controls[0]];
        for t in [0.0, 0.125, 0.3, 0.5, 1.0] {
            let forward = cubic_bezier_point(controls, t).unwrap();
            let backward = cubic_bezier_point(reversed, 1.0 - t).unwrap();
            close(forward.x, backward.x);
            close(forward.y, backward.y);
        }
    }

    #[test]
    fn cubic_bezier_repeated_controls_and_large_coordinates_remain_finite() {
        let point = MmPoint::new(-123.25, 45.5);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(cubic_bezier_point([point; 4], t).unwrap(), point);
        }
        let large = MmPoint::new(f64::MAX, -f64::MAX);
        assert_eq!(cubic_bezier_point([large; 4], 0.5).unwrap(), large);
        let opposite = MmPoint::new(-f64::MAX, f64::MAX);
        assert_eq!(
            cubic_bezier_point([large, large, opposite, opposite], 0.5).unwrap(),
            MmPoint::new(0.0, 0.0)
        );
    }

    #[test]
    fn cubic_bezier_rejects_nonfinite_controls_and_out_of_range_parameters() {
        for t in [
            -1.0,
            -f64::EPSILON,
            1.0 + f64::EPSILON,
            f64::NAN,
            f64::INFINITY,
        ] {
            assert!(cubic_bezier_point(arch(), t).is_err());
        }
        for i in 0..4 {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                for axis in 0..2 {
                    let mut controls = arch();
                    if axis == 0 {
                        controls[i].x = invalid;
                    } else {
                        controls[i].y = invalid;
                    }
                    // Validate all controls even when evaluating an endpoint.
                    for t in [0.0, 0.5, 1.0] {
                        assert!(cubic_bezier_point(controls, t).is_err());
                    }
                }
            }
        }
    }

    #[test]
    fn transforms_invert_and_flip_y() {
        for scale in [0.01, 0.5, 2.0, 100.0] {
            let view = ViewTransform::new(CanvasPoint::new(125.0, 900.0), scale).unwrap();
            for point in [
                MmPoint::new(0.0, 0.0),
                MmPoint::new(-12.345, 67.89),
                MmPoint::new(400.0, 600.0),
            ] {
                let canvas = view.model_to_canvas(point);
                close(canvas.y, 900.0 - point.y * scale);
                let restored = view.canvas_to_model(canvas);
                close(restored.x, point.x);
                close(restored.y, point.y);
            }
        }
    }

    #[test]
    fn pan_changes_view_only() {
        let mut view = ViewTransform::new(CanvasPoint::new(0.0, 0.0), 2.0).unwrap();
        let point = MmPoint::new(30.0, 40.0);
        view.pan(CanvasPoint::new(10.0, -20.0)).unwrap();
        assert_eq!(view.model_to_canvas(point), CanvasPoint::new(70.0, -100.0));
        assert_eq!(view.canvas_to_model(CanvasPoint::new(70.0, -100.0)), point);
    }

    #[test]
    fn zoom_preserves_cursor_anchor_at_limits() {
        let mut view = ViewTransform::new(CanvasPoint::new(100.0, 800.0), 1.0).unwrap();
        let anchor = CanvasPoint::new(234.0, 321.0);
        let before = view.canvas_to_model(anchor);
        for factor in [2.0, 0.25, 1e10, 1e-10] {
            view.zoom_at(anchor, factor).unwrap();
            let after = view.canvas_to_model(anchor);
            close(before.x, after.x);
            close(before.y, after.y);
            assert!((ViewTransform::MIN_SCALE..=ViewTransform::MAX_SCALE).contains(&view.scale()));
        }
    }

    #[test]
    fn invalid_view_changes_are_atomic() {
        assert!(ViewTransform::new(CanvasPoint::new(0.0, 0.0), 0.0).is_err());
        let mut view = ViewTransform::new(CanvasPoint::new(100.0, 200.0), 1.0).unwrap();
        let before = view;
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(view.zoom_at(CanvasPoint::new(10.0, 20.0), factor).is_err());
            assert_eq!(view, before);
        }
        assert!(view.pan(CanvasPoint::new(f64::NAN, 0.0)).is_err());
        assert_eq!(view, before);
    }
}
