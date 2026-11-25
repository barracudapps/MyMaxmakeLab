//! Relief carving of a layer: an outer contour is the carved area,
//! contours inside it stand up as rounded motifs, contours inside those
//! are carved again; pictures are carved deeper where they are darker.
//! The tool never leaves the shapes: around them the field is a wall.

use crate::vcarve::distance_to_outside;
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Path, Point, Rect};
use mml_core::layer::{CncReliefParams, ReliefTool};
use mml_gcode::GcodeWriter;
use mml_relief::{HeightField, ReliefParams, ToolShape, emit_field};

/// Height of the cells the tool must keep out of: high enough that even
/// the rim of the tool would stand above the stock top there, so the
/// drop-cutter leaves the whole footprint alone.
fn wall(params: &CncReliefParams) -> f64 {
    params.tool_diameter + 1.0
}

fn relief_params(params: &CncReliefParams, safe_z: f64) -> ReliefParams {
    ReliefParams {
        tool: match params.tool {
            ReliefTool::Ball => ToolShape::Ball,
            ReliefTool::Flat => ToolShape::Flat,
        },
        tool_diameter: params.tool_diameter,
        stepover: params.stepover.clamp(0.02, 0.9),
        depth: params.depth,
        pass_depth: params.pass_depth,
        allowance: params.allowance,
        roughing: params.roughing,
        finishing: true,
        feed: params.feed,
        plunge: params.plunge,
        spindle: params.spindle,
        safe_z,
        ..ReliefParams::default()
    }
}

/// Grid step for the field: half the distance between finishing lines.
fn grid_step(params: &CncReliefParams) -> f64 {
    let line_spacing = (params.tool_diameter * params.stepover.clamp(0.02, 0.9)).max(0.02);
    (line_spacing / 2.0).clamp(0.05, 0.5)
}

/// Rise of a motif edge at `distance` from the background: a quarter
/// circle `rounding` wide, then flat at 1.
pub fn edge_profile(distance: f64, rounding: f64) -> f64 {
    if rounding <= 0.0 || distance >= rounding {
        return 1.0;
    }
    let t = (distance / rounding).max(0.0);
    (1.0 - (1.0 - t) * (1.0 - t)).sqrt()
}

/// Height field of the vector shapes, `None` if there is nothing closed.
pub fn vector_field(paths: &[Path], params: &CncReliefParams) -> Option<HeightField> {
    let closed: Vec<&Path> = paths.iter().filter(|path| path.closed).collect();
    if closed.is_empty() {
        return None;
    }
    let step = grid_step(params);
    let bounds = closed
        .iter()
        .fold(Rect::EMPTY, |rect, path| rect.union(path.bounds()));
    let margin = params.tool_diameter / 2.0 + 2.0 * step;
    let origin = [bounds.min.x - margin, bounds.min.y - margin];
    let columns = ((bounds.width() + 2.0 * margin) / step).ceil() as usize + 1;
    let rows = ((bounds.height() + 2.0 * margin) / step).ceil() as usize + 1;
    // How many contours hold each cell: 0 outside, odd in the carved
    // background, even inside a motif.
    let mut count = vec![0u32; columns * rows];
    for path in closed {
        let single = [vec![path.clone()]];
        for row in 0..rows {
            let y = origin[1] + row as f64 * step;
            for (start, end) in mml_laser::scan_intervals(&single, y) {
                let first = ((start - origin[0]) / step).ceil().max(0.0) as usize;
                let last = ((end - origin[0]) / step).floor();
                if last < 0.0 {
                    continue;
                }
                let last = (last as usize).min(columns - 1);
                for cell in count[row * columns..(row + 1) * columns]
                    .iter_mut()
                    .take(last + 1)
                    .skip(first)
                {
                    *cell += 1;
                }
            }
        }
    }
    let motif: Vec<bool> = count
        .iter()
        .map(|&count| count >= 2 && count % 2 == 0)
        .collect();
    let distance = distance_to_outside(&motif, columns, rows);
    // Distance from the outer contour, for a dished background.
    let carved: Vec<bool> = count.iter().map(|&count| count > 0).collect();
    let from_edge = distance_to_outside(&carved, columns, rows);
    let heights = (0..columns * rows)
        .map(|cell| {
            Some(match count[cell] {
                0 => wall(params),
                count if count % 2 == 1 => {
                    // Quarter circle down from the contour: the cell
                    // centre is half a step in.
                    let depth = edge_profile((from_edge[cell] - 0.5).max(0.0) * step, params.dish);
                    -params.depth * depth
                }
                _ => {
                    -params.depth
                        + params.depth * edge_profile(distance[cell] * step, params.rounding)
                }
            })
        })
        .collect();
    Some(HeightField {
        origin,
        step,
        columns,
        rows,
        heights,
    })
}

/// Height field of a picture shape: black at the full depth, white at
/// the stock top.
fn image_field(shape: &Shape, params: &CncReliefParams) -> Option<HeightField> {
    let ShapeKind::Image {
        image,
        width,
        height,
    } = &shape.kind
    else {
        return None;
    };
    let inverse = shape.transform.inverse()?;
    let step = grid_step(params);
    let bounds = shape.bounds();
    let margin = params.tool_diameter / 2.0 + 2.0 * step;
    let origin = [bounds.min.x - margin, bounds.min.y - margin];
    let columns = ((bounds.width() + 2.0 * margin) / step).ceil() as usize + 1;
    let rows = ((bounds.height() + 2.0 * margin) / step).ceil() as usize + 1;
    Some(HeightField::from_fn(origin, step, columns, rows, |x, y| {
        let local = inverse.apply(Point::new(x, y));
        let u = (local.x / width + 0.5) * image.width as f64;
        let v = (0.5 - local.y / height) * image.height as f64;
        if u < 0.0 || v < 0.0 || u >= image.width as f64 || v >= image.height as f64 {
            return Some(wall(params));
        }
        let darkness = 1.0 - image.pixel(u as u32, v as u32) as f64 / 255.0;
        Some(-darkness * params.depth)
    }))
}

pub fn emit_relief(
    writer: &mut GcodeWriter,
    shapes: &[&Shape],
    params: &CncReliefParams,
    safe_z: f64,
) {
    let relief = relief_params(params, safe_z);
    let paths: Vec<Path> = shapes
        .iter()
        .filter(|shape| !shape.is_image())
        .flat_map(|shape| shape.world_paths())
        .collect();
    if let Some(field) = vector_field(&paths, params) {
        emit_field(writer, &field, &relief);
    }
    for shape in shapes.iter().filter(|shape| shape.is_image()) {
        if let Some(field) = image_field(shape, params) {
            emit_field(writer, &field, &relief);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(x: f64, y: f64, radius: f64) -> Path {
        let points = (0..96)
            .map(|index| {
                let angle = index as f64 / 96.0 * std::f64::consts::TAU;
                Point::new(x + radius * angle.cos(), y + radius * angle.sin())
            })
            .collect();
        Path::new(points, true)
    }

    fn height_at(field: &HeightField, x: f64, y: f64) -> Option<f64> {
        let column = ((x - field.origin[0]) / field.step).round() as usize;
        let row = ((y - field.origin[1]) / field.step).round() as usize;
        field.heights[row * field.columns + column]
    }

    #[test]
    fn edge_rises_in_a_quarter_circle() {
        assert_eq!(edge_profile(0.0, 2.0), 0.0);
        assert!((edge_profile(1.0, 2.0) - 0.75f64.sqrt()).abs() < 1e-9);
        assert_eq!(edge_profile(3.0, 2.0), 1.0);
        assert_eq!(edge_profile(0.0, 0.0), 1.0);
    }

    #[test]
    fn nested_contours_alternate_background_and_motif() {
        let params = CncReliefParams {
            depth: 3.0,
            rounding: 2.0,
            ..CncReliefParams::default()
        };
        // Area, a ring-shaped motif, and its hole.
        let paths = [
            circle(0.0, 0.0, 30.0),
            circle(0.0, 0.0, 15.0),
            circle(0.0, 0.0, 5.0),
        ];
        let field = vector_field(&paths, &params).unwrap();
        assert_eq!(height_at(&field, 0.0, 0.0), Some(-3.0));
        assert_eq!(height_at(&field, 22.0, 0.0), Some(-3.0));
        assert_eq!(height_at(&field, 31.0, 0.0), Some(wall(&params)));
        // The middle of the ring is 5 mm from its edges: at the top.
        assert!(height_at(&field, 10.0, 0.0).unwrap().abs() < 1e-9);
        // Close to its edge it is still rising.
        let near = height_at(&field, 14.8, 0.0).unwrap();
        assert!(near < -0.5 && near > -3.0, "{near}");
    }

    #[test]
    fn a_dish_goes_down_from_its_edge_to_the_middle() {
        let params = CncReliefParams {
            depth: 8.0,
            dish: 12.0,
            ..CncReliefParams::default()
        };
        let field = vector_field(&[circle(0.0, 0.0, 12.0)], &params).unwrap();
        let middle = height_at(&field, 0.0, 0.0).unwrap();
        assert!((middle + 8.0).abs() < 0.3, "{middle}");
        // Halfway out it is already deep (quarter circle), near the rim
        // it is shallow.
        let half = height_at(&field, 6.0, 0.0).unwrap();
        assert!(half < -6.0 && half > -8.0, "{half}");
        let rim = height_at(&field, 11.5, 0.0).unwrap();
        assert!(rim > -3.5, "{rim}");
    }

    #[test]
    fn the_tool_stays_inside_the_area() {
        let params = CncReliefParams {
            depth: 2.0,
            rounding: 2.0,
            roughing: true,
            ..CncReliefParams::default()
        };
        let paths = [circle(50.0, 50.0, 20.0), circle(50.0, 50.0, 8.0)];
        let field = vector_field(&paths, &params).unwrap();
        let mut writer = GcodeWriter::new();
        emit_field(&mut writer, &field, &relief_params(&params, 5.0));
        let program = mml_gcode::parse(&writer.finish());
        // Cells are a grid step wide: the edge is found to a step.
        let reach = 20.0 - params.tool_diameter / 2.0 + field.step;
        for motion in program.motions.iter().filter(|motion| motion.is_cutting()) {
            let offset = ((motion.to[0] - 50.0).powi(2) + (motion.to[1] - 50.0).powi(2)).sqrt();
            assert!(offset <= reach, "outside at {:?}", motion.to);
        }
        assert!(program.z_range.0 >= -2.0 - 1e-6);
        assert!(program.z_range.0 <= -1.9);
    }
}
