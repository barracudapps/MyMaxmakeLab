//! Positional 5-axis machining (3+2) on a table-table machine: the A
//! and C axes turn the part so that one of its sides faces the tool,
//! then that side is machined as a 3-axis relief. The rotary axes only
//! move between sides, with the tool above everything that can turn.
//!
//! The table itself is part of the surface the tool must stay above,
//! so a side tilted to 90° never drives the tool into the table.

use crate::{HeightField, ReliefParams, Triangle, bounds, emit_field_at, field_step};
use mml_core::machine::Trunnion;
use mml_gcode::GcodeWriter;

/// Sides of the part, by the direction they face on the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Front,
    Back,
    Left,
    Right,
}

impl Side {
    pub const ALL: [Side; 5] = [Side::Top, Side::Front, Side::Back, Side::Left, Side::Right];

    /// Direction in part coordinates, sides tilted by `tilt` degrees
    /// from the vertical (90 for a true side).
    pub fn normal(self, tilt: f64) -> [f64; 3] {
        let (sin, cos) = tilt.to_radians().sin_cos();
        match self {
            Side::Top => [0.0, 0.0, 1.0],
            Side::Front => [0.0, -sin, cos],
            Side::Back => [0.0, sin, cos],
            Side::Left => [-sin, 0.0, cos],
            Side::Right => [sin, 0.0, cos],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionalParams {
    /// Block of stock centred on the table, X Y Z in mm.
    pub stock: [f64; 3],
    pub sides: Vec<Side>,
    /// Tilt of the side setups from the vertical, degrees.
    pub tilt: f64,
}

impl Default for PositionalParams {
    fn default() -> Self {
        Self {
            stock: [50.0, 50.0, 30.0],
            sides: Side::ALL.to_vec(),
            tilt: 90.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PositionalError {
    #[error("the model ({0:.1} x {1:.1} x {2:.1} mm) does not fit in the stock")]
    StockTooSmall(f64, f64, f64),
    #[error("the A axis cannot tilt the part to face {0:?}")]
    Unreachable(Side),
}

/// Model scaled to `params.width`, centred on the table, sitting on it.
fn place(triangles: &[Triangle], params: &ReliefParams) -> Vec<[[f64; 3]; 3]> {
    let (low, high) = bounds(triangles);
    let scale = params.width / ((high[0] - low[0]) as f64).max(1e-6);
    let centre = [
        (low[0] + high[0]) as f64 / 2.0,
        (low[1] + high[1]) as f64 / 2.0,
    ];
    triangles
        .iter()
        .map(|triangle| {
            triangle.map(|vertex| {
                [
                    (vertex[0] as f64 - centre[0]) * scale,
                    (vertex[1] as f64 - centre[1]) * scale,
                    (vertex[2] - low[2]) as f64 * scale,
                ]
            })
        })
        .collect()
}

/// The table top as a 1 mm thick disc of triangles, an obstacle.
fn table(radius: f64) -> Vec<[[f64; 3]; 3]> {
    let point = |index: usize, z: f64| {
        let angle = index as f64 / 48.0 * std::f64::consts::TAU;
        [radius * angle.cos(), radius * angle.sin(), z]
    };
    (0..48)
        .flat_map(|index| {
            let (top, bottom) = (-0.001, -1.0);
            [
                [[0.0, 0.0, top], point(index, top), point(index + 1, top)],
                [
                    point(index, top),
                    point(index, bottom),
                    point(index + 1, bottom),
                ],
                [
                    point(index, top),
                    point(index + 1, bottom),
                    point(index + 1, top),
                ],
            ]
        })
        .collect()
}

fn box_corners(stock: [f64; 3]) -> Vec<[f64; 3]> {
    (0..8)
        .map(|index| {
            [
                if index & 1 == 0 {
                    -stock[0] / 2.0
                } else {
                    stock[0] / 2.0
                },
                if index & 2 == 0 {
                    -stock[1] / 2.0
                } else {
                    stock[1] / 2.0
                },
                if index & 4 == 0 { 0.0 } else { stock[2] },
            ]
        })
        .collect()
}

pub fn positional_gcode(
    triangles: &[Triangle],
    params: &ReliefParams,
    positional: &PositionalParams,
    trunnion: &Trunnion,
) -> Result<String, PositionalError> {
    let model = place(triangles, params);
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for vertex in model.iter().flatten() {
        for axis in 0..3 {
            low[axis] = low[axis].min(vertex[axis]);
            high[axis] = high[axis].max(vertex[axis]);
        }
    }
    let stock = positional.stock;
    if high[0] > stock[0] / 2.0 + 1e-6
        || -low[0] > stock[0] / 2.0 + 1e-6
        || high[1] > stock[1] / 2.0 + 1e-6
        || -low[1] > stock[1] / 2.0 + 1e-6
        || high[2] > stock[2] + 1e-6
    {
        return Err(PositionalError::StockTooSmall(
            high[0] - low[0],
            high[1] - low[1],
            high[2] - low[2],
        ));
    }
    let setups: Vec<(Side, f64, f64)> = positional
        .sides
        .iter()
        .map(|&side| {
            trunnion
                .orient(side.normal(positional.tilt))
                .map(|(a, c)| (side, a, c))
                .ok_or(PositionalError::Unreachable(side))
        })
        .collect::<Result<_, _>>()?;
    let obstacles: Vec<[[f64; 3]; 3]> = model
        .iter()
        .copied()
        .chain(table(trunnion.table_radius))
        .collect();
    // Above everything that turns: the tool may stay anywhere in XY
    // while the axes move.
    let corners = box_corners(stock);
    let swing = corners
        .iter()
        .copied()
        .chain((0..8).map(|index| {
            let angle = index as f64 / 8.0 * std::f64::consts::TAU;
            let radius = trunnion.table_radius;
            [radius * angle.cos(), radius * angle.sin(), 0.0]
        }))
        .map(|[x, y, z]| (x * x + y * y + (z + trunnion.table_height).powi(2)).sqrt())
        .fold(0.0, f64::max);
    let clearance = swing + params.safe_z;
    let step = field_step(params);
    let margin = params.tool_diameter / 2.0 + step;

    let mut writer = GcodeWriter::new();
    writer.comment(&format!(
        "Positional 5-axis, {} sides, stock {:.1} x {:.1} x {:.1} mm",
        setups.len(),
        stock[0],
        stock[1],
        stock[2]
    ));
    writer.comment(&format!(
        "Trunnion table height {:.3} mm",
        trunnion.table_height
    ));
    writer.comment("Work zero on the pivot (A and C axes crossing)");
    writer.preamble();
    for (side, a, c) in setups {
        writer.rapid_z(clearance);
        writer.comment(&format!("{side:?} side at A{a:.1} C{c:.1}"));
        writer.raw(&format!("G0 A{a:.3} C{c:.3}"));
        writer.rapid_z(clearance);
        let turned_stock: Vec<[f64; 3]> = corners
            .iter()
            .map(|&corner| trunnion.to_machine(corner, a, c))
            .collect();
        let top = turned_stock
            .iter()
            .map(|point| point[2])
            .fold(f64::MIN, f64::max);
        let min_x = turned_stock
            .iter()
            .map(|point| point[0])
            .fold(f64::MAX, f64::min)
            - margin;
        let max_x = turned_stock
            .iter()
            .map(|point| point[0])
            .fold(f64::MIN, f64::max)
            + margin;
        let min_y = turned_stock
            .iter()
            .map(|point| point[1])
            .fold(f64::MAX, f64::min)
            - margin;
        let max_y = turned_stock
            .iter()
            .map(|point| point[1])
            .fold(f64::MIN, f64::max)
            + margin;
        let columns = ((max_x - min_x) / step).ceil() as usize + 1;
        let rows = ((max_y - min_y) / step).ceil() as usize + 1;
        let turned = obstacles
            .iter()
            .map(|triangle| triangle.map(|point| trunnion.to_machine(point, a, c)));
        // Each side reaches at most `params.depth` below the stock top.
        let floor = -params.depth.max(0.1);
        let heights = crate::rasterize(turned, [min_x, min_y], step, columns, rows)
            .into_iter()
            .map(|height| Some(height.map_or(floor, |height| (height - top).max(floor))))
            .collect();
        let field = HeightField {
            origin: [min_x, min_y],
            step,
            columns,
            rows,
            heights,
        };
        emit_field_at(&mut writer, &field, params, top);
    }
    writer.rapid_z(clearance);
    writer.raw("G0 A0 C0");
    writer.program_end();
    Ok(writer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cube of 20 mm sitting on the table.
    fn cube() -> Vec<Triangle> {
        let v = |i: usize| -> [f32; 3] {
            [
                if i & 1 == 0 { 0.0 } else { 20.0 },
                if i & 2 == 0 { 0.0 } else { 20.0 },
                if i & 4 == 0 { 0.0 } else { 20.0 },
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
            width: 20.0,
            depth: 15.0,
            tool: crate::ToolShape::Flat,
            tool_diameter: 4.0,
            stepover: 0.5,
            pass_depth: 3.0,
            ..ReliefParams::default()
        }
    }

    #[test]
    fn every_side_is_cut_down_to_the_part_and_never_into_it_or_the_table() {
        let trunnion = Trunnion {
            table_height: 10.0,
            table_radius: 40.0,
            ..Trunnion::default()
        };
        let positional = PositionalParams {
            stock: [30.0, 30.0, 25.0],
            ..PositionalParams::default()
        };
        let gcode = positional_gcode(&cube(), &params(), &positional, &trunnion).unwrap();
        assert!(gcode.contains("G0 A0.000 C0.000"), "top");
        assert!(gcode.contains("G0 A90.000 C180.000"), "front");
        let program = mml_gcode::parse(&gcode);
        assert_eq!(program.trunnion_table, Some(10.0));
        let mut deepest_per_side = Vec::new();
        for motion in program.motions.iter().filter(|motion| motion.is_cutting()) {
            let [x, y, z] = trunnion.to_part(motion.to, motion.a[1], motion.c[1]);
            // Tool tip outside the cube and above the table, 1 step slack.
            let inside =
                (0.3..19.7).contains(&(x + 10.0)) && (0.3..19.7).contains(&(y + 10.0)) && z < 19.7;
            assert!(!inside, "gouge at part {x:.2} {y:.2} {z:.2}");
            assert!(
                z > -0.01 || x.hypot(y) > 40.0,
                "into the table at {x:.2} {y:.2} {z:.2}"
            );
            deepest_per_side.push((motion.a[1], motion.c[1], z));
        }
        // No move at all, rapids included, crosses the cube or the table
        // (the first one starts from the parser origin, the pivot, where
        // the tool never is).
        for motion in program.motions.iter().skip(1) {
            for sample in 0..=20 {
                let t = sample as f64 / 20.0;
                let point = [0, 1, 2]
                    .map(|axis| motion.from[axis] + (motion.to[axis] - motion.from[axis]) * t);
                let [x, y, z] = trunnion.to_part(point, motion.a[1], motion.c[1]);
                let in_cube = (0.3..19.7).contains(&(x + 10.0))
                    && (0.3..19.7).contains(&(y + 10.0))
                    && (0.3..19.7).contains(&z);
                let in_table = (-0.99..-0.01).contains(&z) && x.hypot(y) < 39.9;
                assert!(!in_cube && !in_table, "move through {x:.2} {y:.2} {z:.2}");
            }
        }
        // The top side reaches the top of the cube.
        let top_lowest = deepest_per_side
            .iter()
            .filter(|(a, _, _)| a.abs() < 1e-9)
            .map(|(_, _, z)| *z)
            .fold(f64::MAX, f64::min);
        assert!(
            top_lowest < 20.0 - 2.0,
            "top side cut down beside the cube: {top_lowest}"
        );
    }

    #[test]
    fn axes_turn_only_with_the_tool_above_everything() {
        let trunnion = Trunnion {
            table_height: 10.0,
            table_radius: 40.0,
            ..Trunnion::default()
        };
        let gcode =
            positional_gcode(&cube(), &params(), &PositionalParams::default(), &trunnion).unwrap();
        let mut z = 0.0f64;
        for line in gcode.lines() {
            if let Some(index) = line.find('Z') {
                let value: String = line[index + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | '.'))
                    .collect();
                z = value.parse().unwrap();
            }
            if line.starts_with("G0 A") {
                assert!(z > 40.0, "turning at Z{z}");
            }
        }
    }

    #[test]
    fn unreachable_sides_and_oversized_models_are_refused() {
        let trunnion = Trunnion {
            a_min: -30.0,
            a_max: 30.0,
            ..Trunnion::default()
        };
        assert_eq!(
            positional_gcode(&cube(), &params(), &PositionalParams::default(), &trunnion),
            Err(PositionalError::Unreachable(Side::Front))
        );
        let small = PositionalParams {
            stock: [10.0, 10.0, 10.0],
            ..PositionalParams::default()
        };
        assert!(matches!(
            positional_gcode(&cube(), &params(), &small, &Trunnion::default()),
            Err(PositionalError::StockTooSmall(..))
        ));
    }
}
