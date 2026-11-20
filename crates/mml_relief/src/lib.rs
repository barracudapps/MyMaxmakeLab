//! 3D relief carving: a mesh is placed in the stock, turned into a
//! height field seen from above, and machined with raster roughing and
//! finishing passes computed by drop-cutter (the tool is lowered until
//! it touches the surface anywhere under its footprint, so it never
//! gouges the model).

use mml_gcode::GcodeWriter;

mod indexed;
pub use indexed::{IndexedError, IndexedParams, indexed_gcode};

pub type Triangle = [[f32; 3]; 3];

#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("unsupported or damaged 3D file")]
    Format,
    #[error("the file contains no triangle")]
    Empty,
}

/// Reads binary or ASCII STL, or OBJ (triangles and polygons).
pub fn load_mesh(data: &[u8]) -> Result<Vec<Triangle>, MeshError> {
    let triangles = if is_binary_stl(data) {
        binary_stl(data)
    } else {
        let text = std::str::from_utf8(data).map_err(|_| MeshError::Format)?;
        if text.trim_start().starts_with("solid") && text.contains("vertex") {
            ascii_stl(text)
        } else {
            obj(text)
        }
    };
    if triangles.is_empty() {
        Err(MeshError::Empty)
    } else {
        Ok(triangles)
    }
}

fn is_binary_stl(data: &[u8]) -> bool {
    data.len() >= 84 && {
        let count = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
        data.len() == 84 + count * 50
    }
}

fn binary_stl(data: &[u8]) -> Vec<Triangle> {
    let count = u32::from_le_bytes([data[80], data[81], data[82], data[83]]) as usize;
    let value = |offset: usize| {
        f32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    };
    (0..count)
        .map(|index| {
            let base = 84 + index * 50 + 12;
            let vertex = |corner: usize| [0, 1, 2].map(|axis| value(base + corner * 12 + axis * 4));
            [vertex(0), vertex(1), vertex(2)]
        })
        .collect()
}

fn ascii_stl(text: &str) -> Vec<Triangle> {
    let vertices: Vec<[f32; 3]> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("vertex"))
        .filter_map(|rest| {
            let values: Vec<f32> = rest
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            (values.len() == 3).then(|| [values[0], values[1], values[2]])
        })
        .collect();
    vertices.as_chunks::<3>().0.to_vec()
}

fn obj(text: &str) -> Vec<Triangle> {
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut triangles = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let values: Vec<f32> = parts
                    .take(3)
                    .filter_map(|value| value.parse().ok())
                    .collect();
                if values.len() == 3 {
                    vertices.push([values[0], values[1], values[2]]);
                }
            }
            Some("f") => {
                let indices: Vec<usize> = parts
                    .filter_map(|part| part.split('/').next()?.parse::<i64>().ok())
                    .filter_map(|index| {
                        let resolved = if index < 0 {
                            vertices.len() as i64 + index
                        } else {
                            index - 1
                        };
                        (resolved >= 0 && (resolved as usize) < vertices.len())
                            .then_some(resolved as usize)
                    })
                    .collect();
                for corner in 1..indices.len().saturating_sub(1) {
                    triangles.push([
                        vertices[indices[0]],
                        vertices[indices[corner]],
                        vertices[indices[corner + 1]],
                    ]);
                }
            }
            _ => {}
        }
    }
    triangles
}

pub fn bounds(triangles: &[Triangle]) -> ([f32; 3], [f32; 3]) {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for vertex in triangles.iter().flatten() {
        for axis in 0..3 {
            low[axis] = low[axis].min(vertex[axis]);
            high[axis] = high[axis].max(vertex[axis]);
        }
    }
    (low, high)
}

/// Quarter turn around X, Y or Z, to choose which face looks up.
pub fn rotate_quarter(triangles: &mut [Triangle], axis: usize) {
    for vertex in triangles.iter_mut().flatten() {
        let [x, y, z] = *vertex;
        *vertex = match axis {
            0 => [x, -z, y],
            1 => [z, y, -x],
            _ => [-y, x, z],
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolShape {
    Flat,
    Ball,
    /// V-bit, included angle in degrees; `tool_diameter` is its widest
    /// cutting diameter.
    V {
        angle: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ReliefParams {
    /// Lower-left corner of the model in work coordinates, mm.
    pub origin: [f64; 2],
    /// Model width after scaling (X), mm; the ratio is kept.
    pub width: f64,
    /// Depth of the relief: the model is scaled in Z to fit it.
    pub depth: f64,
    /// Areas without model are machined down to the floor.
    pub background_floor: bool,
    pub tool: ToolShape,
    pub tool_diameter: f64,
    /// Fraction of the tool diameter between raster lines.
    pub stepover: f64,
    pub pass_depth: f64,
    /// Material left by roughing for the finishing pass, mm.
    pub allowance: f64,
    pub roughing: bool,
    pub finishing: bool,
    pub feed: f64,
    pub plunge: f64,
    pub spindle: f64,
    pub safe_z: f64,
}

impl Default for ReliefParams {
    fn default() -> Self {
        Self {
            origin: [0.0, 0.0],
            width: 80.0,
            depth: 5.0,
            background_floor: true,
            tool: ToolShape::Ball,
            tool_diameter: 3.175,
            stepover: 0.15,
            pass_depth: 1.0,
            allowance: 0.3,
            roughing: true,
            finishing: true,
            feed: 1200.0,
            plunge: 300.0,
            spindle: 12000.0,
            safe_z: 5.0,
        }
    }
}

/// Surface heights seen from above on a regular grid, work coordinates
/// (0 = stock top, negative below). `None` where there is no model.
pub struct HeightField {
    pub origin: [f64; 2],
    pub step: f64,
    pub columns: usize,
    pub rows: usize,
    pub heights: Vec<Option<f64>>,
}

impl HeightField {
    /// Field sampled from a function of the work position.
    pub fn from_fn(
        origin: [f64; 2],
        step: f64,
        columns: usize,
        rows: usize,
        height: impl Fn(f64, f64) -> Option<f64>,
    ) -> Self {
        let (columns, rows) = (columns.max(2), rows.max(2));
        let mut heights = Vec::with_capacity(columns * rows);
        for row in 0..rows {
            for column in 0..columns {
                heights.push(height(
                    origin[0] + column as f64 * step,
                    origin[1] + row as f64 * step,
                ));
            }
        }
        Self {
            origin,
            step,
            columns,
            rows,
            heights,
        }
    }

    pub fn width(&self) -> f64 {
        self.step * (self.columns - 1) as f64
    }

    pub fn height(&self) -> f64 {
        self.step * (self.rows - 1) as f64
    }

    fn at(&self, column: i64, row: i64) -> Option<f64> {
        if column < 0 || row < 0 || column >= self.columns as i64 || row >= self.rows as i64 {
            return None;
        }
        self.heights[row as usize * self.columns + column as usize]
    }
}

/// Rasterises the mesh from above (z-buffer keeping the highest hit)
/// after scaling it to `params.width` and `params.depth`.
pub fn height_field(triangles: &[Triangle], params: &ReliefParams, step: f64) -> HeightField {
    let (low, high) = bounds(triangles);
    let model_width = (high[0] - low[0]).max(1e-6) as f64;
    let model_height = (high[2] - low[2]).max(1e-6) as f64;
    let scale = params.width / model_width;
    let z_scale = params.depth / model_height;
    let to_work = |vertex: [f32; 3]| -> [f64; 3] {
        [
            params.origin[0] + (vertex[0] - low[0]) as f64 * scale,
            params.origin[1] + (vertex[1] - low[1]) as f64 * scale,
            (vertex[2] - high[2]) as f64 * z_scale,
        ]
    };
    let step = step.max(0.02);
    let columns = (params.width / step).ceil() as usize + 1;
    let rows = (((high[1] - low[1]) as f64 * scale) / step).ceil() as usize + 1;
    let mut heights = rasterize(
        triangles.iter().map(|triangle| triangle.map(to_work)),
        params.origin,
        step,
        columns,
        rows,
    );
    if params.background_floor {
        for slot in heights.iter_mut().filter(|slot| slot.is_none()) {
            *slot = Some(-params.depth);
        }
    }
    HeightField {
        origin: params.origin,
        step,
        columns,
        rows,
        heights,
    }
}

/// Z-buffer seen from above on a grid: the highest point of the
/// triangles over each node, `None` where there is none.
fn rasterize(
    triangles: impl Iterator<Item = [[f64; 3]; 3]>,
    origin: [f64; 2],
    step: f64,
    columns: usize,
    rows: usize,
) -> Vec<Option<f64>> {
    let mut heights: Vec<Option<f64>> = vec![None; columns * rows];
    for [a, b, c] in triangles {
        let min_x = a[0].min(b[0]).min(c[0]);
        let max_x = a[0].max(b[0]).max(c[0]);
        let min_y = a[1].min(b[1]).min(c[1]);
        let max_y = a[1].max(b[1]).max(c[1]);
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
        if area.abs() < 1e-12 {
            continue;
        }
        let first_column = ((min_x - origin[0]) / step).ceil().max(0.0) as usize;
        let last_column = (((max_x - origin[0]) / step).floor() as usize).min(columns - 1);
        let first_row = ((min_y - origin[1]) / step).ceil().max(0.0) as usize;
        let last_row = (((max_y - origin[1]) / step).floor() as usize).min(rows - 1);
        for row in first_row..=last_row {
            let y = origin[1] + row as f64 * step;
            for column in first_column..=last_column {
                let x = origin[0] + column as f64 * step;
                // Barycentric coordinates of the node in the triangle.
                let w0 = ((b[0] - x) * (c[1] - y) - (c[0] - x) * (b[1] - y)) / area;
                let w1 = ((c[0] - x) * (a[1] - y) - (a[0] - x) * (c[1] - y)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let slot = &mut heights[row * columns + column];
                if slot.is_none_or(|current| z > current) {
                    *slot = Some(z);
                }
            }
        }
    }
    heights
}

/// Lowest tool tip height at a node so that no part of the tool cuts
/// into the surface (drop-cutter).
struct DropCutter {
    kernel: Vec<(i64, i64, f64)>,
}

impl DropCutter {
    fn new(shape: ToolShape, radius: f64, step: f64) -> Self {
        let reach = (radius / step).ceil() as i64;
        let mut kernel = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let distance = ((dx * dx + dy * dy) as f64).sqrt() * step;
                if distance <= radius + 1e-9 {
                    let offset = match shape {
                        ToolShape::Flat => 0.0,
                        ToolShape::Ball => {
                            (radius * radius - distance * distance).max(0.0).sqrt() - radius
                        }
                        ToolShape::V { angle } => {
                            -distance / (angle.clamp(5.0, 175.0) / 2.0).to_radians().tan()
                        }
                    };
                    kernel.push((dx, dy, offset));
                }
            }
        }
        Self { kernel }
    }

    fn tip(&self, field: &HeightField, column: usize, row: usize) -> Option<f64> {
        self.kernel
            .iter()
            .filter_map(|(dx, dy, offset)| {
                field
                    .at(column as i64 + dx, row as i64 + dy)
                    .map(|z| z + offset)
            })
            .fold(None, |best: Option<f64>, z| {
                Some(best.map_or(z, |best| best.max(z)))
            })
    }
}

/// Raster roughing by levels then raster finishing along X.
pub fn relief_gcode(triangles: &[Triangle], params: &ReliefParams) -> String {
    let line_spacing = (params.tool_diameter * params.stepover.clamp(0.02, 0.9)).max(0.02);
    let step = (line_spacing / 2.0).clamp(0.05, 0.5);
    let field = height_field(triangles, params, step);
    let mut writer = GcodeWriter::new();
    writer.comment(&format!(
        "Relief {:.1} x {:.1} mm, depth {:.2} mm, {:?} tool {:.3} mm",
        field.width(),
        field.height(),
        params.depth,
        params.tool,
        params.tool_diameter
    ));
    writer.preamble();
    emit_field(&mut writer, &field, params);
    writer.rapid(params.origin[0], params.origin[1]);
    writer.program_end();
    writer.finish()
}

/// Grid step that suits the stepover of `params`.
pub fn field_step(params: &ReliefParams) -> f64 {
    let line_spacing = (params.tool_diameter * params.stepover.clamp(0.02, 0.9)).max(0.02);
    (line_spacing / 2.0).clamp(0.05, 0.5)
}

/// Machines a height field (roughing then finishing as configured) into
/// an existing program; the spindle is started and stopped here.
pub fn emit_field(writer: &mut GcodeWriter, field: &HeightField, params: &ReliefParams) {
    let radius = params.tool_diameter / 2.0;
    let line_spacing = (params.tool_diameter * params.stepover.clamp(0.02, 0.9)).max(0.02);
    let step = field.step;
    let cutter = DropCutter::new(params.tool, radius, step);
    let tips: Vec<Option<f64>> = (0..field.rows)
        .flat_map(|row| (0..field.columns).map(move |column| (column, row)))
        .map(|(column, row)| cutter.tip(field, column, row))
        .collect();
    let row_stride = ((line_spacing / step).round() as usize).max(1);
    writer.spindle_on(params.spindle);
    writer.rapid_z(params.safe_z);

    let raster = |writer: &mut GcodeWriter, floor: f64, offset: f64, stride: usize| {
        let mut forward = true;
        for row in (0..field.rows).step_by(stride) {
            let columns: Vec<usize> = if forward {
                (0..field.columns).collect()
            } else {
                (0..field.columns).rev().collect()
            };
            let mut cutting = false;
            for column in columns {
                let Some(tip) = tips[row * field.columns + column] else {
                    if cutting {
                        writer.rapid_z(params.safe_z);
                        cutting = false;
                    }
                    continue;
                };
                let z = (tip + offset).max(floor).min(0.0);
                let x = field.origin[0] + column as f64 * field.step;
                let y = field.origin[1] + row as f64 * field.step;
                if !cutting {
                    writer.rapid(x, y);
                    writer.linear_z(z, params.plunge);
                    cutting = true;
                } else {
                    writer.linear_xyz(x, y, z, params.feed);
                }
            }
            forward = !forward;
        }
        writer.rapid_z(params.safe_z);
    };

    if params.roughing {
        let lowest = tips.iter().flatten().copied().fold(0.0f64, f64::min) + params.allowance;
        let levels = ((-lowest) / params.pass_depth.max(0.05)).ceil() as usize;
        let rough_stride = ((params.tool_diameter * 0.45 / step).round() as usize).max(1);
        for level in 1..=levels {
            let floor = (-(level as f64) * params.pass_depth).max(lowest);
            writer.comment(&format!("Roughing level {floor:.2}"));
            raster(writer, floor, params.allowance, rough_stride);
        }
    }
    if params.finishing {
        writer.comment("Finishing");
        raster(writer, f64::NEG_INFINITY, 0.0, row_stride);
    }
    writer.tool_off();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Square pyramid 10 x 10 at the base, 5 high, apex at the centre.
    fn pyramid() -> Vec<Triangle> {
        let apex = [5.0, 5.0, 5.0];
        let corners = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
        ];
        (0..4)
            .map(|index| [corners[index], corners[(index + 1) % 4], apex])
            .collect()
    }

    #[test]
    fn stl_and_obj_load_the_same_triangles() {
        let obj_text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        assert_eq!(load_mesh(obj_text.as_bytes()).unwrap().len(), 2);
        let ascii = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid";
        assert_eq!(load_mesh(ascii.as_bytes()).unwrap().len(), 1);
    }

    #[test]
    fn height_field_puts_the_top_at_zero_and_scales_depth() {
        let params = ReliefParams {
            width: 20.0,
            depth: 2.0,
            background_floor: false,
            ..ReliefParams::default()
        };
        let field = height_field(&pyramid(), &params, 0.5);
        let center = field.at(20, 20).unwrap();
        let edge = field.at(0, 0).unwrap();
        assert!(center.abs() < 1e-6);
        assert!((edge + 2.0).abs() < 1e-6);
    }

    #[test]
    fn ball_tool_never_goes_below_the_surface() {
        let params = ReliefParams {
            width: 20.0,
            depth: 4.0,
            roughing: false,
            ..ReliefParams::default()
        };
        let gcode = relief_gcode(&pyramid(), &params);
        let program = mml_gcode::parse(&gcode);
        let field = height_field(&pyramid(), &params, 0.05);
        for motion in program.motions.iter().filter(|motion| motion.is_cutting()) {
            let column = ((motion.to[0] - field.origin[0]) / field.step).round() as i64;
            let row = ((motion.to[1] - field.origin[1]) / field.step).round() as i64;
            if let Some(surface) = field.at(column, row) {
                assert!(motion.to[2] >= surface - 0.05, "gouge at {:?}", motion.to);
            }
        }
        assert!(program.z_range.0 >= -4.0 - 1e-6);
    }

    #[test]
    fn v_bit_reaches_depth_only_where_the_groove_is_wide_enough() {
        // Groove 2 mm wide: with a 90° bit the bottom is 1 mm deep.
        let field = HeightField::from_fn([0.0, 0.0], 0.05, 81, 21, |x, _| {
            let distance = (1.0 - (x - 2.0).abs()).max(0.0);
            Some(-distance.min(3.0))
        });
        let params = ReliefParams {
            tool: ToolShape::V { angle: 90.0 },
            tool_diameter: 6.0,
            roughing: false,
            stepover: 0.1,
            ..ReliefParams::default()
        };
        let mut writer = GcodeWriter::new();
        emit_field(&mut writer, &field, &params);
        let program = mml_gcode::parse(&writer.finish());
        assert!(
            (program.z_range.0 + 1.0).abs() < 0.06,
            "deepest {}",
            program.z_range.0
        );
    }

    #[test]
    fn roughing_respects_the_pass_depth() {
        let params = ReliefParams {
            width: 20.0,
            depth: 3.0,
            finishing: false,
            pass_depth: 1.0,
            ..ReliefParams::default()
        };
        let gcode = relief_gcode(&pyramid(), &params);
        assert_eq!(gcode.matches("; Roughing level").count(), 3);
    }
}
