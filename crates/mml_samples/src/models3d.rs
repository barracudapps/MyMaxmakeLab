//! More slot-together models: a triceratops and a galleon. Like the
//! dragon, a central part seen from the side carries the slots of the
//! parts that cross it.

use crate::puzzles3d::{Crossing, legs, outline_of, smooth_closed, vertical_extent, vertical_slot};
use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::{Path, Point};
use mml_core::region::{clip_lines, offset_paths, region_of};
use mml_core::{Document, MachineKind};

/// A part crossing under the central one at `x` (legs, a cradle): its
/// top rises `rise` into the central part. Returns the central part's
/// slot and the crossing.
fn from_below(profile: &Path, x: f64, rise: f64) -> (Vec<Path>, Crossing) {
    let (bottom, _) = vertical_extent(profile, x).expect("a part under the profile");
    let meet = bottom + rise / 2.0;
    (
        vertical_slot(x, bottom - 5.0, meet),
        Crossing {
            x,
            body_edge: bottom,
            part_edge: bottom + rise,
            meet,
        },
    )
}

/// A part crossing over the central one at `x` (a frill, a sail): its
/// bottom goes `depth` into the central part.
fn from_above(profile: &Path, x: f64, depth: f64) -> (Vec<Path>, Crossing) {
    let (_, top) = vertical_extent(profile, x).expect("a part over the profile");
    let meet = top - depth / 2.0;
    (
        vertical_slot(x, meet, top + 5.0),
        Crossing {
            x,
            body_edge: top,
            part_edge: top - depth,
            meet,
        },
    )
}

/// The slot a crossing part needs, in its own frame (centre at 0,
/// same heights as the central part).
fn part_slot(crossing: &Crossing) -> Vec<Path> {
    let beyond = if crossing.part_edge > crossing.meet {
        crossing.part_edge + 5.0
    } else {
        crossing.part_edge - 5.0
    };
    vertical_slot(0.0, crossing.meet, beyond)
}

/// A slot-together model: the central part, the crossing parts and
/// where they cross.
pub struct Model {
    pub central: Part,
    pub crossing: Vec<Part>,
    #[allow(dead_code)]
    pub crossings: Vec<Crossing>,
}

/// Whether `p` lies inside `area` (even-odd over all its paths).
fn inside(area: &[Path], p: Point) -> bool {
    let mut inside = false;
    for path in area {
        let count = path.points.len();
        for index in 0..count {
            let (a, b) = (path.points[index], path.points[(index + 1) % count]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (b.x - a.x) * (p.y - a.y) / (b.y - a.y) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Dots of `radius` on a staggered grid, wholly inside `area`.
fn dots(area: &[Path], pitch: f64, radius: f64) -> Vec<Path> {
    let bounds = mml_core::geometry::bounds_of(area);
    let shrunk = offset_paths(area, -radius);
    let mut result = Vec::new();
    let mut row = 0;
    let mut y = bounds.min.y;
    while y < bounds.max.y {
        let mut x = bounds.min.x + if row % 2 == 0 { 0.0 } else { pitch / 2.0 };
        while x < bounds.max.x {
            let centre = point(x, y);
            if inside(&shrunk, centre) {
                result.push(circle(centre, radius));
            }
            x += pitch;
        }
        y += pitch * 0.87;
        row += 1;
    }
    result
}

pub fn triceratops_parts(names: [String; 4]) -> Model {
    let [body_name, frill_name, front_name, rear_name] = names;
    let profile = smooth_closed(
        &[
            (5.0, 60.0),
            (25.0, 59.0),
            (50.0, 64.0),
            (75.0, 73.0),
            (100.0, 79.0),
            (125.0, 77.0),
            (150.0, 71.0),
            (166.0, 67.0),
            (182.0, 72.0),
            (196.0, 77.0),
            (207.0, 75.0),
            (216.0, 70.0),
            (226.0, 66.0),
            (234.0, 60.0),
            (227.0, 55.0),
            (214.0, 52.0),
            (198.0, 50.0),
            (180.0, 47.0),
            (165.0, 43.0),
            (150.0, 38.0),
            (122.0, 35.0),
            (95.0, 36.0),
            (75.0, 40.0),
            (60.0, 46.0),
            (40.0, 51.0),
            (20.0, 55.0),
            (8.0, 58.0),
        ],
        8,
    );
    let mut shapes = vec![vec![profile.clone()]];
    // Two long brow horns and a short nose horn.
    for (base, width, tip) in [(199.0, 8.0, (226.0, 100.0)), (217.0, 6.0, (228.0, 82.0))] {
        if let Some((_, top)) = vertical_extent(&profile, base) {
            shapes.push(vec![polygon(&[
                (base - width / 2.0, top - 3.0),
                (base + width / 2.0, top - 3.0),
                tip,
            ])]);
        }
    }
    let mut cuts = Vec::new();
    let mut crossings = Vec::new();
    let mut crossing_parts = Vec::new();
    for (x, half, name) in [(148.0, 36.0, front_name), (85.0, 44.0, rear_name)] {
        let (slot, crossing) = from_below(&profile, x, 16.0);
        cuts.push(slot);
        let area = difference(vec![
            vec![legs(half, crossing.body_edge - 5.0, crossing.part_edge)],
            part_slot(&crossing),
        ]);
        crossing_parts.push(Part::new(name, outline_of(area)));
        crossings.push(crossing);
    }
    // Frill: a scalloped fan standing across the neck.
    let (slot, crossing) = from_above(&profile, 172.0, 16.0);
    cuts.push(slot);
    let base = crossing.part_edge;
    let hub = point(0.0, base + 12.0);
    let mut fan = vec![(-14.0, base), (14.0, base)];
    let bumps = 11;
    for step in 0..=bumps * 4 {
        let angle = -15.0 + 210.0 * step as f64 / (bumps * 4) as f64;
        let radius = 46.0 + 3.0 * ((step % 4) as f64 * std::f64::consts::FRAC_PI_2).cos();
        let p = polar(hub, radius, angle);
        fan.push((p.x, p.y));
    }
    let frill_area = difference(vec![vec![polygon(&fan)], part_slot(&crossing)]);
    let frill_outline = outline_of(frill_area);
    let mut frill_lines: Vec<Path> = (0..9)
        .map(|index| {
            let angle = 10.0 + index as f64 * 20.0;
            line(polar(hub, 14.0, angle), polar(hub, 40.0, angle))
        })
        .collect();
    for side in [-1.0, 1.0] {
        let spot = point(side * 24.0, hub.y + 20.0);
        frill_lines.push(circle(spot, 6.0));
        frill_lines.push(circle(spot, 3.0));
    }
    let frill_lines = clip_lines(
        &frill_lines,
        &region_of(&offset_paths(std::slice::from_ref(&frill_outline), -2.0)),
    );
    crossing_parts.push(Part::new(frill_name, frill_outline).with_engraving(frill_lines));
    crossings.push(crossing);
    let mut body_shapes = vec![union(shapes)];
    body_shapes.extend(cuts);
    let body_area = difference(body_shapes);
    // Pebbly skin on the body, the eye, the mouth.
    let skin_area = offset_paths(
        &difference(vec![
            vec![profile.clone()],
            vec![rect(160.0, 0.0, 100.0, 200.0)],
        ]),
        -5.0,
    );
    let mut engraving = dots(&skin_area, 7.0, 1.6);
    let eye = point(203.0, 66.0);
    engraving.push(circle(eye, 4.0));
    engraving.push(line(point(231.0, 58.0), point(206.0, 55.0)));
    let central = Part::from_area(body_name, body_area)
        .with_holes(vec![circle(eye, 1.8)])
        .with_engraving(engraving);
    Model {
        central,
        crossing: crossing_parts,
        crossings,
    }
}

/// A triceratops to slot together in 3 mm plywood.
pub fn triceratops(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "triceratops");
    b.stock(BOARD.0, BOARD.1);
    let model = triceratops_parts([
        b.t("part.body"),
        b.t("part.frill"),
        b.t("part.front_legs"),
        b.t("part.rear_legs"),
    ]);
    let mut parts = vec![model.central];
    parts.extend(model.crossing);
    cut_and_finish(b, parts)
}

/// Laser boards for a slot-together model, then the assembly steps.
fn cut_and_finish(mut b: Builder, parts: Vec<Part>) -> Document {
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 4.0),
        BoardOps {
            engrave: Some(settings::laser_line()),
            engrave_alone: false,
            holes: settings::laser_cut(),
            outline: settings::laser_cut(),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.step("s3", Vec::new());
    b.finish(MachineKind::Laser)
}

/// A square sail `half` wide from `bottom` to `top`, its sides bulging.
fn sail(half: f64, bottom: f64, top: f64) -> Path {
    let mut points = vec![(-half - 4.0, top), (half + 4.0, top)];
    let steps = 8;
    for step in 1..steps {
        let t = step as f64 / steps as f64;
        let y = top + (bottom - top) * t;
        points.push((half + 3.0 * (t * std::f64::consts::PI).sin(), y));
    }
    points.push((half - 2.0, bottom));
    points.push((-half + 2.0, bottom));
    for step in (1..steps).rev() {
        let t = step as f64 / steps as f64;
        let y = top + (bottom - top) * t;
        points.push((-half - 3.0 * (t * std::f64::consts::PI).sin(), y));
    }
    polygon(&points)
}

/// Mast width: the sail slots leave 3 mm of wood either side.
const MAST: f64 = 9.0;

pub fn galleon_parts(names: [String; 3]) -> Model {
    let [hull_name, sail_name, cradle_name] = names;
    let hull = polygon(&[
        (18.0, 22.0),
        (30.0, 20.0),
        (175.0, 20.0),
        (195.0, 23.0),
        (210.0, 30.0),
        (221.0, 41.0),
        (228.0, 52.0),
        (231.0, 62.0),
        (196.0, 62.0),
        (193.0, 55.0),
        (62.0, 55.0),
        (58.0, 66.0),
        (14.0, 70.0),
        (8.0, 62.0),
        (9.0, 45.0),
        (12.0, 32.0),
    ]);
    // Masts 9 mm wide, so the sail slots leave 3 mm either side; the
    // bowsprit points up from the bow.
    let masts = [(96.0, 170.0), (152.0, 160.0)];
    let mut shapes = vec![vec![hull.clone()]];
    for (x, top) in masts {
        shapes.push(vec![rect(x - MAST / 2.0, 50.0, MAST, top - 50.0)]);
    }
    shapes.push(vec![polygon(&[
        (222.0, 56.0),
        (248.0, 74.0),
        (247.0, 77.0),
        (219.0, 62.0),
    ])]);
    // A pennant flying from a staff at the stern.
    shapes.push(vec![rect(20.0, 60.0, 4.0, 40.0)]);
    shapes.push(vec![polygon(&[(23.0, 100.0), (50.0, 94.0), (23.0, 88.0)])]);
    let ship = union(shapes);
    let ship_outline = outline_of(ship.clone());
    let mut cuts = Vec::new();
    let mut crossings = Vec::new();
    let mut crossing_parts = Vec::new();
    for (index, (x, top)) in masts.iter().enumerate() {
        let (slot, crossing) = from_above(&ship_outline, *x, top - 95.0 - index as f64 * 5.0);
        cuts.push(slot);
        let half = if index == 0 { 38.0 } else { 33.0 };
        let area = difference(vec![
            vec![sail(half, crossing.part_edge, top + 2.0)],
            part_slot(&crossing),
        ]);
        let outline = outline_of(area);
        // A cross on the sail, and the yard along its top.
        let middle = (crossing.part_edge + top) / 2.0 + 4.0;
        let mut marks = vec![
            line(point(-half, top - 4.0), point(half, top - 4.0)),
            circle(point(0.0, middle), half * 0.45),
        ];
        let arm = half * 0.3;
        marks.push(line(point(-arm, middle), point(arm, middle)));
        marks.push(line(point(0.0, middle - arm), point(0.0, middle + arm)));
        let marks = clip_lines(
            &marks,
            &region_of(&offset_paths(std::slice::from_ref(&outline), -1.5)),
        );
        crossing_parts
            .push(Part::new(format!("{sail_name} {}", index + 1), outline).with_engraving(marks));
        crossings.push(crossing);
    }
    for (index, x) in [60.0, 172.0].into_iter().enumerate() {
        let (slot, crossing) = from_below(&ship_outline, x, 14.0);
        cuts.push(slot);
        let top = crossing.part_edge;
        let cradle = difference(vec![
            vec![polygon(&[
                (-46.0, 0.0),
                (46.0, 0.0),
                (30.0, top),
                (-30.0, top),
            ])],
            vec![rounded_rect(-26.0, -5.0, 52.0, 15.0, 7.0)],
            part_slot(&crossing),
        ]);
        crossing_parts.push(Part::new(
            format!("{cradle_name} {}", index + 1),
            outline_of(cradle),
        ));
        crossings.push(crossing);
    }
    let mut central_shapes = vec![ship];
    central_shapes.extend(cuts);
    let central_area = difference(central_shapes);
    // Planks along the hull, gun ports, the stern windows.
    let hull_inside = offset_paths(std::slice::from_ref(&hull), -2.5);
    let mut planks = Vec::new();
    for y in [27.0, 34.0, 41.0, 48.0] {
        planks.push(line(point(0.0, y), point(240.0, y)));
    }
    let mut engraving = clip_lines(&planks, &region_of(&hull_inside));
    for index in 0..7 {
        let x = 70.0 + index as f64 * 18.0;
        engraving.push(rect(x, 43.0, 6.0, 5.0));
    }
    for index in 0..3 {
        engraving.push(rect(16.0 + index as f64 * 11.0, 58.0, 6.0, 6.0));
    }
    let central = Part::from_area(hull_name, central_area).with_engraving(engraving);
    Model {
        central,
        crossing: crossing_parts,
        crossings,
    }
}

/// A galleon to slot together: hull and masts in one part, two square
/// sails and two cradles.
pub fn galleon(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "galleon");
    b.stock(BOARD.0, BOARD.1);
    let model = galleon_parts([b.t("part.hull"), b.t("part.sail"), b.t("part.cradle")]);
    let mut parts = vec![model.central];
    parts.extend(model.crossing);
    cut_and_finish(b, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzles3d::THICKNESS;

    fn check(model: &Model, expected: usize) {
        assert_eq!(model.crossings.len(), expected);
        for crossing in &model.crossings {
            let middle = (crossing.body_edge + crossing.part_edge) / 2.0;
            assert!((crossing.meet - middle).abs() < 1e-9);
            let (low, high) = vertical_extent(&model.central.outline, crossing.x).unwrap();
            let edge = if crossing.part_edge > crossing.body_edge {
                low
            } else {
                high
            };
            assert!(
                (edge - crossing.meet).abs() < 0.5,
                "{crossing:?} {low} {high}"
            );
        }
        let size = model.central.bounds();
        assert!(
            size.width() <= BOARD.0 - 10.0 && size.height() <= BOARD.1 - 10.0,
            "{size:?}"
        );
    }

    #[test]
    fn triceratops_slots_meet_halfway() {
        let model = triceratops_parts(["b".into(), "f".into(), "l".into(), "r".into()]);
        check(&model, 3);
        // Four feet on the ground, the body above them.
        for legs in &model.crossing[..2] {
            assert!(legs.bounds().min.y.abs() < 0.01);
        }
        assert!(model.central.bounds().min.y > 30.0);
    }

    #[test]
    fn galleon_slots_meet_halfway_and_masts_keep_their_sides() {
        let model = galleon_parts(["h".into(), "s".into(), "c".into()]);
        check(&model, 4);
        // 3 mm of mast either side of each sail slot.
        const { assert!((MAST - THICKNESS) / 2.0 >= 3.0) };
    }
}
