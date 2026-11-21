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

/// Contour at `distance` from some paths (outside when positive): the
/// enclosed area grows or shrinks with round corners, and open lines
/// get the outline around them when growing.
pub fn offset_paths(paths: &[Path], distance: f64) -> Vec<Path> {
    let mut area = offset(&region_of(paths), distance);
    if distance > 0.0 {
        for open in paths
            .iter()
            .filter(|path| !path.closed && path.points.len() > 1)
        {
            let line = LineString::new(
                open.points
                    .iter()
                    .map(|point| Coord {
                        x: point.x,
                        y: point.y,
                    })
                    .collect(),
            );
            area = area.union(&line.buffer(distance));
        }
    }
    area.0
        .iter()
        .flat_map(|polygon| std::iter::once(polygon.exterior()).chain(polygon.interiors()))
        .map(to_path)
        .filter(|path| path.points.len() > 2)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boolean {
    /// Everything covered by any shape.
    Union,
    /// The first shape minus all the others.
    Difference,
    /// What all the shapes cover together.
    Intersection,
    /// What exactly one of two shapes covers.
    Exclusion,
}

/// Combines the areas of several shapes (each a list of closed paths,
/// holes by even-odd); `None` when nothing is left.
pub fn combine(shapes: &[Vec<Path>], operation: Boolean) -> Option<Vec<Path>> {
    let mut regions = shapes
        .iter()
        .map(|paths| region_of(paths))
        .filter(|r| !r.0.is_empty());
    let first = regions.next()?;
    let result = regions.fold(first, |acc, next| match operation {
        Boolean::Union => acc.union(&next),
        Boolean::Difference => acc.difference(&next),
        Boolean::Intersection => acc.intersection(&next),
        Boolean::Exclusion => acc.xor(&next),
    });
    let rings: Vec<Path> = result
        .0
        .iter()
        .flat_map(|polygon| std::iter::once(polygon.exterior()).chain(polygon.interiors()))
        .map(to_path)
        .filter(|path| path.points.len() > 2)
        .collect();
    (!rings.is_empty()).then_some(rings)
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

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Area;

    fn square(x: f64, y: f64, size: f64) -> Vec<Path> {
        vec![Path::new(
            vec![
                Point::new(x, y),
                Point::new(x + size, y),
                Point::new(x + size, y + size),
                Point::new(x, y + size),
            ],
            true,
        )]
    }

    #[test]
    fn booleans_combine_areas() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 0.0, 10.0);
        let area = |op| region_of(&combine(&[a.clone(), b.clone()], op).unwrap()).unsigned_area();
        assert!((area(Boolean::Union) - 150.0).abs() < 1e-6);
        assert!((area(Boolean::Difference) - 50.0).abs() < 1e-6);
        assert!((area(Boolean::Intersection) - 50.0).abs() < 1e-6);
        assert!((area(Boolean::Exclusion) - 100.0).abs() < 1e-6);
        let far = square(50.0, 0.0, 5.0);
        assert!(combine(&[a.clone(), far], Boolean::Intersection).is_none());
        // A hole punched through stays a hole.
        let hole = combine(&[a, square(4.0, 4.0, 2.0)], Boolean::Difference).unwrap();
        assert_eq!(hole.len(), 2);
    }

    #[test]
    fn offsets_grow_squares_and_surround_lines() {
        let square = Path::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ],
            true,
        );
        let grown = offset_paths(std::slice::from_ref(&square), 1.0);
        let area = region_of(&grown).unsigned_area();
        // 12 × 12 minus the four rounded corners.
        assert!(
            (area - (100.0 + 40.0 + std::f64::consts::PI)).abs() < 0.2,
            "{area}"
        );
        let shrunk = offset_paths(std::slice::from_ref(&square), -2.0);
        assert!((region_of(&shrunk).unsigned_area() - 36.0).abs() < 1e-6);
        let line = Path::new(vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false);
        let around = offset_paths(&[line], 1.0);
        assert_eq!(around.len(), 1);
    }
}
