use crate::LaserSettings;
use mml_core::geometry::{Path, Point};
use mml_core::layer::LineParams;
use mml_core::ordering::order_paths;
use mml_gcode::GcodeWriter;

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
            for point in &path.points[1..] {
                writer.linear(point.x, point.y, params.speed, Some(power));
            }
            if path.closed {
                writer.linear(first.x, first.y, params.speed, Some(power));
            }
            writer.linear(first.x, first.y, params.speed, Some(0.0));
            *head = if path.closed {
                first
            } else {
                *path.points.last().unwrap()
            };
        }
    }
    writer.tool_off();
    if params.air_assist {
        writer.air(false);
    }
}
