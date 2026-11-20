//! Envelope deformations of vector artwork, like WordArt: text on a
//! circle, waves, bulges. Paths are subdivided first so straight edges
//! bend smoothly.

use crate::geometry::{Path, Point, bounds_of};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Warp {
    /// Baseline on top of a circle of this radius (reads over the arc).
    ArcOver {
        radius: f64,
    },
    /// Baseline under a circle of this radius (reads inside a smile).
    ArcUnder {
        radius: f64,
    },
    Wave {
        amplitude: f64,
        wavelength: f64,
    },
    /// Middle taller (positive) or narrower (negative), -0.9..2.
    Bulge {
        amount: f64,
    },
}

fn subdivide(path: &Path, max_length: f64) -> Path {
    let mut points = Vec::with_capacity(path.points.len() * 2);
    for (a, b) in path.segments() {
        points.push(a);
        let pieces = (a.distance(b) / max_length).ceil() as usize;
        for piece in 1..pieces {
            points.push(a.lerp(b, piece as f64 / pieces as f64));
        }
    }
    if !path.closed
        && let Some(last) = path.points.last()
    {
        points.push(*last);
    }
    Path::new(points, path.closed)
}

/// Deforms paths around the bottom centre of their bounds, which stays
/// in place.
pub fn warp_paths(paths: &[Path], warp: Warp) -> Vec<Path> {
    let bounds = bounds_of(paths);
    if bounds.is_empty() {
        return paths.to_vec();
    }
    let anchor = Point::new(bounds.center().x, bounds.min.y);
    let half_width = (bounds.width() / 2.0).max(1e-6);
    let height = bounds.height().max(1e-6);
    let step = (bounds.width().max(bounds.height()) / 200.0).clamp(0.05, 1.0);
    let map = |point: Point| -> Point {
        let x = point.x - anchor.x;
        let y = point.y - anchor.y;
        let local = match warp {
            Warp::ArcOver { radius } => {
                let radius = radius.max(1.0);
                let angle = x / radius;
                Point::new(
                    (radius + y) * angle.sin(),
                    (radius + y) * angle.cos() - radius,
                )
            }
            Warp::ArcUnder { radius } => {
                let radius = radius.max(height + 1.0);
                let angle = x / radius;
                Point::new(
                    (radius - y) * angle.sin(),
                    radius - (radius - y) * angle.cos(),
                )
            }
            Warp::Wave {
                amplitude,
                wavelength,
            } => Point::new(
                x,
                y + amplitude * (std::f64::consts::TAU * x / wavelength.max(1.0)).sin(),
            ),
            Warp::Bulge { amount } => {
                let profile =
                    (std::f64::consts::FRAC_PI_2 * (x / half_width).clamp(-1.0, 1.0)).cos();
                Point::new(x, y * (1.0 + amount.clamp(-0.9, 2.0) * profile))
            }
        };
        local + anchor
    };
    paths
        .iter()
        .map(|path| {
            let dense = subdivide(path, step);
            Path::new(dense.points.into_iter().map(map).collect(), dense.closed)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(width: f64) -> Vec<Path> {
        vec![Path::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(width, 0.0),
                Point::new(width, 1.0),
                Point::new(0.0, 1.0),
            ],
            true,
        )]
    }

    #[test]
    fn arc_over_bends_the_baseline_onto_the_circle() {
        let radius = 20.0;
        let warped = warp_paths(&bar(20.0), Warp::ArcOver { radius });
        let center = Point::new(10.0, -radius);
        let on_circle = warped[0]
            .points
            .iter()
            .filter(|point| (point.distance(center) - radius).abs() < 1e-6)
            .count();
        assert!(on_circle > 10, "the bottom edge must lie on the circle");
        let bounds = bounds_of(&warped);
        assert!(bounds.min.y < -1.0, "the ends go down");
    }

    #[test]
    fn wave_keeps_width_and_moves_vertically() {
        let warped = warp_paths(
            &bar(40.0),
            Warp::Wave {
                amplitude: 3.0,
                wavelength: 20.0,
            },
        );
        let bounds = bounds_of(&warped);
        assert!((bounds.width() - 40.0).abs() < 1e-9);
        assert!(bounds.height() > 6.0);
    }
}
