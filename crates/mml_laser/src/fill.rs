use crate::LaserSettings;
use mml_core::geometry::{Path, Point, Rect, Transform};
use mml_core::layer::FillParams;
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
    for _ in 0..params.passes.max(1) {
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
}
