//! Involute spur gear outline, to cut gears that really mesh.

use crate::geometry::{Path, Point};
use std::f64::consts::{PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearParams {
    pub teeth: usize,
    /// Module: pitch diameter divided by the number of teeth, mm. Two
    /// gears mesh when they share the module and the pressure angle.
    pub module: f64,
    pub pressure_angle: f64,
    /// Diameter of the centre hole, mm (0 for none).
    pub bore: f64,
    /// Play removed from each tooth flank so cut gears turn freely, mm.
    pub backlash: f64,
}

impl Default for GearParams {
    fn default() -> Self {
        Self {
            teeth: 20,
            module: 2.0,
            pressure_angle: 20.0,
            bore: 5.0,
            backlash: 0.1,
        }
    }
}

impl GearParams {
    pub fn pitch_diameter(&self) -> f64 {
        self.module * self.teeth.max(6) as f64
    }

    pub fn outside_diameter(&self) -> f64 {
        self.pitch_diameter() + 2.0 * self.module
    }
}

/// Angle of the involute at radius `r` for a base circle `base`.
fn involute(base: f64, r: f64) -> f64 {
    let alpha = (base / r).clamp(-1.0, 1.0).acos();
    alpha.tan() - alpha
}

/// Outline (and bore) centred on the origin.
pub fn generate(params: &GearParams) -> Vec<Path> {
    let teeth = params.teeth.max(6);
    let m = params.module.max(0.1);
    let pitch = m * teeth as f64 / 2.0;
    let base = pitch * params.pressure_angle.to_radians().cos();
    let outside = pitch + m;
    let root = (pitch - 1.25 * m).max(0.1);
    // Half the tooth thickness at the pitch circle, as an angle, minus
    // the backlash shared between both flanks.
    let half_tooth = PI / (2.0 * teeth as f64) - params.backlash / 2.0 / pitch;
    let pitch_inv = involute(base, pitch);
    let start = root.max(base);
    let steps = 8;
    let mut points = Vec::new();
    for tooth in 0..teeth {
        let centre = tooth as f64 * TAU / teeth as f64;
        let flank = |r: f64, side: f64| {
            let angle = centre + side * (half_tooth + pitch_inv - involute(base, r));
            Point::new(r * angle.cos(), r * angle.sin())
        };
        // Root before the tooth, rising flank, tip, falling flank.
        let root_angle = centre - (PI / teeth as f64);
        points.push(Point::new(root * root_angle.cos(), root * root_angle.sin()));
        if root < base {
            let a = centre - (half_tooth + pitch_inv);
            points.push(Point::new(root * a.cos(), root * a.sin()));
        }
        for step in 0..=steps {
            let r = start + (outside - start) * step as f64 / steps as f64;
            points.push(flank(r, -1.0));
        }
        for step in (0..=steps).rev() {
            let r = start + (outside - start) * step as f64 / steps as f64;
            points.push(flank(r, 1.0));
        }
        if root < base {
            let a = centre + (half_tooth + pitch_inv);
            points.push(Point::new(root * a.cos(), root * a.sin()));
        }
    }
    let mut paths = vec![Path::new(points, true)];
    if params.bore > 0.0 && params.bore < 2.0 * root {
        let r = params.bore / 2.0;
        let segments = 48;
        paths.push(Path::new(
            (0..segments)
                .map(|i| {
                    let a = i as f64 / segments as f64 * TAU;
                    Point::new(r * a.cos(), r * a.sin())
                })
                .collect(),
            true,
        ));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_spans_the_outside_diameter_with_a_bore() {
        let params = GearParams::default();
        let paths = generate(&params);
        assert_eq!(paths.len(), 2);
        let bounds = paths[0].bounds();
        let size = bounds.width().max(bounds.height());
        assert!((size - params.outside_diameter()).abs() < 0.2, "{size}");
        // Every point lies between the root and outside circles.
        let (root, outside) = (20.0 - 2.5, 22.0);
        for point in &paths[0].points {
            let r = (point.x * point.x + point.y * point.y).sqrt();
            assert!(r > root - 1e-6 && r < outside + 1e-6, "{r}");
        }
    }

    #[test]
    fn teeth_are_counted() {
        let params = GearParams {
            teeth: 12,
            ..GearParams::default()
        };
        let outline = &generate(&params)[0];
        let outside = params.outside_diameter() / 2.0;
        // Tip points come in pairs per tooth.
        let tips = outline
            .points
            .iter()
            .filter(|p| ((p.x * p.x + p.y * p.y).sqrt() - outside).abs() < 1e-6)
            .count();
        assert_eq!(tips, 24);
    }
}
