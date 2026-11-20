//! Indexed 4-axis machining (3+1): round stock held on an A axis along
//! X, machined from several angles in turn. At each angle the part is
//! a plain 3-axis relief seen from above; the stock only turns between
//! faces, with the tool retracted.
//!
//! Work coordinates: Y0 on the rotation axis, Z0 on the top of the
//! stock (touch it with the probe), so the axis is at Z = -diameter/2.
//! Positive A follows the right-hand rule around +X.

use crate::{HeightField, ReliefParams, Triangle, bounds, emit_field, field_step};
use mml_gcode::GcodeWriter;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndexedParams {
    pub stock_diameter: f64,
    /// Number of evenly spaced angles, 2 (front and back) or more.
    pub faces: usize,
}

impl Default for IndexedParams {
    fn default() -> Self {
        Self {
            stock_diameter: 40.0,
            faces: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IndexedError {
    #[error("the model needs a stock of at least {needed:.1} mm in diameter")]
    StockTooSmall { needed: f64 },
}

/// Model in work coordinates, centred on the axis, `params.width` long
/// from `params.origin[0]`.
fn place(triangles: &[Triangle], params: &ReliefParams) -> Vec<[[f64; 3]; 3]> {
    let (low, high) = bounds(triangles);
    let scale = params.width / ((high[0] - low[0]) as f64).max(1e-6);
    let centre_y = (low[1] + high[1]) as f64 / 2.0;
    let centre_z = (low[2] + high[2]) as f64 / 2.0;
    triangles
        .iter()
        .map(|triangle| {
            triangle.map(|vertex| {
                [
                    params.origin[0] + (vertex[0] - low[0]) as f64 * scale,
                    (vertex[1] as f64 - centre_y) * scale,
                    (vertex[2] as f64 - centre_z) * scale,
                ]
            })
        })
        .collect()
}

/// Height field of the part turned by `angle` degrees, relative to the
/// stock top. Nothing is cut below the axis: the opposite face does it.
fn face_field(
    model: &[[[f64; 3]; 3]],
    params: &ReliefParams,
    radius: f64,
    angle: f64,
    step: f64,
) -> HeightField {
    let (sin, cos) = angle.to_radians().sin_cos();
    let turned = model
        .iter()
        .map(|triangle| triangle.map(|[x, y, z]| [x, y * cos - z * sin, y * sin + z * cos]));
    let margin = params.tool_diameter / 2.0 + step;
    let origin = [params.origin[0] - margin, -radius - margin];
    let columns = ((params.width + 2.0 * margin) / step).ceil() as usize + 1;
    let rows = ((2.0 * (radius + margin)) / step).ceil() as usize + 1;
    let heights = crate::rasterize(turned, origin, step, columns, rows)
        .into_iter()
        .map(|height| Some(height.unwrap_or(0.0).max(0.0) - radius))
        .collect();
    HeightField {
        origin,
        step,
        columns,
        rows,
        heights,
    }
}

pub fn indexed_gcode(
    triangles: &[Triangle],
    params: &ReliefParams,
    indexed: &IndexedParams,
) -> Result<String, IndexedError> {
    let model = place(triangles, params);
    let needed = 2.0
        * model
            .iter()
            .flatten()
            .map(|[_, y, z]| y.hypot(*z))
            .fold(0.0, f64::max);
    if needed > indexed.stock_diameter + 1e-6 {
        return Err(IndexedError::StockTooSmall { needed });
    }
    let radius = indexed.stock_diameter / 2.0;
    let faces = indexed.faces.max(2);
    let step = field_step(params);
    let mut writer = GcodeWriter::new();
    writer.comment(&format!(
        "Indexed 4-axis, {faces} faces, stock {:.1} mm, part {:.1} mm long",
        indexed.stock_diameter, params.width
    ));
    writer.comment("Z0 on top of the stock, Y0 on the A axis");
    writer.preamble();
    for face in 0..faces {
        let angle = 360.0 * face as f64 / faces as f64;
        writer.rapid_z(params.safe_z);
        writer.comment(&format!("Face {} at A{angle:.1}", face + 1));
        writer.raw(&format!("G0 A{angle:.3}"));
        let field = face_field(&model, params, radius, angle, step);
        emit_field(&mut writer, &field, params);
    }
    writer.rapid_z(params.safe_z);
    writer.raw("G0 A0");
    writer.program_end();
    Ok(writer.finish())
}

/// Deepest point the finishing pass reaches on a face, for tests.
#[cfg(test)]
fn deepest(field: &HeightField, params: &ReliefParams) -> f64 {
    let cutter = crate::DropCutter::new(params.tool, params.tool_diameter / 2.0, field.step);
    (0..field.rows)
        .flat_map(|row| (0..field.columns).map(move |column| (column, row)))
        .filter_map(|(column, row)| cutter.tip(field, column, row))
        .fold(0.0, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Box 40 long (X), 10 wide (Y), 6 high (Z).
    fn bar() -> Vec<Triangle> {
        let (x, y, z) = (40.0f32, 10.0f32, 6.0f32);
        let v = |i: usize| -> [f32; 3] {
            [
                if i & 1 == 0 { 0.0 } else { x },
                if i & 2 == 0 { 0.0 } else { y },
                if i & 4 == 0 { 0.0 } else { z },
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

    fn params() -> ReliefParams {
        ReliefParams {
            width: 40.0,
            tool: crate::ToolShape::Flat,
            tool_diameter: 3.0,
            stepover: 0.4,
            pass_depth: 2.0,
            roughing: false,
            ..ReliefParams::default()
        }
    }

    #[test]
    fn each_face_sees_the_part_turned_by_its_angle() {
        let params = params();
        let model = place(&bar(), &params);
        let radius = 10.0;
        let step = field_step(&params);
        // Top face: 3 mm above the axis, so 7 mm below the stock top.
        let top = face_field(&model, &params, radius, 0.0, step);
        assert!((deepest(&top, &params) + 10.0).abs() < 1e-6);
        let centre = top.heights[(top.rows / 2) * top.columns + top.columns / 2].unwrap();
        assert!((centre + 7.0).abs() < 0.01, "{centre}");
        // A quarter turn shows the 10 mm side: 5 mm above the axis.
        let side = face_field(&model, &params, radius, 90.0, step);
        let centre = side.heights[(side.rows / 2) * side.columns + side.columns / 2].unwrap();
        assert!((centre + 5.0).abs() < 0.01, "{centre}");
    }

    #[test]
    fn the_program_turns_between_faces_with_the_tool_up() {
        let gcode = indexed_gcode(
            &bar(),
            &params(),
            &IndexedParams {
                stock_diameter: 20.0,
                faces: 4,
            },
        )
        .unwrap();
        for angle in ["A0.000", "A90.000", "A180.000", "A270.000"] {
            assert!(gcode.contains(&format!("G0 {angle}")), "{angle}");
        }
        let program = mml_gcode::parse(&gcode);
        assert!(program.uses_rotary);
        assert!(program.z_range.0 >= -10.0 - 1e-6, "never below the axis");
        // Every A move happens at the safe height.
        let mut z = 0.0;
        for line in gcode.lines() {
            if let Some(index) = line.find('Z') {
                let value: String = line[index + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | '.'))
                    .collect();
                z = value.parse().unwrap();
            }
            if line.starts_with("G0 A") {
                assert!(z >= params().safe_z - 1e-6, "turning at Z{z}");
            }
        }
    }

    #[test]
    fn a_model_larger_than_the_stock_is_refused() {
        let error = indexed_gcode(
            &bar(),
            &params(),
            &IndexedParams {
                stock_diameter: 8.0,
                faces: 2,
            },
        )
        .unwrap_err();
        let IndexedError::StockTooSmall { needed } = error;
        assert!((needed - (10.0f64.hypot(6.0))).abs() < 0.01);
    }
}
