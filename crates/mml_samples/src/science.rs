//! Science you can hold: optical illusions and the time from the sun.

use crate::mixed::laser_fill_dark;
use crate::shapes::*;
use crate::tests_projects::text_language;
use crate::{Builder, settings};
use mml_core::Document;
use mml_core::geometry::{Path, Point};
use mml_core::layer::ProfileSide;

/// Filled ring sector from `start` to `end` degrees.
fn sector(centre: Point, inner: f64, outer: f64, start: f64, end: f64) -> Path {
    let steps = ((end - start).abs() / 3.0).ceil().max(2.0) as usize;
    let mut points: Vec<Point> = (0..=steps)
        .map(|step| {
            polar(
                centre,
                outer,
                start + (end - start) * step as f64 / steps as f64,
            )
        })
        .collect();
    points.extend((0..=steps).rev().map(|step| {
        polar(
            centre,
            inner,
            start + (end - start) * step as f64 / steps as f64,
        )
    }));
    Path::new(points, true)
}

/// Benham's top: half black, arcs on the other half. Spun, the arcs
/// look coloured.
fn benham(centre: Point) -> Vec<Path> {
    let mut paths = vec![sector(centre, 0.0, 31.0, 0.0, 180.0)];
    for group in 0..4 {
        let start = 180.0 + group as f64 * 45.0;
        for arc in 0..3 {
            let inner = 7.0 + group as f64 * 6.0 + arc as f64 * 2.0;
            paths.push(sector(centre, inner, inner + 1.2, start, start + 45.0));
        }
    }
    paths
}

/// Two-arm spiral: spun, it seems to grow or shrink.
fn spiral(centre: Point) -> Vec<Path> {
    let (start, end, turns) = (5.0, 31.0, 3.0);
    let growth = (end - start) / (turns * std::f64::consts::TAU);
    let width = growth * std::f64::consts::PI / 2.0;
    let last = turns * 360.0 - width / growth * 180.0 / std::f64::consts::PI;
    let steps = 360;
    (0..2)
        .map(|arm| {
            let offset = arm as f64 * 180.0;
            let at = |degrees: f64, extra: f64| {
                let radius = start + growth * degrees.to_radians() + extra;
                polar(centre, radius, degrees + offset)
            };
            let mut points: Vec<Point> = (0..=steps)
                .map(|step| at(last * step as f64 / steps as f64, 0.0))
                .collect();
            points.extend(
                (0..=steps)
                    .rev()
                    .map(|step| at(last * step as f64 / steps as f64, width)),
            );
            Path::new(points, true)
        })
        .collect()
}

/// Rings whose centres drift: spun, they wobble.
fn drifting_rings(centre: Point) -> Vec<Path> {
    (0..7)
        .map(|index| {
            let inner = 4.0 + index as f64 * 4.0;
            let shifted = centre + point(index as f64 * 0.4, 0.0);
            difference(vec![
                vec![circle(shifted, inner + 2.0)],
                vec![circle(shifted, inner)],
            ])
        })
        .flat_map(|ring| ring.into_iter())
        .collect()
}

/// Draws the pattern of a top around its centre.
type Pattern = fn(Point) -> Vec<Path>;

/// Six spinning tops with optical illusions: the laser burns the
/// patterns, the spindle bores the axle holes and cuts the discs.
pub fn benham_tops(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "benham_tops");
    b.stock(240.0, 160.0);
    let patterns = b.layer(b.t("layer.patterns"), laser_fill_dark());
    let holes = b.layer(
        b.t("layer.holes"),
        settings::profile(3.175, 6.3, ProfileSide::Inside, 0),
    );
    let discs = b.layer(
        b.t("layer.discs"),
        settings::profile(3.175, 6.3, ProfileSide::Outside, 3),
    );
    let kinds: [(&str, Pattern); 3] = [
        ("part.benham", benham),
        ("part.spiral", spiral),
        ("part.rings", drifting_rings),
    ];
    for (row, y) in [50.0, 130.0].into_iter().enumerate() {
        for (column, (key, pattern)) in kinds.iter().enumerate() {
            let centre = point(50.0 + column as f64 * 80.0, y);
            let name = format!("{} {}", b.t(key), row + 1);
            b.add(patterns, name.clone(), pattern(centre));
            b.add(holes, name.clone(), vec![circle(centre, 3.0)]);
            b.add(discs, name, vec![circle(centre, 35.0)]);
        }
    }
    b.step("s1", Vec::new());
    b.step("s2", vec![patterns]);
    b.step("s3", vec![holes, discs]);
    b.step("s4", Vec::new());
    b.finish_hybrid()
}

/// Latitude the sundial is drawn for, degrees north.
pub const SUNDIAL_LATITUDE: f64 = 47.0;
/// Length of the gnomon along the noon line, short of the frame so the
/// 12 stays visible, mm.
const GNOMON: f64 = 65.0;
/// Where the gnomon tab starts and ends past the root, mm.
const TAB: (f64, f64) = (8.0, 53.0);

/// Angle of the shadow from the noon line on a horizontal dial at
/// `latitude`, `hours` after solar noon, degrees (positive east).
pub fn hour_angle(latitude: f64, hours: f64) -> f64 {
    let t = (hours * 15.0).to_radians();
    if (hours.abs() - 6.0).abs() < 1e-9 {
        return 90f64.copysign(hours);
    }
    let angle = (latitude.to_radians().sin() * t.tan()).atan().to_degrees();
    // Past six hours the shadow turns beyond the east-west line.
    if hours.abs() > 6.0 {
        angle + 180f64.copysign(hours)
    } else {
        angle
    }
}

/// Where a ray from `origin` along `direction` leaves `rect` (from the
/// inside).
fn exit_point(origin: Point, direction: Point, min: Point, max: Point) -> Point {
    let mut distance = f64::INFINITY;
    for (start, delta, low, high) in [
        (origin.x, direction.x, min.x, max.x),
        (origin.y, direction.y, min.y, max.y),
    ] {
        if delta > 1e-12 {
            distance = distance.min((high - start) / delta);
        } else if delta < -1e-12 {
            distance = distance.min((low - start) / delta);
        }
    }
    origin + direction * distance
}

/// Horizontal sundial: a dial plate with its hour lines burnt by the
/// laser, a gnomon held in a slot cut by the spindle.
pub fn sundial(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "sundial");
    b.stock(250.0, 125.0);
    let thickness = 6.0;
    let latitude = SUNDIAL_LATITUDE;
    let lines_layer = b.layer(b.t("layer.lines"), settings::laser_line());
    let slot_layer = b.layer(
        b.t("layer.slot"),
        settings::profile(3.175, thickness + 0.3, ProfileSide::Inside, 0),
    );
    let outline_layer = b.layer(
        b.t("layer.outline"),
        settings::profile(3.175, thickness + 0.3, ProfileSide::Outside, 3),
    );
    // Dial plate 150 x 115 from (15, 15); the style meets the plate at
    // the root, the gnomon faces 3 mm either side of the noon line.
    let (min, max) = (point(27.0, 27.0), point(153.0, 118.0));
    let root = point(90.0, 45.0);
    let half = thickness / 2.0;
    let mut lines = vec![rect(min.x, min.y, max.x - min.x, max.y - min.y)];
    let ray = |hours: f64| {
        let angle = hour_angle(latitude, hours).to_radians();
        let side = if hours.abs() < 1e-9 {
            0.0
        } else {
            half.copysign(hours)
        };
        (root + point(side, 0.0), point(angle.sin(), angle.cos()))
    };
    for hour in 6..=18 {
        let hours = hour as f64 - 12.0;
        // The gnomon faces stand on the noon lines: only their ends,
        // past the gnomon, are drawn.
        let (origins, start) = if hour == 12 {
            (
                vec![
                    (root - point(half, 0.0), point(0.0, 1.0)),
                    (root + point(half, 0.0), point(0.0, 1.0)),
                ],
                GNOMON,
            )
        } else {
            (vec![ray(hours)], 12.0)
        };
        for (origin, direction) in origins {
            let end = exit_point(origin, direction, min, max);
            lines.push(line(origin + direction * start, end));
        }
        let (origin, direction) = ray(hours);
        let end = exit_point(origin, direction, min, max);
        lines.extend(stroke_text(&hour.to_string(), 5.0, end + direction * 6.0));
    }
    for half_hour in 0..12 {
        let hours = half_hour as f64 - 5.5;
        let (origin, direction) = ray(hours);
        let end = exit_point(origin, direction, min, max);
        lines.push(line(end - direction * 5.0, end));
    }
    lines.extend(stroke_text(
        &format!("{latitude:.0} N"),
        4.0,
        point(root.x, 34.0),
    ));
    b.add(lines_layer, b.t("layer.lines"), lines);
    // Through slot, 0.2 mm wider than the gnomon. Its straight part
    // runs 2 mm past each end of the tab: the round ends left by the
    // cutter and the fillets it leaves where the tab meets the gnomon
    // stay clear, so the gnomon sits flat.
    b.add(
        slot_layer,
        b.t("part.slot"),
        vec![rounded_rect(
            root.x - 3.1,
            root.y + TAB.0 - 2.0 - 3.1,
            6.2,
            TAB.1 - TAB.0 + 4.0 + 6.2,
            3.1,
        )],
    );
    b.add(
        outline_layer,
        b.t("part.dial"),
        vec![rounded_rect(15.0, 15.0, 150.0, 115.0, 8.0)],
    );
    // Gnomon: the style rises from the root at the latitude angle; its
    // tab goes through the plate.
    let foot = point(175.0, 25.0);
    let height = GNOMON * latitude.to_radians().tan();
    let triangle = polygon(&[
        (foot.x, foot.y),
        (foot.x + GNOMON, foot.y),
        (foot.x + GNOMON, foot.y + height),
    ]);
    let tab = rect(
        foot.x + TAB.0,
        foot.y - thickness,
        TAB.1 - TAB.0,
        thickness + 3.0,
    );
    let gnomon = union(vec![vec![triangle], vec![tab]]);
    b.add(outline_layer, b.t("part.gnomon"), gnomon);
    b.step("s1", Vec::new());
    b.step("s2", vec![lines_layer]);
    b.step("s3", vec![slot_layer, outline_layer]);
    b.step("s4", Vec::new());
    b.step("s5", Vec::new());
    b.finish_hybrid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_lines_follow_the_sundial_formula() {
        assert_eq!(hour_angle(47.0, 0.0), 0.0);
        assert_eq!(hour_angle(47.0, 6.0), 90.0);
        assert_eq!(hour_angle(47.0, -6.0), -90.0);
        // 3 pm: tan(angle) = sin(47°) × tan(45°).
        let three = hour_angle(47.0, 3.0);
        assert!((three.to_radians().tan() - 47f64.to_radians().sin()).abs() < 1e-9);
        assert!((hour_angle(47.0, -3.0) + three).abs() < 1e-9);
        // At the pole the lines are 15° apart, at the equator they all
        // crowd towards noon.
        assert!((hour_angle(90.0, 1.0) - 15.0).abs() < 1e-9);
        assert!(hour_angle(10.0, 3.0) < hour_angle(60.0, 3.0));
    }

    #[test]
    fn gnomon_tab_fits_the_slot() {
        let document = sundial("fr");
        let shape_named = |name: &str| {
            document
                .shapes
                .iter()
                .find(|shape| shape.name == name)
                .expect(name)
                .world_paths()
        };
        let slot = mml_core::geometry::bounds_of(&shape_named(&crate::text::get(
            "fr",
            "sundial.part.slot",
        )));
        let gnomon = mml_core::geometry::bounds_of(&shape_named(&crate::text::get(
            "fr",
            "sundial.part.gnomon",
        )));
        // Slot along the noon line, 0.2 mm wider than the gnomon, its
        // straight part 4 mm longer than the tab, all of it under the
        // gnomon; the gnomon rises at the latitude.
        assert!((slot.width() - 6.2).abs() < 0.01);
        assert!((slot.height() - 6.2 - (TAB.1 - TAB.0 + 4.0)).abs() < 0.01);
        assert!(slot.min.y >= 45.0 && slot.max.y <= 45.0 + GNOMON);
        let rise = gnomon.height() - 6.0;
        assert!((rise / GNOMON - SUNDIAL_LATITUDE.to_radians().tan()).abs() < 0.01);
    }
}
