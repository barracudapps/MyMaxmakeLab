//! Mold mode: the imprint of a 3D model sunk into the stock, machined
//! from above. The model keeps its proportions, can be placed anywhere
//! on the stock and sunk to any depth (half its height for a two-part
//! mold). An offset enlarges the cavity evenly in every direction
//! (clearance, shrinkage, a coating), and small islands of stock left
//! standing inside the cavity can be removed automatically.

use crate::{HeightField, ReliefParams, Triangle, bounds, emit_field, field_step};
use mml_gcode::GcodeWriter;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoldParams {
    /// Depth of the lowest point of the model below the stock top, mm.
    pub sink: f64,
    /// Clearance added all around the imprint, mm.
    pub offset: f64,
    /// Islands of stock enclosed by the cavity and smaller than this
    /// area (mm²) are cut away; 0 keeps them all.
    pub min_island: f64,
    /// Mirror the model (Y), for the second half of a mold.
    pub mirror: bool,
}

impl Default for MoldParams {
    fn default() -> Self {
        Self {
            sink: 10.0,
            offset: 0.0,
            min_island: 20.0,
            mirror: false,
        }
    }
}

/// Stock left untouched, slightly above the top so the drop-cutter
/// never lets the tool cut into it.
const STOCK: f64 = 1e-3;

/// Cavity floor seen from above (heights relative to the stock top),
/// `STOCK` where nothing is cut.
pub fn mold_field(triangles: &[Triangle], params: &ReliefParams, mold: &MoldParams) -> HeightField {
    let (low, high) = bounds(triangles);
    let scale = params.width / ((high[0] - low[0]) as f64).max(1e-6);
    let depth = (high[1] - low[1]) as f64 * scale;
    let step = field_step(params);
    let margin = mold.offset.max(0.0) + params.tool_diameter / 2.0 + 2.0 * step;
    let origin = [params.origin[0] - margin, params.origin[1] - margin];
    let columns = ((params.width + 2.0 * margin) / step).ceil() as usize + 1;
    let rows = ((depth + 2.0 * margin) / step).ceil() as usize + 1;
    // Lowest point of the model over each node: rasterise -z.
    let placed = triangles.iter().map(|triangle| {
        triangle.map(|vertex| {
            let y = (vertex[1] - low[1]) as f64 * scale;
            [
                params.origin[0] + (vertex[0] - low[0]) as f64 * scale,
                params.origin[1] + if mold.mirror { depth - y } else { y },
                -((vertex[2] - low[2]) as f64 * scale - mold.sink),
            ]
        })
    });
    let floor: Vec<Option<f64>> = crate::rasterize(placed, origin, step, columns, rows)
        .into_iter()
        .map(|negated| negated.map(|value| -value).filter(|z| *z < 0.0))
        .collect();
    let mut floor = dilate(&floor, columns, rows, step, mold.offset);
    if mold.min_island > 0.0 {
        remove_islands(&mut floor, columns, rows, mold.min_island / (step * step));
    }
    HeightField {
        origin,
        step,
        columns,
        rows,
        heights: floor
            .into_iter()
            .map(|z| Some(z.unwrap_or(STOCK)))
            .collect(),
    }
}

/// Offset of the cavity by a ball of `radius`: the floor goes down by
/// the radius and the walls move out by it, a uniform clearance.
fn dilate(
    floor: &[Option<f64>],
    columns: usize,
    rows: usize,
    step: f64,
    radius: f64,
) -> Vec<Option<f64>> {
    if radius <= 0.0 {
        return floor.to_vec();
    }
    let reach = (radius / step).ceil() as i64;
    let mut kernel = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let distance = ((dx * dx + dy * dy) as f64).sqrt() * step;
            if distance <= radius {
                kernel.push((dx, dy, (radius * radius - distance * distance).sqrt()));
            }
        }
    }
    (0..rows)
        .flat_map(|row| (0..columns).map(move |column| (column, row)))
        .map(|(column, row)| {
            kernel
                .iter()
                .filter_map(|&(dx, dy, lift)| {
                    let (x, y) = (column as i64 + dx, row as i64 + dy);
                    if x < 0 || y < 0 || x >= columns as i64 || y >= rows as i64 {
                        return None;
                    }
                    floor[y as usize * columns + x as usize].map(|z| z - lift)
                })
                .filter(|z| *z < 0.0)
                .fold(None, |best: Option<f64>, z| {
                    Some(best.map_or(z, |best| best.min(z)))
                })
        })
        .collect()
}

/// Cuts down the enclosed islands of stock smaller than `max_cells`
/// to the shallowest floor around them.
fn remove_islands(floor: &mut [Option<f64>], columns: usize, rows: usize, max_cells: f64) {
    let mut seen = vec![false; floor.len()];
    let mut stack = Vec::new();
    for start in 0..floor.len() {
        if floor[start].is_some() || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut cells = Vec::new();
        let mut touches_border = false;
        let mut rim = f64::NEG_INFINITY;
        while let Some(index) = stack.pop() {
            cells.push(index);
            let (x, y) = (index % columns, index / columns);
            if x == 0 || y == 0 || x + 1 == columns || y + 1 == rows {
                touches_border = true;
            }
            let neighbours = [
                (x > 0).then(|| index - 1),
                (x + 1 < columns).then(|| index + 1),
                (y > 0).then(|| index - columns),
                (y + 1 < rows).then(|| index + columns),
            ];
            for next in neighbours.into_iter().flatten() {
                match floor[next] {
                    Some(z) => rim = rim.max(z),
                    None if !seen[next] => {
                        seen[next] = true;
                        stack.push(next);
                    }
                    None => {}
                }
            }
        }
        if !touches_border && (cells.len() as f64) < max_cells && rim.is_finite() {
            for index in cells {
                floor[index] = Some(rim);
            }
        }
    }
}

pub fn mold_gcode(triangles: &[Triangle], params: &ReliefParams, mold: &MoldParams) -> String {
    let field = mold_field(triangles, params, mold);
    let mut writer = GcodeWriter::new();
    writer.comment(&format!(
        "Mold imprint, model {:.1} mm wide sunk {:.2} mm, offset {:.2} mm",
        params.width, mold.sink, mold.offset
    ));
    writer.comment("Z0 on top of the stock");
    writer.preamble();
    emit_field(&mut writer, &field, params);
    writer.rapid_z(params.safe_z);
    writer.program_end();
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cuboid(min: [f32; 3], max: [f32; 3]) -> Vec<Triangle> {
        let v = |i: usize| -> [f32; 3] {
            [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ]
        };
        let quads = [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ];
        quads
            .iter()
            .flat_map(|q| [[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]])
            .collect()
    }

    fn params(width: f64) -> ReliefParams {
        ReliefParams {
            origin: [20.0, 30.0],
            width,
            tool: crate::ToolShape::Flat,
            tool_diameter: 2.0,
            stepover: 0.4,
            pass_depth: 2.0,
            ..ReliefParams::default()
        }
    }

    fn floor_at(field: &HeightField, x: f64, y: f64) -> f64 {
        let column = ((x - field.origin[0]) / field.step).round() as usize;
        let row = ((y - field.origin[1]) / field.step).round() as usize;
        field.heights[row * field.columns + column].unwrap()
    }

    #[test]
    fn the_imprint_sits_where_placed_and_grows_with_the_offset() {
        let block = cuboid([0.0, 0.0, 0.0], [10.0, 10.0, 8.0]);
        let mold = MoldParams {
            sink: 5.0,
            offset: 0.0,
            ..MoldParams::default()
        };
        let field = mold_field(&block, &params(10.0), &mold);
        assert!((floor_at(&field, 25.0, 35.0) + 5.0).abs() < 1e-6);
        assert_eq!(floor_at(&field, 18.0, 35.0), STOCK, "outside the imprint");
        let offset = MoldParams {
            offset: 1.0,
            ..mold
        };
        let field = mold_field(&block, &params(10.0), &offset);
        assert!(
            (floor_at(&field, 25.0, 35.0) + 6.0).abs() < 1e-6,
            "deeper by the offset"
        );
        assert!(floor_at(&field, 19.6, 35.0) < -5.0, "wider by the offset");
        assert_eq!(floor_at(&field, 18.6, 35.0), STOCK);
    }

    #[test]
    fn small_enclosed_islands_are_removed_on_request() {
        // A square frame: 20 mm outside, 3 mm hole in the middle.
        let mut frame = cuboid([0.0, 0.0, 0.0], [20.0, 8.5, 6.0]);
        frame.extend(cuboid([0.0, 11.5, 0.0], [20.0, 20.0, 6.0]));
        frame.extend(cuboid([0.0, 8.5, 0.0], [8.5, 11.5, 6.0]));
        frame.extend(cuboid([11.5, 8.5, 0.0], [20.0, 11.5, 6.0]));
        let keep = MoldParams {
            sink: 4.0,
            min_island: 0.0,
            ..MoldParams::default()
        };
        let field = mold_field(&frame, &params(20.0), &keep);
        assert_eq!(floor_at(&field, 30.0, 40.0), STOCK, "island kept");
        let remove = MoldParams {
            min_island: 20.0,
            ..keep
        };
        let field = mold_field(&frame, &params(20.0), &remove);
        assert!(
            (floor_at(&field, 30.0, 40.0) + 4.0).abs() < 1e-6,
            "island cut to the floor"
        );
        let gcode = mold_gcode(&frame, &params(20.0), &remove);
        let program = mml_gcode::parse(&gcode);
        assert!((program.z_range.0 + 4.0).abs() < 1e-6);
        // Nothing cut outside the imprint.
        let cut = program.cut_bounds();
        assert!(
            cut.min.x >= 20.0 - 1e-6 && cut.max.x <= 40.0 + 1e-6,
            "{cut:?}"
        );
    }
}
