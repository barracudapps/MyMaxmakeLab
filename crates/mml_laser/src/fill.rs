use crate::LaserSettings;
use mml_core::geometry::{Path, Point, Rect, Transform};
use mml_core::layer::{FillParams, FillPattern};
use mml_gcode::GcodeWriter;

/// Inside intervals of a horizontal line at `y`. Each item of `shapes`
/// is filled with the even-odd rule (so letters and holes work), and the
/// shapes are united with each other.
pub fn scan_intervals(shapes: &[Vec<Path>], y: f64) -> Vec<(f64, f64)> {
    let mut intervals = Vec::new();
    let mut crossings = Vec::new();
    for paths in shapes {
        crossings.clear();
        for path in paths
            .iter()
            .filter(|path| path.closed && path.points.len() > 2)
        {
            let count = path.points.len();
            for index in 0..count {
                let a = path.points[index];
                let b = path.points[(index + 1) % count];
                if (a.y > y) != (b.y > y) {
                    crossings.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        intervals.extend(
            crossings
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[start, end]| (*start, *end)),
        );
    }
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(intervals.len());
    for (start, end) in intervals {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged.retain(|(start, end)| end - start > 1e-6);
    merged
}

pub fn emit_fill(
    writer: &mut GcodeWriter,
    head: &mut Point,
    shapes: Vec<Vec<Path>>,
    params: &FillParams,
    settings: &LaserSettings,
) {
    if shapes
        .iter()
        .all(|paths| paths.iter().all(|path| !path.closed))
    {
        return;
    }
    let angles: &[f64] = if params.crosshatch {
        &[params.angle, params.angle + 90.0]
    } else {
        std::slice::from_ref(&params.angle)
    };
    if params.air_assist {
        writer.air(true);
    }
    writer.laser_on(settings.dynamic_power);
    let power = settings.s_value(params.power);
    let curves = match params.pattern {
        FillPattern::Lines => None,
        FillPattern::Circles => Some(circles(&shapes, params.interval.max(0.01))),
        FillPattern::Contours => Some(contours(&shapes, params.interval.max(0.01))),
    };
    for _ in 0..params.passes.max(1) {
        if let Some(curves) = &curves {
            burn_curves(writer, head, curves, params.speed, power);
            continue;
        }
        for &angle in angles {
            // Work in a frame where scan lines are horizontal.
            let to_scan = Transform::rotate_degrees(-angle);
            let to_world = Transform::rotate_degrees(angle);
            let rotated: Vec<Vec<Path>> = shapes
                .iter()
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| path.transformed(&to_scan))
                        .collect()
                })
                .collect();
            let bounds = rotated
                .iter()
                .flatten()
                .fold(Rect::EMPTY, |rect, path| rect.union(path.bounds()));
            scan(writer, head, &rotated, bounds, &to_world, params, power);
        }
    }
    writer.tool_off();
    if params.air_assist {
        writer.air(false);
    }
}

/// Even-odd inside test, shapes united.
fn inside(shapes: &[Vec<Path>], point: Point) -> bool {
    shapes.iter().any(|paths| {
        let mut inside = false;
        for path in paths
            .iter()
            .filter(|path| path.closed && path.points.len() > 2)
        {
            let count = path.points.len();
            for index in 0..count {
                let a = path.points[index];
                let b = path.points[(index + 1) % count];
                if (a.y > point.y) != (b.y > point.y)
                    && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
                {
                    inside = !inside;
                }
            }
        }
        inside
    })
}

/// Circles around the centre of the design, clipped to its inside.
fn circles(shapes: &[Vec<Path>], interval: f64) -> Vec<Vec<Point>> {
    let bounds = shapes
        .iter()
        .flatten()
        .fold(Rect::EMPTY, |rect, path| rect.union(path.bounds()));
    let centre = Point::new(
        (bounds.min.x + bounds.max.x) / 2.0,
        (bounds.min.y + bounds.max.y) / 2.0,
    );
    let reach = bounds.width().hypot(bounds.height()) / 2.0;
    let mut curves = Vec::new();
    let mut radius = interval / 2.0;
    let mut forward = true;
    while radius <= reach {
        // Chords within 0.01 mm of the true circle.
        let chord = (0.08 * radius).sqrt().clamp(0.02, 1.0);
        let count = ((std::f64::consts::TAU * radius / chord).ceil() as usize).max(16);
        let mut points: Vec<(Point, bool)> = (0..count)
            .map(|index| {
                let mut angle = std::f64::consts::TAU * index as f64 / count as f64;
                if !forward {
                    angle = -angle;
                }
                let point = Point::new(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                );
                (point, inside(shapes, point))
            })
            .collect();
        if points.iter().all(|(_, inside)| *inside) {
            let mut circle: Vec<Point> = points.iter().map(|(point, _)| *point).collect();
            circle.push(circle[0]);
            curves.push(circle);
        } else if let Some(outside) = points.iter().position(|(_, inside)| !inside) {
            // Start on an outside point so arcs are never cut in two.
            points.rotate_left(outside);
            let mut arc: Vec<Point> = Vec::new();
            for (point, is_inside) in points
                .into_iter()
                .chain(std::iter::once((Point::default(), false)))
            {
                if is_inside {
                    arc.push(point);
                } else if arc.len() > 1 {
                    curves.push(std::mem::take(&mut arc));
                } else {
                    arc.clear();
                }
            }
        }
        radius += interval;
        forward = !forward;
    }
    curves
}

/// Rings following each outline inwards.
fn contours(shapes: &[Vec<Path>], interval: f64) -> Vec<Vec<Point>> {
    let mut curves = Vec::new();
    for paths in shapes {
        let region = mml_core::region::region_of(paths);
        let mut depth = interval / 2.0;
        loop {
            let ring = mml_core::region::offset(&region, -depth);
            if ring.0.is_empty() || depth > 10_000.0 {
                break;
            }
            for polygon in &ring.0 {
                for line in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
                    let path = mml_core::region::to_path(line);
                    if path.points.len() > 2 {
                        let mut points = path.points;
                        points.push(points[0]);
                        curves.push(points);
                    }
                }
            }
            depth += interval;
        }
    }
    curves
}

/// Burns each polyline, nearest first, travelling with the laser off.
fn burn_curves(
    writer: &mut GcodeWriter,
    head: &mut Point,
    curves: &[Vec<Point>],
    speed: f64,
    power: f64,
) {
    let mut left: Vec<&Vec<Point>> = curves.iter().filter(|curve| curve.len() > 1).collect();
    while !left.is_empty() {
        let (index, reversed) = left
            .iter()
            .enumerate()
            .flat_map(|(index, curve)| {
                [
                    (index, false, curve[0].distance(*head)),
                    (index, true, curve[curve.len() - 1].distance(*head)),
                ]
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(index, reversed, _)| (index, reversed))
            .unwrap();
        let curve = left.swap_remove(index);
        let points: Vec<Point> = if reversed {
            curve.iter().rev().copied().collect()
        } else {
            curve.clone()
        };
        writer.rapid(points[0].x, points[0].y);
        for point in &points[1..] {
            writer.linear(point.x, point.y, speed, Some(power));
        }
        *head = *points.last().unwrap();
    }
}

fn scan(
    writer: &mut GcodeWriter,
    head: &mut Point,
    shapes: &[Vec<Path>],
    bounds: Rect,
    to_world: &Transform,
    params: &FillParams,
    power: f64,
) {
    let interval = params.interval.max(0.01);
    let rows = (bounds.height() / interval).floor() as usize + 1;
    let mut forward = true;
    let mut started = false;
    for row in 0..rows {
        // Offset by half a line so the outline itself is not doubled.
        let y = bounds.min.y + interval * (row as f64 + 0.5);
        if y > bounds.max.y {
            break;
        }
        let mut intervals = scan_intervals(shapes, y);
        if intervals.is_empty() {
            continue;
        }
        if !forward {
            intervals.reverse();
            for interval in &mut intervals {
                *interval = (interval.1, interval.0);
            }
        }
        let sign = if forward { 1.0 } else { -1.0 };
        let entry = to_world.apply(Point::new(intervals[0].0 - sign * params.overscan, y));
        if started {
            writer.linear(entry.x, entry.y, params.speed, Some(0.0));
        } else {
            writer.rapid(entry.x, entry.y);
            started = true;
        }
        for (start, end) in &intervals {
            let a = to_world.apply(Point::new(*start, y));
            let b = to_world.apply(Point::new(*end, y));
            writer.linear(a.x, a.y, params.speed, Some(0.0));
            writer.linear(b.x, b.y, params.speed, Some(power));
        }
        let last = intervals.last().unwrap().1;
        let exit = to_world.apply(Point::new(last + sign * params.overscan, y));
        writer.linear(exit.x, exit.y, params.speed, Some(0.0));
        *head = exit;
        if params.bidirectional {
            forward = !forward;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Path {
        Path::new(
            vec![
                Point::new(x, y),
                Point::new(x + size, y),
                Point::new(x + size, y + size),
                Point::new(x, y + size),
            ],
            true,
        )
    }

    #[test]
    fn overlapping_shapes_are_united_and_holes_kept() {
        let ring = vec![square(0.0, 0.0, 10.0), square(4.0, 0.0, 2.0)];
        let other = vec![square(8.0, 0.0, 10.0)];
        let intervals = scan_intervals(&[ring, other], 1.0);
        assert_eq!(intervals, vec![(0.0, 4.0), (6.0, 18.0)]);
    }

    #[test]
    fn circles_and_contours_stay_inside_and_cover_the_shape() {
        let shapes = vec![vec![square(0.0, 0.0, 10.0), square(4.0, 4.0, 2.0)]];
        for pattern in [FillPattern::Circles, FillPattern::Contours] {
            let params = FillParams {
                pattern,
                interval: 0.5,
                ..FillParams::default()
            };
            let mut writer = GcodeWriter::new();
            let settings = LaserSettings {
                max_s: 1000.0,
                dynamic_power: true,
            };
            emit_fill(
                &mut writer,
                &mut Point::default(),
                shapes.clone(),
                &params,
                &settings,
            );
            let program = mml_gcode::parse(&writer.finish());
            let burns: Vec<_> = program.motions.iter().filter(|m| m.is_cutting()).collect();
            assert!(!burns.is_empty(), "{pattern:?}");
            for motion in &burns {
                let mid = Point::new(
                    (motion.from[0] + motion.to[0]) / 2.0,
                    (motion.from[1] + motion.to[1]) / 2.0,
                );
                let in_hole = (4.05..5.95).contains(&mid.x) && (4.05..5.95).contains(&mid.y);
                assert!(!in_hole, "{pattern:?} burns the hole at {mid:?}");
                assert!((-0.05..10.05).contains(&mid.x) && (-0.05..10.05).contains(&mid.y));
            }
            let bounds = program.cut_bounds();
            assert!(
                bounds.width() > 9.0 && bounds.height() > 9.0,
                "{pattern:?} {bounds:?}"
            );
        }
    }
}
