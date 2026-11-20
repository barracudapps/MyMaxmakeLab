//! How big a cutter the geometry allows. A tool of diameter d follows a
//! shape exactly when shrinking it by d/2 then growing it back loses
//! nothing (a morphological opening): corners rounder than d/2 and gaps
//! narrower than d are what a bigger tool would miss.

use geo::{Area, BooleanOps, MultiPolygon, Rect, coord};
use mml_core::geometry::Path;
use mml_core::region::{offset, region_of};

/// Where the tool works relative to the shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Inside the shapes: pockets, inside profiles.
    Inside,
    /// Around them: outside profiles, between parts.
    Outside,
}

fn union(shapes: &[Vec<Path>]) -> MultiPolygon<f64> {
    shapes.iter().map(|paths| region_of(paths)).fold(
        MultiPolygon::new(Vec::new()),
        |all, region| {
            if all.0.is_empty() {
                region
            } else {
                all.union(&region)
            }
        },
    )
}

fn working_area(shapes: &[Vec<Path>], side: Side) -> MultiPolygon<f64> {
    let region = union(shapes);
    match side {
        Side::Inside => region,
        Side::Outside => {
            // The free space around the parts, with a margin so the tool
            // always fits along the outer edges.
            let Some(bounds) = geo::BoundingRect::bounding_rect(&region) else {
                return MultiPolygon::new(Vec::new());
            };
            let margin = 20.0;
            let frame = Rect::new(
                coord! { x: bounds.min().x - margin, y: bounds.min().y - margin },
                coord! { x: bounds.max().x + margin, y: bounds.max().y + margin },
            )
            .to_polygon();
            MultiPolygon::new(vec![frame]).difference(&region)
        }
    }
}

/// Largest tool diameter that follows the shapes while losing at most
/// `tolerance` (share of the area, e.g. 0.01).
pub fn largest_tool(shapes: &[Vec<Path>], side: Side, tolerance: f64) -> Option<f64> {
    let area = working_area(shapes, side);
    let total = area.unsigned_area();
    if total <= 0.0 {
        return None;
    }
    let keeps = |diameter: f64| {
        let opened = offset(&offset(&area, -diameter / 2.0), diameter / 2.0);
        (total - opened.unsigned_area()) / total <= tolerance
    };
    let (mut low, mut high) = (0.05, 25.0);
    if !keeps(low) {
        return None;
    }
    if keeps(high) {
        return Some(high);
    }
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if keeps(middle) {
            low = middle
        } else {
            high = middle
        }
    }
    Some(low)
}

/// Widest stroke of the shapes: the diameter of the largest circle that
/// fits inside, what a V-bit must cover to carve the full width.
pub fn widest_stroke(shapes: &[Vec<Path>]) -> Option<f64> {
    let region = union(shapes);
    if region.unsigned_area() <= 0.0 {
        return None;
    }
    let (mut low, mut high) = (0.0, 200.0);
    for _ in 0..30 {
        let middle = (low + high) / 2.0;
        if offset(&region, -middle / 2.0).unsigned_area() > 1e-9 {
            low = middle
        } else {
            high = middle
        }
    }
    Some(low)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::geometry::Point;

    fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Path {
        Path::new(
            vec![
                Point::new(x, y),
                Point::new(x + width, y),
                Point::new(x + width, y + height),
                Point::new(x, y + height),
            ],
            true,
        )
    }

    #[test]
    fn a_narrow_slot_limits_the_cutter() {
        // A 40 x 30 pocket with a 3 mm wide slot coming out of it.
        let pocket = vec![
            vec![rectangle(0.0, 0.0, 40.0, 30.0)],
            vec![rectangle(40.0, 13.5, 20.0, 3.0)],
        ];
        let diameter = largest_tool(&pocket, Side::Inside, 0.002).unwrap();
        assert!(diameter > 2.0 && diameter <= 3.05, "{diameter}");
        // Without the slot, square corners are the only limit: a bigger
        // tool keeps 99 % of the area.
        let plain = vec![vec![rectangle(0.0, 0.0, 40.0, 30.0)]];
        assert!(largest_tool(&plain, Side::Inside, 0.01).unwrap() > 6.0);
    }

    #[test]
    fn parts_close_together_limit_an_outside_cut() {
        let parts = vec![
            vec![rectangle(0.0, 0.0, 20.0, 20.0)],
            vec![rectangle(22.0, 0.0, 20.0, 20.0)],
        ];
        let diameter = largest_tool(&parts, Side::Outside, 0.002).unwrap();
        assert!(diameter <= 2.05, "{diameter}");
        let stroke = widest_stroke(&[vec![rectangle(0.0, 0.0, 50.0, 4.0)]]).unwrap();
        assert!((stroke - 4.0).abs() < 0.05, "{stroke}");
    }
}
