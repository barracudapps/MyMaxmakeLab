//! Wall clock with a pendulum, in 6 mm plywood, every part milled with
//! a 2 mm end mill.
//!
//! Train: the weight turns the minute arbor once an hour. Its wheel
//! (50 teeth) drives a 10-leaf pinion, whose wheel (40) drives another
//! 10-leaf pinion, whose wheel (36) drives the 12-leaf pinion of the
//! escape wheel: 5 x 4 x 3 = 60, the escape wheel turns once a minute.
//! It has 40 teeth and lets one through per swing and back of the
//! pendulum, so the pendulum beats 1.5 s a period: about 559 mm long.
//! Motion work in front: 10/30 then 10/40 = 12, the hour hand.

use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{BoardOps, Builder, settings};
use mml_core::geometry::{Path, Point};
use mml_core::layer::ProfileSide;
use mml_core::{Document, MachineKind};

/// Module of the train and of the second stage of the motion work.
const MODULE: f64 = 2.0;
/// Module of the cannon pinion and minute wheel (first stage).
const MOTION_MODULE: f64 = 2.5;
const ESCAPE_TEETH: usize = 40;
const ESCAPE_RADIUS: f64 = 42.0;
const ESCAPE_DEPTH: f64 = 6.0;
/// Half the angle the anchor spans on the escape wheel: 7.5 teeth.
const ANCHOR_HALF: f64 = 7.5 * 360.0 / ESCAPE_TEETH as f64 / 2.0;
/// Arbor holes in the plates (the arbor turns), and bores of the parts
/// glued on an arbor.
const RUNNING: f64 = 4.3;
const TIGHT: f64 = 4.0;
const PILLAR: f64 = 10.0;

/// Arbors, seen from the front, minute arbor at the origin.
pub(crate) struct Layout {
    pub minute: Point,
    pub second: Point,
    pub third: Point,
    pub escape: Point,
    pub pallet: Point,
    pub stud: Point,
    pub pillars: [Point; 3],
    pub dial_feet: [Point; 3],
    pub hang: Point,
}

pub(crate) fn layout() -> Layout {
    let minute = point(0.0, 0.0);
    // Centre distances: (teeth + leaves) x module / 2.
    let second = polar(minute, (50.0 + 10.0) * MODULE / 2.0, 0.0);
    let third = polar(second, (40.0 + 10.0) * MODULE / 2.0, 115.0);
    let escape = polar(third, (36.0 + 12.0) * MODULE / 2.0, 165.0);
    let pallet = escape + point(0.0, ESCAPE_RADIUS / ANCHOR_HALF.to_radians().cos());
    Layout {
        minute,
        second,
        third,
        escape,
        pallet,
        stud: point(-(10.0 + 30.0) * MOTION_MODULE / 2.0, 0.0),
        pillars: [point(-57.0, 48.0), point(35.0, -55.0), point(-50.0, -55.0)],
        dial_feet: [
            polar(minute, 47.0, 110.0),
            polar(minute, 47.0, 30.0),
            polar(minute, 47.0, -50.0),
        ],
        hang: pallet + point(0.0, 20.0),
    }
}

/// Escape wheel: pointed teeth leaning forward, the wheel turning
/// counter-clockwise seen from the front.
fn escape_wheel(centre: Point) -> Vec<Path> {
    let pitch = 360.0 / ESCAPE_TEETH as f64;
    let (tip, root) = (ESCAPE_RADIUS, ESCAPE_RADIUS - ESCAPE_DEPTH);
    let mut points = Vec::new();
    for tooth in 0..ESCAPE_TEETH {
        let base = -(tooth as f64) * pitch;
        // Walking clockwise: tip, flat, back down to the root, along the
        // root, then up the leading face of the next tooth (undercut).
        points.push(polar(centre, tip, base));
        points.push(polar(centre, tip, base - 0.6));
        points.push(polar(centre, root, base - 5.5));
        points.push(polar(centre, root, base - 8.0));
        points.push(polar(centre, root, base - pitch - 1.5));
    }
    let mut outline = Path::new(points, true);
    if outline.signed_area() < 0.0 {
        outline = outline.reversed();
    }
    let mut holes = vec![circle(centre, TIGHT / 2.0)];
    holes.extend(spoke_windows(centre, 9.0, root - 5.0, 4, 6.0));
    let mut all = vec![outline];
    all.extend(holes);
    all
}

/// Distance from `centre` of the point at `radius` and `degrees`
/// around `pivot`.
fn reach(pivot: Point, radius: f64, degrees: f64, centre: Point) -> f64 {
    polar(pivot, radius, degrees).distance(centre)
}

/// Angle around `pivot`, from `start` towards `direction` (+1 or -1
/// degree steps), where the point at `radius` is `target` from
/// `centre`.
fn angle_at(
    pivot: Point,
    radius: f64,
    start: f64,
    direction: f64,
    target: f64,
    centre: Point,
) -> f64 {
    let mut angle = start;
    let mut previous = reach(pivot, radius, angle, centre) - target;
    for _ in 0..4000 {
        let next = angle + direction * 0.02;
        let value = reach(pivot, radius, next, centre) - target;
        if previous.signum() != value.signum() {
            return next;
        }
        previous = value;
        angle = next;
    }
    angle
}

/// Graham (dead-beat) anchor: pallets that are arcs around its pivot,
/// impulse faces at their ends, a hub on the pivot.
fn anchor(layout: &Layout) -> Vec<Path> {
    let (pivot, centre) = (layout.pallet, layout.escape);
    let tangent = pivot.distance(centre).powi(2) - ESCAPE_RADIUS.powi(2);
    let length = tangent.sqrt();
    // Pallet width: half a tooth pitch along the rim, minus the drop.
    let width = std::f64::consts::PI * ESCAPE_RADIUS / ESCAPE_TEETH as f64 - 0.5;
    let mut shapes = vec![vec![circle(pivot, 9.0)]];
    for side in [-1.0, 1.0] {
        // Right pallet (side +1) is the entry one: teeth arrive from
        // its outer face. The left one, the exit, from its inner face.
        let contact = polar(centre, ESCAPE_RADIUS, 90.0 - side * ANCHOR_HALF);
        let middle = (contact - pivot).normalized();
        let start = middle.y.atan2(middle.x).to_degrees();
        let inward =
            if reach(pivot, length, start + 0.5, centre) < reach(pivot, length, start, centre) {
                1.0
            } else {
                -1.0
            };
        let (outer, inner) = (length + width / 2.0, length - width / 2.0);
        // First contact corner deeper than the other: the impulse face
        // lifts the pallet out as the tooth slides along it.
        let (deep, shallow) = if side > 0.0 {
            (outer, inner)
        } else {
            (inner, outer)
        };
        let deep_end = angle_at(pivot, deep, start, inward, ESCAPE_RADIUS - 1.5, centre);
        let shallow_end = angle_at(pivot, shallow, start, inward, ESCAPE_RADIUS - 0.3, centre);
        let deep_root = angle_at(pivot, deep, start, -inward, ESCAPE_RADIUS + 6.0, centre);
        let shallow_root = angle_at(pivot, shallow, start, -inward, ESCAPE_RADIUS + 6.0, centre);
        let arc = |radius: f64, from: f64, to: f64| -> Vec<Point> {
            let steps = 12;
            (0..=steps)
                .map(|step| {
                    polar(
                        pivot,
                        radius,
                        from + (to - from) * step as f64 / steps as f64,
                    )
                })
                .collect()
        };
        let mut points = arc(deep, deep_root, deep_end);
        points.extend(arc(shallow, shallow_end, shallow_root));
        shapes.push(vec![Path::new(points, true)]);
        // Arm from the outer end of the pallet to the hub.
        let root = polar(pivot, length, (deep_root + shallow_root) / 2.0);
        shapes.push(bar(root, pivot, 7.0));
    }
    let mut area = union(shapes);
    area.push(circle(pivot, TIGHT / 2.0));
    area
}

/// Ratchet of the winding drum: 24 saw teeth.
fn ratchet(centre: Point) -> Vec<Path> {
    let mut points = Vec::new();
    for tooth in 0..24 {
        let angle = tooth as f64 * 15.0;
        points.push(polar(centre, 20.0, angle));
        points.push(polar(centre, 24.0, angle + 14.0));
        points.push(polar(centre, 20.0, angle + 14.5));
    }
    vec![Path::new(points, true), circle(centre, RUNNING / 2.0)]
}

/// Click (pawl) pivoting on the great wheel, pushed onto the ratchet by
/// a small rubber band.
fn click() -> Vec<Path> {
    let pivot = point(0.0, 0.0);
    let tip = point(14.0, -3.0);
    let mut area = union(vec![
        vec![circle(pivot, 5.0)],
        bar(pivot, tip, 4.5),
        vec![polygon(&[(12.0, -5.2), (17.0, -3.5), (12.0, -0.8)])],
    ]);
    area.push(circle(pivot, 1.5));
    area
}

fn hand(length: f64, leaf: f64, bore: f64) -> Vec<Path> {
    let centre = point(0.0, 0.0);
    let blade = polygon(&[
        (-leaf * 0.5, 0.0),
        (-leaf, length * 0.55),
        (0.0, length),
        (leaf, length * 0.55),
        (leaf * 0.5, 0.0),
    ]);
    let mut area = union(vec![
        vec![blade],
        vec![circle(centre, bore / 2.0 + 3.5)],
        bar(centre, point(0.0, -length * 0.22), 5.0),
        vec![circle(point(0.0, -length * 0.22), 6.0)],
    ]);
    area.push(circle(centre, bore / 2.0));
    area
}

/// Skeleton plate: discs around the holes, joined by bars.
fn plate(layout: &Layout, front: bool) -> Vec<Path> {
    let l = layout;
    let arbors = [l.minute, l.second, l.third, l.escape, l.pallet];
    let mut shapes: Vec<Vec<Path>> = arbors.iter().map(|p| vec![circle(*p, 11.0)]).collect();
    shapes.extend(l.pillars.iter().map(|p| vec![circle(*p, 11.0)]));
    let mut bars = vec![
        (l.pallet, l.escape),
        (l.escape, l.third),
        (l.third, l.second),
        (l.second, l.minute),
        (l.minute, l.escape),
        (l.escape, l.pillars[0]),
        (l.pillars[0], l.minute),
        (l.minute, l.pillars[1]),
        (l.minute, l.pillars[2]),
        (l.pillars[1], l.second),
        (l.pillars[1], l.pillars[2]),
        (l.pallet, l.pillars[0]),
    ];
    if front {
        shapes.push(vec![circle(l.stud, 9.0)]);
        bars.extend([
            (l.stud, l.minute),
            (l.stud, l.pillars[0]),
            (l.stud, l.pillars[2]),
        ]);
        for foot in l.dial_feet {
            shapes.push(vec![circle(foot, 7.0)]);
            bars.push((foot, l.minute));
        }
    } else {
        shapes.push(vec![circle(l.hang, 8.0)]);
        bars.push((l.pallet, l.hang));
    }
    for (a, b) in bars {
        shapes.push(bar(a, b, 10.0));
    }
    let mut area = union(shapes);
    area.extend(arbors.iter().map(|p| circle(*p, RUNNING / 2.0)));
    area.extend(l.pillars.iter().map(|p| circle(*p, PILLAR / 2.0)));
    if front {
        area.push(circle(l.stud, TIGHT / 2.0));
        area.extend(l.dial_feet.iter().map(|p| circle(*p, 3.0)));
    } else {
        area.push(circle(l.hang, 2.5));
    }
    area
}

fn dial(layout: &Layout) -> (Vec<Path>, Vec<Path>) {
    let centre = layout.minute;
    let mut area = vec![circle(centre, 70.0), circle(centre, 3.5)];
    area.extend(layout.dial_feet.iter().map(|p| circle(*p, 3.0)));
    let mut lines = Vec::new();
    for minute in 0..60 {
        let angle = 90.0 - minute as f64 * 6.0;
        let inner = if minute % 5 == 0 { 59.0 } else { 64.0 };
        lines.push(line(
            polar(centre, inner, angle),
            polar(centre, 67.0, angle),
        ));
    }
    for (number, angle) in [("12", 90.0), ("3", 0.0), ("6", -90.0), ("9", 180.0)] {
        lines.extend(text(
            number,
            "Hershey Roman",
            11.0,
            polar(centre, 50.0, angle),
        ));
    }
    lines.push(circle(centre, 67.0));
    (area, lines)
}

fn gear_part(name: String, teeth: usize, module: f64, bore: f64, windows: bool) -> Part {
    let centre = point(0.0, 0.0);
    let (outline, mut holes) = gear(teeth, module, bore, centre);
    let root = module * teeth as f64 / 2.0 - 1.25 * module;
    if windows {
        holes.extend(spoke_windows(centre, bore / 2.0 + 6.0, root - 5.0, 5, 6.0));
    }
    Part::new(name, outline).with_holes(holes)
}

pub fn pendulum_clock(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "pendulum_clock");
    b.stock_board();
    let l = layout();
    let origin = point(0.0, 0.0);
    let mut parts = vec![
        Part::from_area(b.t("part.back_plate"), plate(&l, false)),
        Part::from_area(b.t("part.front_plate"), plate(&l, true)),
    ];
    let (dial_area, dial_lines) = dial(&l);
    parts.push(Part::from_area(b.t("part.dial"), dial_area).with_engraving(dial_lines));
    // Great wheel: solid, it carries the click.
    parts.push(
        gear_part(b.t("part.wheel_a"), 50, MODULE, TIGHT, false).with_holes(vec![
            circle(point(31.0, 0.0), 1.5),
            circle(polar(origin, 31.0, 50.0), 1.5),
        ]),
    );
    parts.push(gear_part(b.t("part.wheel_b"), 40, MODULE, TIGHT, true));
    parts.push(gear_part(b.t("part.wheel_c"), 36, MODULE, TIGHT, true));
    parts.push(gear_part(b.t("part.pinion10"), 10, MODULE, TIGHT, false));
    parts.push(gear_part(b.t("part.pinion10"), 10, MODULE, TIGHT, false));
    parts.push(gear_part(b.t("part.pinion12"), 12, MODULE, TIGHT, false));
    parts.push(Part::from_area(b.t("part.escape"), escape_wheel(origin)));
    parts.push(Part::from_area(b.t("part.anchor"), anchor(&l)));
    parts.push(Part::from_area(b.t("part.ratchet"), ratchet(origin)));
    parts.push(
        Part::new(b.t("part.drum_core"), circle(origin, 14.0)).with_holes(vec![
            circle(origin, RUNNING / 2.0),
            circle(point(0.0, 9.0), 1.25),
        ]),
    );
    parts.push(
        Part::new(b.t("part.drum_flange"), circle(origin, 22.0))
            .with_holes(vec![circle(origin, RUNNING / 2.0)]),
    );
    parts.push(Part::from_area(b.t("part.click"), click()));
    parts.push(gear_part(
        b.t("part.cannon"),
        10,
        MOTION_MODULE,
        TIGHT,
        false,
    ));
    parts.push(gear_part(
        b.t("part.minute_wheel"),
        30,
        MOTION_MODULE,
        RUNNING,
        true,
    ));
    parts.push(gear_part(
        b.t("part.minute_pinion"),
        10,
        MODULE,
        RUNNING,
        false,
    ));
    parts.push(gear_part(b.t("part.hour_wheel"), 40, MODULE, 6.0, true));
    parts.push(Part::from_area(
        b.t("part.minute_hand"),
        hand(64.0, 3.0, TIGHT),
    ));
    parts.push(Part::from_area(b.t("part.hour_hand"), hand(46.0, 4.5, 6.0)));
    // Pendulum: hanger on the pallet arbor, notch for a 10 x 10 rod.
    let hanger = vec![
        polygon(&[
            (-12.0, -40.0),
            (-5.2, -40.0),
            (-5.2, -12.0),
            (5.2, -12.0),
            (5.2, -40.0),
            (12.0, -40.0),
            (12.0, 6.0),
            (-12.0, 6.0),
        ]),
        circle(origin, TIGHT / 2.0),
    ];
    parts.push(Part::from_area(b.t("part.hanger"), hanger));
    for _ in 0..2 {
        parts.push(
            Part::new(b.t("part.bob"), circle(origin, 40.0))
                .with_holes(vec![rect(-5.2, -5.2, 10.4, 10.4)]),
        );
    }
    b.step("s1", Vec::new());
    b.cut_boards(
        boards(parts, 6.0),
        BoardOps {
            engrave: Some(settings::v_line(0.6)),
            engrave_alone: true,
            holes: settings::profile(2.0, 6.3, ProfileSide::Inside, 0),
            outline: settings::profile(2.0, 6.3, ProfileSide::Outside, 3),
        },
        "board",
    );
    for step in ["s2", "s3", "s4", "s5", "s6", "s7"] {
        b.step(step, Vec::new());
    }
    b.finish(MachineKind::Cnc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheels_clear_the_other_arbors_and_pillars() {
        let l = layout();
        // (arbor, outside radius of what turns on it in the plates)
        let wheels = [
            (l.minute, 50.0 + MODULE, "great wheel"),
            (l.second, 40.0 + MODULE, "second wheel"),
            (l.third, 36.0 + MODULE, "third wheel"),
            (l.escape, ESCAPE_RADIUS, "escape wheel"),
        ];
        // Arbors carry 6 mm tubes; pillars are 10 mm.
        let arbors = [l.minute, l.second, l.third, l.escape, l.pallet];
        for (centre, radius, name) in wheels {
            for arbor in arbors.iter().filter(|a| a.distance(centre) > 1.0) {
                let meshing = arbor.distance(centre) < radius;
                if meshing {
                    // Only the arbor of the pinion it drives may be inside.
                    let next = match name {
                        "great wheel" => l.second,
                        "second wheel" => l.third,
                        "third wheel" => l.escape,
                        _ => panic!("{name} reaches an arbor"),
                    };
                    assert!(arbor.distance(next) < 0.01, "{name} reaches {arbor:?}");
                } else {
                    assert!(
                        arbor.distance(centre) > radius + 3.0 + 0.8,
                        "{name} {arbor:?}"
                    );
                }
            }
            for pillar in l.pillars {
                assert!(
                    pillar.distance(centre) > radius + 5.0 + 1.5,
                    "{name} {pillar:?}"
                );
            }
        }
        // Motion work in front: minute wheel around the stud, hour wheel
        // around the minute arbor, clear of the dial feet.
        for foot in l.dial_feet {
            assert!(foot.distance(l.stud) > 30.0 * MOTION_MODULE / 2.0 + MOTION_MODULE + 3.0 + 1.0);
            assert!(foot.distance(l.minute) > 40.0 * MODULE / 2.0 + MODULE + 3.0 + 1.0);
        }
        // The anchor pivot sees the wheel under 7.5 teeth.
        let half = (ESCAPE_RADIUS / l.pallet.distance(l.escape))
            .acos()
            .to_degrees();
        assert!((half - ANCHOR_HALF).abs() < 0.01);
    }

    #[test]
    fn anchor_reaches_into_the_wheel_only_with_its_pallets() {
        let l = layout();
        let area = anchor(&l);
        let outline = &area[0];
        let inside: Vec<&Point> = outline
            .points
            .iter()
            .filter(|p| p.distance(l.escape) < ESCAPE_RADIUS)
            .collect();
        assert!(!inside.is_empty());
        // Pallets go no deeper than 1.5 mm into the teeth.
        for point in &inside {
            assert!(point.distance(l.escape) > ESCAPE_RADIUS - 1.6, "{point:?}");
        }
    }

    #[test]
    fn the_pendulum_beats_a_minute_per_escape_turn() {
        // 60 x 60 s per hour at the minute arbor, train ratio 60.
        let ratio = (50.0 / 10.0) * (40.0 / 10.0) * (36.0 / 12.0);
        assert_eq!(ratio, 60.0);
        let period = 60.0 / ESCAPE_TEETH as f64;
        let length = 9.81 * (period / std::f64::consts::TAU).powi(2);
        assert!((length - 0.559).abs() < 0.001, "{length}");
        // Motion work: 12 turns of the minute hand per hour hand turn,
        // both stages on the same centre distance.
        assert_eq!((30.0 / 10.0) * (40.0 / 10.0), 12.0);
        assert_eq!((10.0 + 30.0) * MOTION_MODULE, (10.0 + 40.0) * MODULE);
    }
}
