//! Brush strokes drawn with a stylus: a thin line stays a centre line
//! (line engraving, cutting), a marker or a pressure brush becomes the
//! filled area the stroke covers.

use crate::geometry::{Path, Point, simplify_polyline, smooth_polyline};
use crate::region::to_path;
use geo::{Buffer, ConvexHull, Coord, LineString, MultiPoint, Polygon};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrushStyle {
    /// Centre line, whatever the width shown on the tablet.
    #[default]
    Line,
    /// Constant width, filled.
    Marker,
    /// Width following the stylus pressure, filled.
    Brush,
}

/// Point of a stroke with the stylus pressure (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokePoint {
    pub at: Point,
    pub pressure: f64,
}

/// Lightest pressure still drawn, as a share of the full width.
const MIN_SHARE: f64 = 0.15;
/// Segments of a circle in caps and joints.
const ROUND: usize = 12;

/// Smoothed, simplified centre line of a stroke, closed when it ends
/// where it started.
pub fn centre_line(points: &[StrokePoint]) -> Path {
    let raw: Vec<Point> = points.iter().map(|point| point.at).collect();
    let smoothed = smooth_polyline(&raw, 5);
    let mut line = simplify_polyline(&smoothed, 0.05);
    let closed = line.len() > 3 && line[0].distance(*line.last().unwrap()) < 1.5;
    if closed {
        line.pop();
    }
    Path::new(line, closed)
}

/// Outline of the area covered by a stroke `width` mm wide (the full
/// width at full pressure for a brush).
pub fn outline(points: &[StrokePoint], style: BrushStyle, width: f64) -> Vec<Path> {
    let width = width.max(0.05);
    let line = centre_line(points);
    if line.points.is_empty() {
        return Vec::new();
    }
    match style {
        BrushStyle::Line => vec![line],
        BrushStyle::Marker => {
            let mut coords: Vec<Coord<f64>> = line
                .points
                .iter()
                .map(|point| Coord {
                    x: point.x,
                    y: point.y,
                })
                .collect();
            if line.closed {
                coords.push(coords[0]);
            }
            if coords.len() == 1 {
                coords.push(coords[0]);
            }
            rings(&LineString::new(coords).buffer(width / 2.0))
        }
        BrushStyle::Brush => {
            let radii = pressure_radii(points, &line.points, width);
            let mut pieces: Vec<Polygon<f64>> = Vec::new();
            let mut previous: Option<(Point, f64)> = None;
            for (point, radius) in line.points.iter().copied().zip(radii) {
                if let Some((start, start_radius)) = previous {
                    pieces.push(capsule(start, start_radius, point, radius));
                }
                previous = Some((point, radius));
            }
            if pieces.is_empty()
                && let Some((point, radius)) = previous
            {
                pieces.push(capsule(point, radius, point, radius));
            }
            rings(&geo::unary_union(&pieces))
        }
    }
}

/// Radius at each point of the simplified line, from the pressure of
/// the nearest raw point, smoothed so the edge does not wobble.
fn pressure_radii(raw: &[StrokePoint], line: &[Point], width: f64) -> Vec<f64> {
    let radius =
        |pressure: f64| width / 2.0 * (MIN_SHARE + (1.0 - MIN_SHARE) * pressure.clamp(0.0, 1.0));
    let mut radii: Vec<f64> = line
        .iter()
        .map(|point| {
            raw.iter()
                .min_by(|a, b| a.at.distance(*point).total_cmp(&b.at.distance(*point)))
                .map_or(width / 2.0, |nearest| radius(nearest.pressure))
        })
        .collect();
    for _ in 0..2 {
        let copy = radii.clone();
        for index in 1..radii.len().saturating_sub(1) {
            radii[index] = (copy[index - 1] + 2.0 * copy[index] + copy[index + 1]) / 4.0;
        }
    }
    radii
}

/// Convex hull of two discs: one segment of a variable-width stroke.
fn capsule(a: Point, radius_a: f64, b: Point, radius_b: f64) -> Polygon<f64> {
    let mut coords = Vec::with_capacity(2 * ROUND);
    for (center, radius) in [(a, radius_a), (b, radius_b)] {
        for step in 0..ROUND {
            let angle = step as f64 / ROUND as f64 * std::f64::consts::TAU;
            coords.push(geo::Point::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            ));
        }
    }
    MultiPoint::new(coords).convex_hull()
}

fn rings(area: &geo::MultiPolygon<f64>) -> Vec<Path> {
    area.0
        .iter()
        .flat_map(|polygon| std::iter::once(polygon.exterior()).chain(polygon.interiors()))
        .map(to_path)
        .filter(|path| path.points.len() > 2)
        .collect()
}

/// Exposed for tests: area enclosed by outline rings (even-odd).
pub fn area(paths: &[Path]) -> f64 {
    use geo::Area;
    crate::region::region_of(paths).unsigned_area()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(pressures: &[f64]) -> Vec<StrokePoint> {
        pressures
            .iter()
            .enumerate()
            .map(|(index, pressure)| StrokePoint {
                at: Point::new(index as f64 * 2.0, 0.0),
                pressure: *pressure,
            })
            .collect()
    }

    #[test]
    fn a_line_stays_a_centre_line() {
        let paths = outline(&straight(&[0.5; 11]), BrushStyle::Line, 3.0);
        assert_eq!(paths.len(), 1);
        assert!(!paths[0].closed);
    }

    #[test]
    fn a_marker_covers_its_width() {
        // 20 mm long, 2 mm wide, round ends: 40 + π.
        let paths = outline(&straight(&[0.5; 11]), BrushStyle::Marker, 2.0);
        assert!(paths.iter().all(|path| path.closed));
        assert!((area(&paths) - (40.0 + std::f64::consts::PI)).abs() < 0.5);
    }

    #[test]
    fn a_brush_widens_with_pressure() {
        let light = area(&outline(&straight(&[0.1; 11]), BrushStyle::Brush, 4.0));
        let firm = area(&outline(&straight(&[1.0; 11]), BrushStyle::Brush, 4.0));
        assert!(firm > light * 2.0, "{light} vs {firm}");
        // One piece, no hole, even with growing pressure.
        let ramp: Vec<f64> = (0..11).map(|index| index as f64 / 10.0).collect();
        let paths = outline(&straight(&ramp), BrushStyle::Brush, 4.0);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn a_dot_still_leaves_a_mark() {
        let dot = [StrokePoint {
            at: Point::new(5.0, 5.0),
            pressure: 1.0,
        }];
        assert!(area(&outline(&dot, BrushStyle::Brush, 2.0)) > 2.0);
        assert!(area(&outline(&dot, BrushStyle::Marker, 2.0)) > 2.0);
    }
}
