//! 3D puzzles: flat parts in 3 mm plywood that slot together crosswise.
//! Two crossing parts each have a slot along the line where they meet,
//! from opposite ends to the middle of that line, so each part slides
//! into the other's slot.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::{Path, Point, Transform};
use mml_core::region::{clip_lines, offset_paths, region_of};
use mml_core::{Document, MachineKind};

/// Plywood thickness, and width of every slot.
pub const THICKNESS: f64 = 3.0;

/// Smooth closed curve through `points` (Catmull-Rom).
pub fn smooth_closed(points: &[(f64, f64)], steps: usize) -> Path {
    let count = points.len();
    let at = |index: usize| {
        let (x, y) = points[index % count];
        point(x, y)
    };
    let mut result = Vec::new();
    for index in 0..count {
        let (p0, p1, p2, p3) = (
            at(index + count - 1),
            at(index),
            at(index + 1),
            at(index + 2),
        );
        for step in 0..steps {
            let t = step as f64 / steps as f64;
            let (t2, t3) = (t * t, t * t * t);
            let blend = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b
                    + (c - a) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            result.push(point(
                blend(p0.x, p1.x, p2.x, p3.x),
                blend(p0.y, p1.y, p2.y, p3.y),
            ));
        }
    }
    Path::new(result, true)
}

/// Lowest and highest points of a closed path on the vertical line at
/// `x`.
pub fn vertical_extent(path: &Path, x: f64) -> Option<(f64, f64)> {
    let count = path.points.len();
    let mut found: Option<(f64, f64)> = None;
    for index in 0..count {
        let (a, b) = (path.points[index], path.points[(index + 1) % count]);
        if (a.x - x) * (b.x - x) > 0.0 || (a.x - b.x).abs() < 1e-12 {
            continue;
        }
        let y = a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x);
        found = Some(match found {
            Some((low, high)) => (low.min(y), high.max(y)),
            None => (y, y),
        });
    }
    found
}

/// Slot `THICKNESS` wide centred on `x`, from `from` to `to` in y.
fn vertical_slot(x: f64, from: f64, to: f64) -> Vec<Path> {
    vec![rect(
        x - THICKNESS / 2.0,
        from.min(to),
        THICKNESS,
        (to - from).abs(),
    )]
}

/// The largest closed path of an area.
fn outline_of(area: Vec<Path>) -> Path {
    area.into_iter()
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .expect("an area has an outline")
}

/// The dragon's parts: body, wings and two pairs of legs, with the
/// heights where they cross so tests can check the slots.
pub struct Dragon {
    pub body: Part,
    pub wings: Part,
    pub front_legs: Part,
    pub rear_legs: Part,
    /// Where each crossing part meets the body; read by the tests.
    #[allow(dead_code)]
    pub crossings: Vec<Crossing>,
}

/// A part crossing the body at `x`: the body's slot runs from its
/// edge to `meet`, the part's slot from its own edge to `meet`.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Crossing {
    pub x: f64,
    /// Body edge the body's slot opens from.
    pub body_edge: f64,
    /// Edge of the crossing part its slot opens from.
    pub part_edge: f64,
    pub meet: f64,
}

/// Leg pair seen from the front, `half` wide, standing on y = 0, its
/// top bar `top` high and the gap between the legs `crotch` high.
fn legs(half: f64, crotch: f64, top: f64) -> Path {
    polygon(&[
        (-half, 0.0),
        (-half + 16.0, 0.0),
        (-11.0, crotch),
        (11.0, crotch),
        (half - 16.0, 0.0),
        (half, 0.0),
        (half - 10.0, top - 10.0),
        (half - 18.0, top),
        (-half + 18.0, top),
        (-half + 10.0, top - 10.0),
    ])
}

pub fn dragon_parts(names: [String; 4]) -> Dragon {
    let [body_name, wings_name, front_name, rear_name] = names;
    // Side view: tail tip on the left, head on the right.
    let profile = smooth_closed(
        &[
            (5.0, 70.0),
            (20.0, 63.0),
            (40.0, 59.0),
            (60.0, 63.0),
            (80.0, 70.0),
            (100.0, 75.0),
            (125.0, 77.0),
            (150.0, 75.0),
            (165.0, 80.0),
            (177.0, 95.0),
            (184.0, 112.0),
            (191.0, 125.0),
            (202.0, 133.0),
            (217.0, 135.0),
            (232.0, 128.0),
            (239.0, 120.0),
            (231.0, 114.0),
            (216.0, 113.0),
            (205.0, 109.0),
            (195.0, 99.0),
            (187.0, 84.0),
            (178.0, 66.0),
            (164.0, 47.0),
            (150.0, 38.0),
            (125.0, 35.0),
            (100.0, 36.0),
            (85.0, 40.0),
            (70.0, 47.0),
            (55.0, 51.0),
            (38.0, 50.0),
            (20.0, 55.0),
            (8.0, 63.0),
        ],
        8,
    );
    let (rear_x, wing_x, front_x): (f64, f64, f64) = (95.0, 125.0, 150.0);
    let mut shapes = vec![vec![profile.clone()]];
    // Spikes along the back, none over the wing slot.
    let mut x: f64 = 26.0;
    while x < 165.0 {
        if (x - wing_x).abs() > 14.0
            && let Some((_, top)) = vertical_extent(&profile, x)
        {
            shapes.push(vec![polygon(&[
                (x - 5.0, top - 2.0),
                (x + 5.0, top - 2.0),
                (x + 1.5, top + 7.0),
            ])]);
        }
        x += 13.0;
    }
    // Two horns swept back from the head.
    for (base, tip) in [(199.0, (186.0, 148.0)), (207.0, (196.0, 151.0))] {
        if let Some((_, top)) = vertical_extent(&profile, base) {
            shapes.push(vec![polygon(&[
                (base - 4.0, top - 3.0),
                (base + 4.0, top - 3.0),
                tip,
            ])]);
        }
    }
    let mut crossings = Vec::new();
    let mut cuts = Vec::new();
    let mut leg_parts = Vec::new();
    for (x, half, name) in [(front_x, 38.0, front_name), (rear_x, 44.0, rear_name)] {
        let (bottom, _) = vertical_extent(&profile, x).expect("legs under the body");
        let top = bottom + 16.0;
        let meet = bottom + 8.0;
        cuts.push(vertical_slot(x, bottom - 5.0, meet));
        crossings.push(Crossing {
            x,
            body_edge: bottom,
            part_edge: top,
            meet,
        });
        let area = difference(vec![
            vec![legs(half, bottom - 5.0, top)],
            vertical_slot(0.0, meet, top + 5.0),
        ]);
        leg_parts.push(Part::new(name, outline_of(area)));
    }
    let (_, back) = vertical_extent(&profile, wing_x).expect("wings on the body");
    let wing_bottom = back - 18.0;
    let meet = back - 9.0;
    cuts.push(vertical_slot(wing_x, meet, back + 5.0));
    crossings.push(Crossing {
        x: wing_x,
        body_edge: back,
        part_edge: wing_bottom,
        meet,
    });
    let mut body_shapes = vec![union(shapes)];
    body_shapes.extend(cuts);
    let body_area = difference(body_shapes);
    // Scales, the eye, a nostril and the mouth.
    let mut scales = Vec::new();
    for row in 0..30 {
        let y = 30.0 + row as f64 * 4.0;
        let shift = if row % 2 == 0 { 0.0 } else { 3.0 };
        let mut x = shift;
        while x < 180.0 {
            let arc: Vec<Point> = (0..=6)
                .map(|step| polar(point(x, y), 3.0, 180.0 + step as f64 * 30.0))
                .collect();
            scales.push(Path::new(arc, false));
            x += 6.0;
        }
    }
    // Scales on the body and tail, not on the neck and head.
    let inside = region_of(&offset_paths(
        &difference(vec![
            vec![profile.clone()],
            vec![rect(176.0, 0.0, 100.0, 200.0)],
        ]),
        -5.0,
    ));
    let mut engraving = clip_lines(&scales, &inside);
    let eye = point(219.0, 126.0);
    engraving.push(circle(eye, 4.5));
    engraving.push(circle(point(234.0, 124.0), 1.2));
    engraving.push(line(point(237.0, 118.0), point(214.0, 116.5)));
    let body = Part::from_area(body_name, body_area)
        .with_holes(vec![circle(eye, 2.0)])
        .with_engraving(engraving);
    // Wings seen from the front: the slot rises from the bottom middle.
    let rise = back + 10.0;
    let right = [
        (15.0, rise + 8.0),
        (45.0, rise + 30.0),
        (75.0, rise + 50.0),
        (98.0, rise + 62.0),
        (88.0, rise + 40.0),
        (76.0, rise + 46.0),
        (66.0, rise + 22.0),
        (52.0, rise + 28.0),
        (42.0, rise + 8.0),
        (28.0, rise + 12.0),
        (16.0, wing_bottom + 2.0),
    ];
    let mut points = vec![(0.0, rise)];
    points.extend(right);
    points.push((0.0, wing_bottom));
    points.extend(right.iter().rev().map(|(x, y)| (-x, *y)));
    let wing_area = difference(vec![
        vec![polygon(&points)],
        vertical_slot(0.0, wing_bottom - 5.0, meet),
    ]);
    let wing_outline = outline_of(wing_area);
    let mut veins = Vec::new();
    for side in [1.0, -1.0] {
        for (x, y) in [(98.0, 62.0), (88.0, 40.0), (66.0, 22.0), (42.0, 8.0)] {
            veins.push(line(
                point(side * 6.0, rise - 4.0),
                point(side * x, rise + y),
            ));
        }
    }
    let veins = clip_lines(
        &veins,
        &region_of(&offset_paths(std::slice::from_ref(&wing_outline), -2.0)),
    );
    let wings = Part::new(wings_name, wing_outline).with_engraving(veins);
    let rear_legs = leg_parts.pop().expect("rear legs");
    let front_legs = leg_parts.pop().expect("front legs");
    Dragon {
        body,
        wings,
        front_legs,
        rear_legs,
        crossings,
    }
}

/// A dragon to slot together: body, wings and legs in 3 mm plywood,
/// scales and wing veins engraved.
pub fn dragon(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "dragon");
    b.stock(BOARD.0, BOARD.1);
    let parts = dragon_parts([
        b.t("part.body"),
        b.t("part.wings"),
        b.t("part.front_legs"),
        b.t("part.rear_legs"),
    ]);
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(
            vec![parts.body, parts.wings, parts.front_legs, parts.rear_legs],
            4.0,
        ),
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

/// Sphere radius, inner edge of the ribs, heights of the rings, number
/// of ribs.
pub const GLOBE_RADIUS: f64 = 55.0;
pub const GLOBE_RIB_INNER: f64 = 12.0;
pub const GLOBE_RINGS: [f64; 5] = [-40.0, -20.0, 0.0, 20.0, 40.0];
pub const GLOBE_RIBS: usize = 8;

/// Where a ring at height `z` meets a rib: the rib's slot from its
/// inner edge to `meet`, the ring's from its rim back to `meet`.
pub fn globe_meet(z: f64) -> (f64, f64) {
    let rim = (GLOBE_RADIUS * GLOBE_RADIUS - z * z).sqrt();
    (rim, (GLOBE_RIB_INNER + rim) / 2.0)
}

/// Slotted globe: horizontal rings and vertical ribs. The ribs slide in
/// from outside, their slots opening inwards; the rings' slots open
/// from their rim.
pub fn slotted_globe(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "globe");
    b.stock(BOARD.0, BOARD.1);
    let mut parts = Vec::new();
    // Rib: the part of the half disc beyond the inner edge.
    let reach = (GLOBE_RIB_INNER / GLOBE_RADIUS).acos().to_degrees();
    let steps = 90;
    let arc: Vec<Point> = (0..=steps)
        .map(|step| {
            polar(
                point(0.0, 0.0),
                GLOBE_RADIUS,
                -reach + 2.0 * reach * step as f64 / steps as f64,
            )
        })
        .collect();
    let mut rib = vec![vec![Path::new(arc, true)]];
    for z in GLOBE_RINGS {
        let (_, meet) = globe_meet(z);
        rib.push(vec![rect(
            GLOBE_RIB_INNER - 2.0,
            z - THICKNESS / 2.0,
            meet - GLOBE_RIB_INNER + 2.0,
            THICKNESS,
        )]);
    }
    let rib = outline_of(difference(rib));
    for index in 0..GLOBE_RIBS {
        parts.push(Part::new(
            b.tf("part.rib", &[&(index + 1).to_string()]),
            rib.clone(),
        ));
    }
    let centre = point(0.0, 0.0);
    for (index, z) in GLOBE_RINGS.iter().enumerate() {
        let (rim, meet) = globe_meet(*z);
        let mut shapes = vec![vec![circle(centre, rim)], vec![circle(centre, 6.0)]];
        for rib_index in 0..GLOBE_RIBS {
            let angle = rib_index as f64 * 360.0 / GLOBE_RIBS as f64;
            let slot = rect(meet, -THICKNESS / 2.0, rim + 2.0 - meet, THICKNESS);
            shapes.push(vec![slot.transformed(&Transform::rotate_degrees(angle))]);
        }
        // Rings of the same size share a number: 1 at the poles, 3 at
        // the equator.
        let number = 3 - (z.abs() / 20.0).round() as usize;
        parts.push(
            Part::from_area(
                b.tf("part.ring", &[&(index + 1).to_string()]),
                difference(shapes),
            )
            .with_engraving(stroke_text(
                &number.to_string(),
                5.0,
                point(0.0, 6.0 + 6.0),
            )),
        );
    }
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 3.0),
        BoardOps {
            engrave: Some(settings::laser_line()),
            engrave_alone: false,
            holes: settings::laser_cut(),
            outline: settings::laser_cut(),
        },
        "board",
    );
    b.step("s2", Vec::new());
    b.finish(MachineKind::Laser)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragon_slots_meet_halfway_and_stand_on_its_feet() {
        let dragon = dragon_parts(["b".into(), "w".into(), "f".into(), "r".into()]);
        assert_eq!(dragon.crossings.len(), 3);
        for crossing in &dragon.crossings {
            // Both slots end at the same height, halfway along the
            // line where the parts overlap.
            let middle = (crossing.body_edge + crossing.part_edge) / 2.0;
            assert!((crossing.meet - middle).abs() < 1e-9);
            // The body really has a slot there: its edge moved to the
            // meeting height.
            let (low, high) = vertical_extent(&dragon.body.outline, crossing.x).unwrap();
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
        // Feet on the ground, the tail above it.
        for legs in [&dragon.front_legs, &dragon.rear_legs] {
            assert!(legs.bounds().min.y.abs() < 0.01, "{:?}", legs.bounds());
        }
        assert!(dragon.body.bounds().min.y > 30.0);
        assert!(dragon.body.bounds().width() < BOARD.0 - 10.0);
        assert!(dragon.wings.bounds().width() < BOARD.0 - 10.0);
    }

    #[test]
    fn globe_slots_share_each_crossing() {
        for z in GLOBE_RINGS {
            let (rim, meet) = globe_meet(z);
            assert!(meet > GLOBE_RIB_INNER && meet < rim);
            // The ring keeps a solid band inside its slots.
            assert!(meet - 6.0 > 10.0);
        }
        // Ribs are 3 mm apart at least where they come closest.
        let gap = GLOBE_RIB_INNER * (std::f64::consts::TAU / GLOBE_RIBS as f64) - THICKNESS;
        assert!(gap > 3.0);
    }
}
