//! Projects for a router with a laser head (HiMill with its laser
//! module and similar): the spindle and the laser work on the same
//! board, one head per step, the board never moving in between.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{Builder, settings};
use mml_core::Document;
use mml_core::geometry::{Path, Point};
use mml_core::layer::{FillParams, Operation, ProfileSide};

/// A darker fill than the default, for patterns that need contrast.
pub(crate) fn laser_fill_dark() -> Operation {
    Operation::LaserFill(FillParams {
        speed: 2000.0,
        power: 60.0,
        interval: 0.1,
        ..FillParams::default()
    })
}

/// Lines of a vernier pair along +X from `origin`, turned by
/// `degrees`: the milled main scale (one line per mm from -20 to 20,
/// below the base line) and the laser vernier (11 lines 0.9 mm apart
/// from 0, above it), with their numbers.
fn vernier(origin: Point, degrees: f64) -> (Vec<Path>, Vec<Path>) {
    let turn = mml_core::geometry::Transform::rotate_degrees(degrees)
        .then(mml_core::geometry::Transform::translate(origin.x, origin.y));
    let mut main = Vec::new();
    for mm in -20i32..=20 {
        let x = mm as f64;
        let length = match mm {
            0 => 8.0,
            _ if mm % 5 == 0 => 6.0,
            _ => 3.5,
        };
        main.push(line(point(x, -length), point(x, 0.0)));
        if mm % 10 == 0 {
            main.extend(upright_label(
                &mm.to_string(),
                3.0,
                point(x, -11.0),
                degrees,
            ));
        }
    }
    let mut vernier = Vec::new();
    for k in 0..=10 {
        let x = k as f64 * 0.9;
        let length = if k % 5 == 0 { 6.0 } else { 4.0 };
        vernier.push(line(point(x, 0.0), point(x, length)));
        if k % 5 == 0 {
            vernier.extend(upright_label(&k.to_string(), 2.5, point(x, 9.0), degrees));
        }
    }
    (transform(&main, &turn), transform(&vernier, &turn))
}

/// A label at `centre` in the scale's own frame that still reads
/// upright once the scale is turned by `degrees`.
fn upright_label(value: &str, size: f64, centre: Point, degrees: f64) -> Vec<Path> {
    let back = mml_core::geometry::Transform::rotate_degrees(-degrees);
    let label = stroke_text(value, size, point(0.0, 0.0));
    translate(&transform(&label, &back), centre.x, centre.y)
}

/// Alignment test between the spindle and the laser: milled scales and
/// laser verniers read the laser offset to 0.1 mm in X and Y, crosses
/// read it roughly when it is still unknown.
pub fn hybrid_alignment(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "hybrid_alignment");
    b.stock(140.0, 100.0);
    let milled = b.layer(b.t("layer.milled"), settings::v_line(0.3));
    let burnt = b.layer(b.t("layer.burnt"), settings::laser_line());
    let (main_x, vernier_x) = vernier(point(70.0, 45.0), 0.0);
    let (main_y, vernier_y) = vernier(point(122.0, 62.0), 90.0);
    // Rough targets: a milled cross and circle, the laser draws a
    // smaller one on the same centre.
    let target = point(38.0, 88.0);
    let cross = |arm: f64| {
        vec![
            line(target - point(arm, 0.0), target + point(arm, 0.0)),
            line(target - point(0.0, arm), target + point(0.0, arm)),
        ]
    };
    let mut milled_target = cross(12.0);
    milled_target.push(circle(target, 6.0));
    let mut burnt_target = cross(8.0);
    burnt_target.push(circle(target, 3.0));
    let mut names = stroke_text("X", 5.0, point(70.0, 25.0));
    names.extend(stroke_text("Y", 5.0, point(100.0, 62.0)));
    b.add(milled, b.t("part.scale_x"), main_x);
    b.add(milled, b.t("part.scale_y"), main_y);
    b.add(milled, b.t("part.target"), milled_target);
    b.add(milled, b.t("part.names"), names);
    b.add(burnt, b.t("part.vernier_x"), vernier_x);
    b.add(burnt, b.t("part.vernier_y"), vernier_y);
    b.add(burnt, b.t("part.target"), burnt_target);
    b.step("s1", Vec::new());
    b.step("s2", vec![milled]);
    b.step("s3", vec![burnt]);
    b.step("s4", Vec::new());
    b.finish_hybrid()
}

/// Zigzag band of filled triangles between `x0` and `x1`, from `y` up
/// by `height`.
fn zigzag(x0: f64, x1: f64, y: f64, height: f64) -> Vec<Path> {
    let count = ((x1 - x0) / height).floor().max(1.0) as usize;
    let width = (x1 - x0) / count as f64;
    (0..count)
        .map(|index| {
            let left = x0 + index as f64 * width;
            polygon(&[
                (left, y),
                (left + width, y),
                (left + width / 2.0, y + height),
            ])
        })
        .collect()
}

/// Awale (oware) board in 18 mm wood: the laser decorates the top, then
/// a ball end mill digs the twelve cups and the two stores as rounded
/// bowls (the seeds come out with a fingertip) and a flat one cuts it
/// out.
pub fn awale(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "awale");
    b.stock(260.0, 110.0);
    let thickness = 18.0;
    let rings = b.layer(b.t("layer.rings"), settings::laser_line());
    let bands = b.layer(b.t("layer.bands"), laser_fill_dark());
    let cups = b.layer(b.t("layer.cups"), settings::bowls(9.0, 14.0));
    let outline = b.layer(
        b.t("layer.outline"),
        settings::profile(6.0, thickness + 0.3, ProfileSide::Outside, 4),
    );
    let (pitch, radius) = (31.0, 14.0);
    let mut cup_paths = Vec::new();
    let mut ring_paths = Vec::new();
    for row_y in [48.0, 82.0] {
        for index in 0..6 {
            let centre = point(140.0 + (index as f64 - 2.5) * pitch, row_y);
            cup_paths.push(circle(centre, radius));
            ring_paths.push(circle(centre, radius + 1.2));
        }
    }
    let stores = [
        rounded_rect(22.0, 27.0, 22.0, 76.0, 11.0),
        rounded_rect(236.0, 27.0, 22.0, 76.0, 11.0),
    ];
    for store in &stores {
        ring_paths.extend(offset_outline(store, 1.2));
    }
    let mut band_paths = zigzag(52.0, 228.0, 21.0, 7.0);
    band_paths.extend(zigzag(52.0, 228.0, 102.0, 7.0));
    b.add(rings, b.t("layer.rings"), ring_paths);
    b.add(bands, b.t("layer.bands"), band_paths);
    b.add(cups, b.t("part.cups"), cup_paths);
    b.add(cups, b.t("part.stores"), stores.to_vec());
    b.add(
        outline,
        b.t("part.board"),
        vec![rounded_rect(15.0, 15.0, 250.0, 100.0, 12.0)],
    );
    b.step("s1", Vec::new());
    b.step("s2", vec![rings, bands]);
    b.step("s3", vec![cups]);
    b.step("s4", vec![outline]);
    b.step("s5", Vec::new());
    b.step("s6", Vec::new());
    b.finish_hybrid()
}

/// `path` grown by `distance` (its largest ring).
fn offset_outline(path: &Path, distance: f64) -> Vec<Path> {
    mml_core::region::offset_paths(std::slice::from_ref(path), distance)
}

/// Compass rose: filled half of each point, outlines of all of them.
fn compass_rose(centre: Point, radius: f64) -> (Vec<Path>, Vec<Path>) {
    let mut fill = Vec::new();
    let mut lines = Vec::new();
    for index in 0..8 {
        let angle = index as f64 * 45.0;
        let main = index % 2 == 0;
        let (length, base) = if main {
            (radius, radius * 0.25)
        } else {
            (radius * 0.68, radius * 0.19)
        };
        let tip = polar(centre, length, angle);
        let left = polar(centre, base, angle + 45.0);
        let right = polar(centre, base, angle - 45.0);
        // Main points dark on the left, the others on the right.
        let dark = if main { left } else { right };
        fill.push(Path::new(vec![centre, tip, dark], true));
        lines.push(Path::new(vec![tip, left, centre, right], true));
    }
    let fill = union(fill.into_iter().map(|path| vec![path]).collect());
    lines.push(circle(centre, radius + 3.0));
    lines.push(circle(centre, radius + 4.5));
    (fill, lines)
}

/// Valet tray in 12 mm wood: the spindle digs the tray and cuts it out
/// (tabs keep it in place), then the laser burns a compass rose on the
/// floor of the pocket, focused 6 mm lower.
pub fn valet_tray(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "valet_tray");
    b.stock(200.0, 140.0);
    let thickness = 12.0;
    let pocket = b.layer(b.t("layer.pocket"), settings::pocket(6.0, 6.0));
    let outline = b.layer(
        b.t("layer.outline"),
        settings::profile(6.0, thickness + 0.3, ProfileSide::Outside, 4),
    );
    let rose_fill = b.layer(b.t("layer.rose_fill"), laser_fill_dark());
    let rose_lines = b.layer(b.t("layer.rose_lines"), settings::laser_line());
    let centre = point(110.0, 80.0);
    let (fill, mut lines) = compass_rose(centre, 31.0);
    for (letter, angle) in [("N", 90.0), ("E", 0.0), ("S", 270.0), ("W", 180.0)] {
        lines.extend(stroke_text(letter, 5.0, polar(centre, 40.0, angle)));
    }
    b.add(
        pocket,
        b.t("part.pocket"),
        vec![rounded_rect(35.0, 35.0, 150.0, 90.0, 10.0)],
    );
    b.add(
        outline,
        b.t("part.tray"),
        vec![rounded_rect(20.0, 20.0, 180.0, 120.0, 14.0)],
    );
    b.add(rose_fill, b.t("layer.rose_fill"), fill);
    b.add(rose_lines, b.t("layer.rose_lines"), lines);
    b.step("s1", Vec::new());
    b.step("s2", vec![pocket, outline]);
    b.step("s3", vec![rose_fill, rose_lines]);
    b.step("s4", Vec::new());
    b.finish_hybrid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vernier_lines_are_a_tenth_of_a_millimetre_shorter() {
        let (main, vernier) = vernier(point(0.0, 0.0), 0.0);
        let xs = |paths: &[Path]| -> Vec<f64> {
            paths
                .iter()
                .filter(|path| path.points.len() == 2 && path.points[0].x == path.points[1].x)
                .map(|path| path.points[0].x)
                .collect()
        };
        let (main, vernier) = (xs(&main), xs(&vernier));
        assert_eq!(main.len(), 41);
        assert_eq!(vernier.len(), 11);
        // A laser shift of 0.3 mm lines up the third vernier line with
        // the third millimetre.
        let shifted: Vec<f64> = vernier.iter().map(|x| x + 0.3).collect();
        assert!(main.iter().any(|m| (m - shifted[3]).abs() < 1e-9));
        assert!((vernier[10] - 9.0).abs() < 1e-9);
    }
}
