//! V-bit engraving: vectors are V-carved from a distance field (the
//! groove depth follows the distance to the outline, as a V-bit does),
//! pictures are engraved with a depth that follows their darkness.

use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Path, Point, Rect};
use mml_core::layer::VCarveParams;
use mml_gcode::GcodeWriter;
use mml_relief::{HeightField, ReliefParams, ToolShape, emit_field};

fn relief_params(params: &VCarveParams, safe_z: f64) -> ReliefParams {
    ReliefParams {
        tool: ToolShape::V {
            angle: params.angle,
        },
        tool_diameter: params.tool_diameter,
        stepover: (params.line_spacing / params.tool_diameter.max(0.01)).clamp(0.01, 0.9),
        depth: params.max_depth,
        roughing: false,
        finishing: true,
        feed: params.feed,
        plunge: params.plunge,
        spindle: params.spindle,
        safe_z,
        ..ReliefParams::default()
    }
}

/// Two-pass chamfer distance transform on a 5 × 5 neighbourhood
/// (steps of 1, √2 and √5 cells: within about 2 % of the true distance,
/// so round shapes stay round), in cells, from every inside cell to the
/// nearest outside cell.
pub(crate) fn distance_to_outside(inside: &[bool], columns: usize, rows: usize) -> Vec<f64> {
    let mut distance: Vec<f64> = inside
        .iter()
        .map(|&cell| if cell { f64::INFINITY } else { 0.0 })
        .collect();
    let (diagonal, knight) = (std::f64::consts::SQRT_2, 5f64.sqrt());
    // Neighbours already visited by a forward scan; the backward scan
    // uses the opposite ones.
    let forward: [(i64, i64, f64); 8] = [
        (-1, 0, 1.0),
        (0, -1, 1.0),
        (-1, -1, diagonal),
        (1, -1, diagonal),
        (-1, -2, knight),
        (1, -2, knight),
        (-2, -1, knight),
        (2, -1, knight),
    ];
    let (width, height) = (columns as i64, rows as i64);
    let mut pass = |order: &mut dyn Iterator<Item = (i64, i64)>, sign: i64| {
        for (column, row) in order {
            let here = (row * width + column) as usize;
            let mut best = distance[here];
            for (dx, dy, cost) in forward {
                let (x, y) = (column + sign * dx, row + sign * dy);
                if x >= 0 && y >= 0 && x < width && y < height {
                    best = best.min(distance[(y * width + x) as usize] + cost);
                }
            }
            distance[here] = best;
        }
    };
    pass(
        &mut (0..height).flat_map(|row| (0..width).map(move |column| (column, row))),
        1,
    );
    pass(
        &mut (0..height)
            .rev()
            .flat_map(|row| (0..width).rev().map(move |column| (column, row))),
        -1,
    );
    distance
}

pub fn emit_vcarve(
    writer: &mut GcodeWriter,
    shapes: &[&Shape],
    params: &VCarveParams,
    safe_z: f64,
) {
    let relief = relief_params(params, safe_z);
    let step = (params.line_spacing / 2.0).clamp(0.03, 0.5);
    let half_angle = (params.angle.clamp(5.0, 175.0) / 2.0).to_radians();
    let vectors: Vec<Vec<Path>> = shapes
        .iter()
        .filter(|shape| !shape.is_image())
        .map(|shape| shape.world_paths())
        .collect();
    if !vectors.is_empty() {
        let bounds = vectors
            .iter()
            .flatten()
            .fold(Rect::EMPTY, |rect, path| rect.union(path.bounds()));
        let margin = step * 2.0;
        let origin = [bounds.min.x - margin, bounds.min.y - margin];
        let columns = ((bounds.width() + 2.0 * margin) / step).ceil() as usize + 1;
        let rows = ((bounds.height() + 2.0 * margin) / step).ceil() as usize + 1;
        let mut inside = vec![false; columns * rows];
        for row in 0..rows {
            let y = origin[1] + row as f64 * step;
            for (start, end) in mml_laser::scan_intervals(&vectors, y) {
                let first = ((start - origin[0]) / step).ceil().max(0.0) as usize;
                let last = (((end - origin[0]) / step).floor() as usize).min(columns - 1);
                for cell in inside[row * columns..(row + 1) * columns]
                    .iter_mut()
                    .take(last + 1)
                    .skip(first)
                {
                    *cell = true;
                }
            }
        }
        let distance = distance_to_outside(&inside, columns, rows);
        let field = HeightField::from_fn(origin, step, columns, rows, |x, y| {
            let column = ((x - origin[0]) / step).round() as usize;
            let row = ((y - origin[1]) / step).round() as usize;
            let cells = distance[row.min(rows - 1) * columns + column.min(columns - 1)];
            Some(-(cells * step / half_angle.tan()).min(params.max_depth))
        });
        emit_field(writer, &field, &relief);
    }
    for shape in shapes {
        let ShapeKind::Image {
            image,
            width,
            height,
        } = &shape.kind
        else {
            continue;
        };
        let Some(inverse) = shape.transform.inverse() else {
            continue;
        };
        let bounds = shape.bounds();
        let origin = [bounds.min.x, bounds.min.y];
        let columns = (bounds.width() / step).ceil() as usize + 1;
        let rows = (bounds.height() / step).ceil() as usize + 1;
        let field = HeightField::from_fn(origin, step, columns, rows, |x, y| {
            let local = inverse.apply(Point::new(x, y));
            let u = (local.x / width + 0.5) * image.width as f64;
            let v = (0.5 - local.y / height) * image.height as f64;
            if u < 0.0 || v < 0.0 || u >= image.width as f64 || v >= image.height as f64 {
                return None;
            }
            let darkness = 1.0 - image.pixel(u as u32, v as u32) as f64 / 255.0;
            Some(-darkness * params.max_depth)
        });
        emit_field(writer, &field, &relief);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_grows_towards_the_middle_of_a_band() {
        // A band 5 cells wide.
        let columns = 7;
        let inside: Vec<bool> = (0..columns * 3)
            .map(|index| (1..6).contains(&(index % columns)))
            .collect();
        let distance = distance_to_outside(&inside, columns, 3);
        assert_eq!(distance[columns + 1], 1.0);
        assert_eq!(distance[columns + 3], 3.0);
        assert_eq!(distance[columns], 0.0);
    }
}
