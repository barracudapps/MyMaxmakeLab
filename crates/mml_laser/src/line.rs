use crate::LaserSettings;
use mml_core::attach;
use mml_core::geometry::{Path, Point};
use mml_core::layer::LineParams;
use mml_core::ordering::order_paths;
use mml_gcode::GcodeWriter;

/// `emit_line` for the paths of several shapes: with bridges enabled
/// the outer outline of each shape keeps small uncut bridges.
pub fn emit_line_shapes(
    writer: &mut GcodeWriter,
    head: &mut Point,
    shapes: Vec<Vec<Path>>,
    params: &LineParams,
    settings: &LaserSettings,
) {
    if params.bridge_width <= 0.0 {
        emit_line(writer, head, shapes.concat(), params, settings);
        return;
    }
    let count = (params.bridge_count > 0).then_some(params.bridge_count as usize);
    let mut paths = Vec::new();
    for shape in shapes {
        let outer = attach::outer_rings(&shape);
        for (index, path) in shape.into_iter().enumerate() {
            if outer.contains(&index) {
                let centres = attach::attachment_points(&path, count, params.bridge_width);
                paths.extend(attach::split_at_bridges(
                    &path,
                    &centres,
                    params.bridge_width,
                ));
            } else {
                paths.push(path);
            }
        }
    }
    emit_line(writer, head, paths, params, settings);
}

pub fn emit_line(
    writer: &mut GcodeWriter,
    head: &mut Point,
    paths: Vec<Path>,
    params: &LineParams,
    settings: &LaserSettings,
) {
    let paths: Vec<Path> = paths
        .into_iter()
        .map(|path| path.cleaned(1e-4))
        .filter(|path| path.points.len() >= 2)
        .collect();
    if paths.is_empty() {
        return;
    }
    let ordered = order_paths(paths, *head);
    let power = settings.s_value(params.power);
    if params.air_assist {
        writer.air(true);
    }
    writer.laser_on(settings.dynamic_power);
    for pass in 0..params.passes.max(1) {
        if pass > 0 && params.z_step > 0.0 {
            writer.raw(&format!("G91\nG0 Z{:.3}\nG90", -params.z_step));
        }
        for path in &ordered {
            let first = path.points[0];
            writer.rapid(first.x, first.y);
            let mut points = path.points.clone();
            if path.closed {
                points.push(first);
            }
            if params.dash_off > 0.0 && params.dash_on > 0.0 {
                dashed(writer, &points, params, power);
            } else {
                for point in &points[1..] {
                    writer.linear(point.x, point.y, params.speed, Some(power));
                }
            }
            let end = *points.last().unwrap();
            writer.linear(end.x, end.y, params.speed, Some(0.0));
            *head = end;
        }
    }
    writer.tool_off();
    if params.air_assist {
        writer.air(false);
    }
}

/// Burns `dash_on` mm, moves `dash_off` mm with the laser off, along the
/// whole path, without stopping the head.
fn dashed(writer: &mut GcodeWriter, points: &[Point], params: &LineParams, power: f64) {
    let period = params.dash_on + params.dash_off;
    let mut phase = 0.0;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = a.distance(b);
        let mut travelled = 0.0;
        while travelled < length - 1e-9 {
            let burning = phase < params.dash_on;
            let to_boundary = if burning {
                params.dash_on - phase
            } else {
                period - phase
            };
            let step = to_boundary.min(length - travelled);
            travelled += step;
            phase = (phase + step) % period;
            let point = a.lerp(b, travelled / length);
            writer.linear(
                point.x,
                point.y,
                params.speed,
                Some(if burning { power } else { 0.0 }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashed_line_burns_the_requested_share() {
        let params = LineParams {
            dash_on: 2.0,
            dash_off: 1.0,
            ..LineParams::default()
        };
        let mut writer = GcodeWriter::new();
        let path = Path::new(vec![Point::new(0.0, 0.0), Point::new(30.0, 0.0)], false);
        let settings = LaserSettings {
            max_s: 1000.0,
            dynamic_power: true,
            scan_offset: 0.0,
        };
        emit_line(
            &mut writer,
            &mut Point::default(),
            vec![path],
            &params,
            &settings,
        );
        let program = mml_gcode::parse(&writer.finish());
        let burned: f64 = program
            .motions
            .iter()
            .filter(|motion| motion.is_cutting())
            .map(|motion| motion.length())
            .sum();
        assert!((burned - 20.0).abs() < 1e-6, "burned {burned}");
    }

    #[test]
    fn parts_keep_bridges_and_holes_are_cut_free() {
        let square = |x: f64, size: f64| {
            Path::new(
                vec![
                    Point::new(x, x),
                    Point::new(x + size, x),
                    Point::new(x + size, x + size),
                    Point::new(x, x + size),
                ],
                true,
            )
        };
        let params = LineParams {
            bridge_width: 1.0,
            bridge_count: 3,
            ..LineParams::default()
        };
        let settings = LaserSettings {
            max_s: 1000.0,
            dynamic_power: true,
            scan_offset: 0.0,
        };
        let mut writer = GcodeWriter::new();
        let mut head = Point::default();
        // A 40 mm part with a 10 mm hole.
        let part = vec![square(0.0, 40.0), square(15.0, 10.0)];
        emit_line_shapes(&mut writer, &mut head, vec![part], &params, &settings);
        let program = mml_gcode::parse(&writer.finish());
        let burned: f64 = program
            .motions
            .iter()
            .filter(|motion| motion.is_cutting())
            .map(|motion| motion.length())
            .sum();
        assert!(
            (burned - (160.0 - 3.0 + 40.0)).abs() < 1e-6,
            "burned {burned}"
        );
        // The head ends where the last cut ended, not back at its start.
        let last = program.motions.last().unwrap();
        assert!((head.x - last.to[0]).abs() < 1e-9 && (head.y - last.to[1]).abs() < 1e-9);
    }
}
