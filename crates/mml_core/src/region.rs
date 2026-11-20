//! Areas enclosed by paths and their offsets, shared by CNC pockets
//! and laser contour fills.

use crate::geometry::{Path, Point};
use geo::{BooleanOps, Buffer, Coord, LineString, MultiPolygon, Polygon};

/// Area enclosed by the closed paths of one shape, even-odd rule.
pub fn region_of(paths: &[Path]) -> MultiPolygon<f64> {
    let mut region = MultiPolygon::<f64>::new(Vec::new());
    for path in paths
        .iter()
        .filter(|path| path.closed && path.points.len() > 2)
    {
        let ring = Polygon::new(to_line_string(path), Vec::new());
        region = if region.0.is_empty() {
            MultiPolygon::new(vec![ring])
        } else {
            region.xor(&MultiPolygon::new(vec![ring]))
        };
    }
    region
}

/// Grows (positive) or shrinks (negative) a region with round joins,
/// which is exactly the locus of a round tool centre.
pub fn offset(region: &MultiPolygon<f64>, distance: f64) -> MultiPolygon<f64> {
    if region.0.is_empty() || distance == 0.0 {
        return region.clone();
    }
    region.buffer(distance)
}

pub fn to_line_string(path: &Path) -> LineString<f64> {
    let mut coords: Vec<Coord<f64>> = path
        .points
        .iter()
        .map(|point| Coord {
            x: point.x,
            y: point.y,
        })
        .collect();
    if let (Some(first), Some(last)) = (coords.first().copied(), coords.last().copied())
        && first != last
    {
        coords.push(first);
    }
    LineString::new(coords)
}

pub fn to_path(ring: &LineString<f64>) -> Path {
    let mut points: Vec<Point> = ring
        .coords()
        .map(|coord| Point::new(coord.x, coord.y))
        .collect();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    Path::new(points, true)
}
