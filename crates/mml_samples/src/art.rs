//! Silhouettes drawn by others and placed in the public domain (CC0 on
//! openclipart.org, see art/CREDITS.md), used where a hand-drawn shape
//! looks much better than one built from curves.

use crate::shapes::*;
use mml_core::geometry::{Path, Transform, bounds_of, simplify_polyline};

pub const MOOSE: &[u8] = include_bytes!("../art/moose.svg");
pub const STAG: &[u8] = include_bytes!("../art/stag.svg");
pub const LEAPING_DEER: &[u8] = include_bytes!("../art/leaping-deer.svg");
/// A sheet of forest animals; `Pick` areas below take one out.
pub const FOREST_ANIMALS: &[u8] = include_bytes!("../art/forest-animals.svg");

/// Sheets of other animals, and a whale with a star, a heart and a moon.
pub const ANIMALS_A: &[u8] = include_bytes!("../art/animals-a.svg");
pub const ANIMALS_B: &[u8] = include_bytes!("../art/animals-b.svg");
pub const WHALE_SHAPES: &[u8] = include_bytes!("../art/whale-shapes.svg");
pub const BAT: &[u8] = include_bytes!("../art/bat.svg");

/// Where one drawing lies on a sheet of several: left, top, right,
/// bottom as fractions of the sheet, from its top left corner.
pub type Pick = (f64, f64, f64, f64);

pub const BEAR: Pick = (0.0, 0.0, 0.51, 0.27);
pub const WOLF: Pick = (0.52, 0.5, 1.0, 0.79);
pub const OWL: Pick = (0.39, 0.63, 0.63, 1.0);
/// On `ANIMALS_A`.
pub const PENGUIN: Pick = (0.32, 0.0, 0.58, 0.37);
/// On `ANIMALS_B`.
pub const RHINO: Pick = (0.3, 0.0, 0.6, 0.17);
pub const TURTLE: Pick = (0.38, 0.84, 0.67, 1.0);
/// On `WHALE_SHAPES`.
pub const WHALE: Pick = (0.0, 0.0, 0.5, 0.5);

/// The filled area of a silhouette, scaled to `height` mm with its
/// lower left corner at the origin, turned to face the other way if
/// `mirror`. Points closer than 0.05 mm to the line are dropped.
pub fn silhouette(svg: &[u8], height: f64, mirror: bool) -> Vec<Path> {
    picked(svg, (0.0, 0.0, 1.0, 1.0), height, mirror)
}

/// `silhouette` of the drawing of a sheet lying in `pick`: the pieces
/// whose middle is in that area.
pub fn picked(svg: &[u8], pick: Pick, height: f64, mirror: bool) -> Vec<Path> {
    let groups = mml_io::import_svg(svg).expect("a bundled drawing");
    let all: Vec<Path> = groups
        .into_iter()
        .flat_map(|group| group.paths)
        .filter(|path| path.points.len() > 2)
        // Filled shapes, even when the drawing left them open.
        .map(|path| Path::new(path.points, true))
        .collect();
    let sheet = bounds_of(&all);
    let (left, top, right, bottom) = pick;
    let x = |fraction: f64| sheet.min.x + sheet.width() * fraction;
    let y = |fraction: f64| sheet.max.y - sheet.height() * fraction;
    let within = |path: &Path| {
        let piece = path.bounds();
        let (cx, cy) = (
            (piece.min.x + piece.max.x) / 2.0,
            (piece.min.y + piece.max.y) / 2.0,
        );
        cx >= x(left) && cx <= x(right) && cy <= y(top) && cy >= y(bottom)
    };
    // The pieces of the drawing overlap (some drawings are made of
    // thousands, some draw a shape twice): their union is the shape.
    let shapes: Vec<Vec<Path>> = all
        .into_iter()
        .filter(|path| within(path))
        .map(|path| vec![path])
        .collect();
    let area = union(shapes);
    // Specks left around the drawing would only be cut as dust.
    let largest = area
        .iter()
        .map(|path| path.signed_area().abs())
        .fold(0.0, f64::max);
    let area: Vec<Path> = area
        .into_iter()
        .filter(|path| path.signed_area().abs() > largest * 0.02)
        .collect();
    let bounds = bounds_of(&area);
    let scale = height / bounds.height();
    let mut place =
        Transform::translate(-bounds.min.x, -bounds.min.y).then(Transform::scale(scale, scale));
    if mirror {
        place = place
            .then(Transform::scale(-1.0, 1.0))
            .then(Transform::translate(bounds.width() * scale, 0.0));
    }
    area.iter()
        .map(|path| {
            let path = path.transformed(&place);
            let mut points = path.points.clone();
            points.push(points[0]);
            let mut points = simplify_polyline(&points, 0.05);
            points.pop();
            Path::new(points, true)
        })
        .filter(|path| path.points.len() > 2)
        .collect()
}

/// `paths` moved so their lower left corner is at (`x`, `y`).
pub fn placed(paths: &[Path], x: f64, y: f64) -> Vec<Path> {
    translate(paths, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silhouettes_load_at_their_size() {
        for svg in [MOOSE, STAG, LEAPING_DEER] {
            let area = silhouette(svg, 100.0, false);
            let bounds = bounds_of(&area);
            assert!((bounds.height() - 100.0).abs() < 0.1, "{bounds:?}");
            assert!(bounds.min.x.abs() < 0.1 && bounds.min.y.abs() < 0.1);
            let mirrored = bounds_of(&silhouette(svg, 100.0, true));
            assert!((mirrored.width() - bounds.width()).abs() < 0.1);
            assert!(mirrored.min.x.abs() < 0.1);
        }
        for pick in [BEAR, WOLF, OWL] {
            let area = picked(FOREST_ANIMALS, pick, 100.0, false);
            assert!(!area.is_empty());
            assert!((bounds_of(&area).height() - 100.0).abs() < 0.1);
        }
    }
}
