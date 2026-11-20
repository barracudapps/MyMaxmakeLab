//! 4-axis machining of a whole part on round stock held on an A axis
//! along X.
//!
//! - Indexed (3+1): at each angle the part is a plain 3-axis relief
//!   seen from above; the stock only turns between faces, tool up.
//! - Continuous finish: the stock turns while cutting, the tool
//!   follows a helix around the part, always pointing at the axis.
//!   The tool axis stays perpendicular to the A axis, so no kinematic
//!   model of the machine is needed. Moves mixing millimetres and
//!   degrees are fed in inverse time (G93).
//!
//! Work coordinates: Y0 on the rotation axis, Z0 on the top of the
//! stock (touch it with the probe), so the axis is at Z = -diameter/2.
//! Positive A follows the right-hand rule around +X.
//!
//! A stem along the axis is never cut: without it a part that tapers
//! at an end (a sphere, a point) would come loose from the chuck in
//! the middle of the job. The operator cuts it at the end.

use crate::{DropCutter, HeightField, ReliefParams, Triangle, bounds, emit_field, field_step};
use mml_gcode::GcodeWriter;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndexedParams {
    pub stock_diameter: f64,
    /// Number of evenly spaced angles, 2 (front and back) or more.
    pub faces: usize,
    /// Diameter of the holding stem kept along the axis, mm.
    pub stem_diameter: f64,
    /// Finish with a continuous helix instead of finishing each face.
    pub continuous_finish: bool,
}

impl Default for IndexedParams {
    fn default() -> Self {
        Self {
            stock_diameter: 40.0,
            faces: 4,
            stem_diameter: 6.0,
            continuous_finish: true,
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

struct Setup<'a> {
    model: &'a [[[f64; 3]; 3]],
    params: &'a ReliefParams,
    radius: f64,
    stem_radius: f64,
    step: f64,
}

impl Setup<'_> {
    fn margin(&self) -> f64 {
        self.params.tool_diameter / 2.0 + self.step
    }

    /// Height field of the part turned by `angle` degrees, relative to
    /// the stock top, over `|y| <= half_width`. Nothing is cut below
    /// the axis (the opposite side does it) nor into the stem.
    fn field(&self, angle: f64, half_width: f64) -> HeightField {
        let (sin, cos) = angle.to_radians().sin_cos();
        let turned = self
            .model
            .iter()
            .map(|triangle| triangle.map(|[x, y, z]| [x, y * cos - z * sin, y * sin + z * cos]));
        let margin = self.margin();
        let origin = [self.params.origin[0] - margin, -half_width];
        let columns = ((self.params.width + 2.0 * margin) / self.step).ceil() as usize + 1;
        let rows = ((2.0 * half_width) / self.step).round() as usize + 1;
        let heights = crate::rasterize(turned, origin, self.step, columns, rows)
            .into_iter()
            .enumerate()
            .map(|(index, height)| {
                let y = origin[1] + (index / columns) as f64 * self.step;
                let stem = (self.stem_radius.powi(2) - y * y).max(0.0).sqrt();
                Some(height.unwrap_or(0.0).max(stem) - self.radius)
            })
            .collect();
        HeightField {
            origin,
            step: self.step,
            columns,
            rows,
            heights,
        }
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
    let setup = Setup {
        model: &model,
        params,
        radius,
        stem_radius: (indexed.stem_diameter / 2.0).clamp(0.0, radius),
        step: field_step(params),
    };
    let faces = indexed.faces.max(2);
    let continuous = indexed.continuous_finish && params.finishing;
    let face_params = ReliefParams {
        finishing: params.finishing && !continuous,
        ..*params
    };
    let mut writer = GcodeWriter::new();
    writer.comment(&format!(
        "4-axis, {faces} faces{}, part {:.1} mm long",
        if continuous {
            " + continuous finish"
        } else {
            ""
        },
        params.width
    ));
    writer.comment(&format!(
        "Rotary stock diameter {:.3} mm",
        indexed.stock_diameter
    ));
    writer.comment("Z0 on top of the stock, Y0 on the A axis");
    writer.preamble();
    if face_params.roughing || face_params.finishing {
        for face in 0..faces {
            let angle = 360.0 * face as f64 / faces as f64;
            writer.rapid_z(params.safe_z);
            writer.comment(&format!("Face {} at A{angle:.1}", face + 1));
            writer.raw(&format!("G0 A{angle:.3}"));
            let field = setup.field(angle, radius + setup.margin());
            emit_field(&mut writer, &field, &face_params);
        }
    }
    writer.rapid_z(params.safe_z);
    writer.raw("G0 A0");
    if continuous {
        continuous_finish(&mut writer, &setup);
    }
    writer.program_end();
    Ok(writer.finish())
}

/// Helix around the part: X advances by the stepover each turn while A
/// turns continuously. Tool tips come from a drop-cutter on a thin band
/// of the part under the tool, computed for every angle.
fn continuous_finish(writer: &mut GcodeWriter, setup: &Setup) {
    let params = setup.params;
    let radius = setup.radius;
    let step = setup.step;
    let tool_radius = params.tool_diameter / 2.0;
    let stepover = (params.tool_diameter * params.stepover.clamp(0.02, 0.9)).max(0.02);
    let angles = ((std::f64::consts::TAU * radius / step).ceil() as usize).clamp(36, 3600);
    let cutter = DropCutter::new(params.tool, tool_radius, step);
    let band = (tool_radius / step).ceil() * step;
    // tips[angle][column]
    let tips: Vec<Vec<f64>> = (0..angles)
        .map(|index| {
            let field = setup.field(360.0 * index as f64 / angles as f64, band);
            let centre = field.rows / 2;
            (0..field.columns)
                .map(|column| cutter.tip(&field, column, centre).unwrap_or(0.0).min(0.0))
                .collect()
        })
        .collect();
    let columns = tips[0].len();
    let x0 = params.origin[0] - setup.margin();
    let tip_at = |x: f64, angle_index: usize| {
        let position = ((x - x0) / step).clamp(0.0, (columns - 1) as f64);
        let (low, fraction) = (position.floor() as usize, position.fract());
        let row = &tips[angle_index % angles];
        let high = (low + 1).min(columns - 1);
        row[low] * (1.0 - fraction) + row[high] * fraction
    };
    let start_x = params.origin[0];
    let turns = (params.width / stepover).ceil().max(1.0) as usize;
    writer.comment(&format!("Continuous finish, {turns} turns"));
    writer.spindle_on(params.spindle);
    writer.rapid(start_x, 0.0);
    writer.linear_z(tip_at(start_x, 0), params.plunge);
    let mut text = String::from("G93\n");
    let mut previous = [start_x, 0.0, tip_at(start_x, 0)];
    for sample in 1..=turns * angles {
        let angle = 360.0 * sample as f64 / angles as f64;
        let x = start_x + stepover * sample as f64 / angles as f64;
        let z = tip_at(x, sample);
        // Distance travelled on the part surface, for the feed.
        let surface_radius = (radius + (z + previous[2]) / 2.0).max(1.0);
        let arc = surface_radius * (angle - previous[1]).to_radians();
        let length = (x - previous[0]).hypot(arc).hypot(z - previous[2]);
        let inverse_minutes = params.feed / length.max(1e-3);
        let _ = writeln!(text, "G1 X{x:.3} Z{z:.3} A{angle:.3} F{inverse_minutes:.1}");
        previous = [x, angle, z];
    }
    text.push_str("G94");
    writer.raw(&text);
    writer.rapid_z(params.safe_z);
    // Whole turns: the stock is back in its starting orientation.
    writer.raw("G92 A0");
    writer.tool_off();
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

    fn setup<'a>(model: &'a [[[f64; 3]; 3]], params: &'a ReliefParams, stem: f64) -> Setup<'a> {
        Setup {
            model,
            params,
            radius: 10.0,
            stem_radius: stem,
            step: field_step(params),
        }
    }

    fn deepest(field: &HeightField, params: &ReliefParams) -> f64 {
        let cutter = DropCutter::new(params.tool, params.tool_diameter / 2.0, field.step);
        (0..field.rows)
            .flat_map(|row| (0..field.columns).map(move |column| (column, row)))
            .filter_map(|(column, row)| cutter.tip(field, column, row))
            .fold(0.0, f64::min)
    }

    /// Cylinder of radius 4 along X, 30 long, 48 sides.
    fn cylinder() -> Vec<Triangle> {
        let point = |x: f32, index: usize| {
            let angle = index as f32 / 48.0 * std::f32::consts::TAU;
            [x, 4.0 * angle.cos(), 4.0 * angle.sin()]
        };
        (0..48)
            .flat_map(|index| {
                [
                    [
                        point(0.0, index),
                        point(30.0, index),
                        point(30.0, index + 1),
                    ],
                    [
                        point(0.0, index),
                        point(30.0, index + 1),
                        point(0.0, index + 1),
                    ],
                ]
            })
            .collect()
    }

    #[test]
    fn each_face_sees_the_part_turned_by_its_angle() {
        let params = params();
        let model = place(&bar(), &params);
        let setup = setup(&model, &params, 0.0);
        let half = 10.0 + setup.margin();
        // Top face: 3 mm above the axis, so 7 mm below the stock top.
        let top = setup.field(0.0, half);
        assert!((deepest(&top, &params) + 10.0).abs() < 1e-6);
        let centre = top.heights[(top.rows / 2) * top.columns + top.columns / 2].unwrap();
        assert!((centre + 7.0).abs() < 0.01, "{centre}");
        // A quarter turn shows the 10 mm side: 5 mm above the axis.
        let side = setup.field(90.0, half);
        let centre = side.heights[(side.rows / 2) * side.columns + side.columns / 2].unwrap();
        assert!((centre + 5.0).abs() < 0.01, "{centre}");
        // The stem is never cut, even where there is no part.
        let held = setup_with_stem_deepest(&model, &params);
        assert!((held + 10.0 - 3.0).abs() < 1e-6, "{held}");
    }

    fn setup_with_stem_deepest(model: &[[[f64; 3]; 3]], params: &ReliefParams) -> f64 {
        let setup = setup(model, params, 3.0);
        let field = setup.field(0.0, 10.0 + setup.margin());
        let middle = field.rows / 2;
        (0..field.columns)
            .filter_map(|column| field.heights[middle * field.columns + column])
            .fold(0.0, f64::min)
    }

    #[test]
    fn the_continuous_finish_follows_the_part_in_inverse_time() {
        let params = ReliefParams {
            width: 30.0,
            roughing: true,
            ..params()
        };
        let indexed = IndexedParams {
            stock_diameter: 20.0,
            faces: 4,
            stem_diameter: 4.0,
            continuous_finish: true,
        };
        let gcode = indexed_gcode(&cylinder(), &params, &indexed).unwrap();
        let program = mml_gcode::parse(&gcode);
        assert_eq!(program.rotary_diameter, Some(20.0));
        let start = gcode.find("G93").unwrap();
        let end = gcode.rfind("G94\n").unwrap();
        assert!(start < end && gcode[end..].contains("G92 A0"));
        let helix: Vec<_> = program
            .motions
            .iter()
            .filter(|motion| motion.minutes.is_some())
            .collect();
        assert!(helix.len() > 1000);
        let last = helix.last().unwrap();
        assert!(
            (last.a[1] / 360.0).fract().abs() < 1e-9,
            "ends on a whole turn"
        );
        // Flat tool on a 4 mm cylinder: 6 mm below the stock top, all
        // along the part (the ends ramp over the tool radius).
        for motion in helix
            .iter()
            .filter(|motion| (2.0..28.0).contains(&motion.to[0]))
        {
            assert!((motion.to[2] + 6.0).abs() < 0.05, "{:?}", motion.to);
        }
        assert!(
            helix
                .iter()
                .all(|motion| motion.to[2] >= -10.0 + 2.0 - 1e-6)
        );
        // 1 / F minutes at the programmed feed over the surface length.
        let feed_line = gcode[start..].lines().nth(5).unwrap();
        assert!(feed_line.starts_with("G1 X"));
    }

    #[test]
    fn the_program_turns_between_faces_with_the_tool_up() {
        let gcode = indexed_gcode(
            &bar(),
            &params(),
            &IndexedParams {
                stock_diameter: 20.0,
                faces: 4,
                continuous_finish: false,
                ..IndexedParams::default()
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
                ..IndexedParams::default()
            },
        )
        .unwrap_err();
        let IndexedError::StockTooSmall { needed } = error;
        assert!((needed - (10.0f64.hypot(6.0))).abs() < 0.01);
    }
}
