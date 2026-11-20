//! Collision clearance of a milling program: how deep the tool may go at
//! each point before something other than its flutes touches material
//! or a fixture. The whole tool assembly is considered:
//!
//! - the cutting flutes, of the tool diameter, `flute_length` long;
//! - above them the shank, then the collet nut and the spindle nose,
//!   `holder_diameter` wide, starting `stick_out` above the tip;
//!
//! and the work: the top of the material (flat, tilted, probed or of any
//! shape), the bottom of the stock above the table, and obstacles of any
//! outline (clamps, vices, jigs) with their height.

use crate::parser::{MoveKind, Program};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolAssembly {
    pub tool_diameter: f64,
    /// Length of the cutting part, mm.
    pub flute_length: f64,
    /// From the tip to the collet nut, mm.
    pub stick_out: f64,
    /// Collet nut or spindle nose diameter, mm.
    pub holder_diameter: f64,
}

impl Default for ToolAssembly {
    fn default() -> Self {
        Self {
            tool_diameter: 3.175,
            flute_length: 12.0,
            stick_out: 20.0,
            holder_diameter: 20.0,
        }
    }
}

/// A fixture the tool must never touch: closed outline(s) in work XY
/// and the height of its top.
#[derive(Debug, Clone, PartialEq)]
pub struct Obstacle {
    pub outline: Vec<Vec<[f64; 2]>>,
    pub top: f64,
}

impl Obstacle {
    fn contains(&self, x: f64, y: f64) -> bool {
        let mut inside = false;
        for ring in &self.outline {
            let count = ring.len();
            for index in 0..count {
                let (a, b) = (ring[index], ring[(index + 1) % count]);
                if (a[1] > y) != (b[1] > y) && x < a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
                {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Distance from a point to the obstacle, 0 inside.
    fn distance(&self, x: f64, y: f64) -> f64 {
        if self.contains(x, y) {
            return 0.0;
        }
        let mut nearest = f64::MAX;
        for ring in &self.outline {
            let count = ring.len();
            for index in 0..count {
                let (a, b) = (ring[index], ring[(index + 1) % count]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let length = dx * dx + dy * dy;
                let t = if length > 0.0 {
                    (((x - a[0]) * dx + (y - a[1]) * dy) / length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                nearest = nearest.min((x - a[0] - dx * t).hypot(y - a[1] - dy * t));
            }
        }
        nearest
    }

    /// Whether a disc of `radius` around the point touches it, exactly:
    /// a thin clamp edge is never missed between samples.
    fn reached(&self, x: f64, y: f64, radius: f64) -> bool {
        self.distance(x, y) <= radius + 1e-9
    }
}

pub struct Scene<'a> {
    /// Top of the material at a work position (Z0 is usually the stock
    /// top; a probed map or a model gives other shapes).
    pub surface: &'a dyn Fn(f64, f64) -> f64,
    /// Z of the bottom of the stock (minus its thickness).
    pub stock_bottom: f64,
    /// How far below the stock bottom a cut may go (through cuts into a
    /// spoilboard), mm.
    pub through: f64,
    pub obstacles: &'a [Obstacle],
    /// Clearance kept from material and obstacles, mm.
    pub margin: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The table under the stock.
    Table,
    /// Material above the flutes: the shank would rub.
    Flutes,
    /// The collet nut or spindle would hit material.
    Holder,
    /// The tool would cut a fixture.
    Obstacle,
}

/// Samples of a disc: the centre and two rings.
fn disc(x: f64, y: f64, radius: f64) -> impl Iterator<Item = (f64, f64)> {
    std::iter::once((x, y)).chain((1..=2).flat_map(move |ring| {
        let r = radius * ring as f64 / 2.0;
        (0..12).map(move |step| {
            let angle = std::f64::consts::TAU * step as f64 / 12.0;
            (x + r * angle.cos(), y + r * angle.sin())
        })
    }))
}

impl Scene<'_> {
    /// Highest material or obstacle top within `radius` of a point:
    /// obstacles exactly, the material surface by sampling.
    fn highest(&self, x: f64, y: f64, radius: f64) -> f64 {
        let material = disc(x, y, radius)
            .map(|(px, py)| (self.surface)(px, py))
            .fold(f64::MIN, f64::max);
        self.obstacles
            .iter()
            .filter(|obstacle| obstacle.reached(x, y, radius))
            .map(|obstacle| obstacle.top)
            .fold(material, f64::max)
    }

    /// Lowest tip height allowed at a point and what sets it.
    pub fn lowest_tip(&self, tool: &ToolAssembly, x: f64, y: f64) -> (f64, Limit) {
        let radius = tool.tool_diameter / 2.0;
        let mut limits = vec![(self.stock_bottom - self.through, Limit::Table)];
        // The tool itself over a fixture: stay above it.
        let fixture = self
            .obstacles
            .iter()
            .filter(|obstacle| obstacle.reached(x, y, radius))
            .map(|obstacle| obstacle.top)
            .fold(f64::MIN, f64::max);
        if fixture > f64::MIN {
            limits.push((fixture + self.margin, Limit::Obstacle));
        }
        // Material higher than the flutes reach.
        let around_tool = disc(x, y, radius)
            .map(|(px, py)| (self.surface)(px, py))
            .fold(f64::MIN, f64::max);
        limits.push((around_tool - tool.flute_length, Limit::Flutes));
        // The holder above everything under its footprint.
        let under_holder = self.highest(x, y, tool.holder_diameter / 2.0);
        limits.push((under_holder + self.margin - tool.stick_out, Limit::Holder));
        limits
            .into_iter()
            .fold((f64::MIN, Limit::Table), |best, item| {
                if item.0 > best.0 { item } else { best }
            })
    }

    /// Depth allowed below the material top at a point.
    pub fn allowed_depth(&self, tool: &ToolAssembly, x: f64, y: f64) -> (f64, Limit) {
        let (lowest, limit) = self.lowest_tip(tool, x, y);
        ((self.surface)(x, y) - lowest, limit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Violation {
    pub line: usize,
    pub at: [f64; 3],
    /// How far below the allowed height the tip goes, mm.
    pub excess: f64,
    pub limit: Limit,
}

/// Every place where the program goes deeper than allowed: feed moves
/// against all limits, rapids against fixtures and the holder. One
/// violation per source line, at most `max` of them.
pub fn check(program: &Program, tool: &ToolAssembly, scene: &Scene, max: usize) -> Vec<Violation> {
    let mut found: Vec<Violation> = Vec::new();
    for motion in &program.motions {
        if found.len() >= max {
            break;
        }
        if found.last().is_some_and(|last| last.line == motion.line) {
            continue;
        }
        // A straight retract moves away from everything.
        let retract = motion.to[2] > motion.from[2]
            && (motion.to[0] - motion.from[0]).abs() < 1e-9
            && (motion.to[1] - motion.from[1]).abs() < 1e-9;
        if retract {
            continue;
        }
        let length = motion.length();
        let steps = (length / 1.0).ceil().max(1.0) as usize;
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let point =
                [0, 1, 2].map(|axis| motion.from[axis] + (motion.to[axis] - motion.from[axis]) * t);
            let (lowest, limit) = scene.lowest_tip(tool, point[0], point[1]);
            // Rapids fly above the work: only fixtures and the holder
            // matter, never the table or flute depth (they do not cut).
            let relevant =
                motion.kind == MoveKind::Feed || matches!(limit, Limit::Obstacle | Limit::Holder);
            if relevant && point[2] < lowest - 1e-6 {
                found.push(Violation {
                    line: motion.line,
                    at: point,
                    excess: lowest - point[2],
                    limit,
                });
                break;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn flat(_: f64, _: f64) -> f64 {
        0.0
    }

    fn clamp() -> Obstacle {
        Obstacle {
            outline: vec![vec![[40.0, 0.0], [60.0, 0.0], [60.0, 10.0], [40.0, 10.0]]],
            top: 12.0,
        }
    }

    #[test]
    fn each_limit_takes_over_where_it_matters() {
        let obstacles = [clamp()];
        let scene = Scene {
            surface: &flat,
            stock_bottom: -18.0,
            through: 0.3,
            obstacles: &obstacles,
            margin: 1.0,
        };
        let tool = ToolAssembly::default();
        // Open stock: the flutes (12 mm) limit, not the 18 mm stock.
        let (depth, limit) = scene.allowed_depth(&tool, 0.0, 50.0);
        assert_eq!(limit, Limit::Flutes);
        assert!((depth - 12.0).abs() < 1e-9);
        // Long flutes and stick-out: the table limits.
        let long = ToolAssembly {
            flute_length: 30.0,
            stick_out: 40.0,
            ..tool
        };
        let (depth, limit) = scene.allowed_depth(&long, 0.0, 50.0);
        assert_eq!(limit, Limit::Table);
        assert!((depth - 18.3).abs() < 1e-9);
        // Next to the clamp (12 mm high): the nut must clear it, so with
        // 20 mm of stick-out the tip may only go 7 mm deep.
        let (depth, limit) = scene.allowed_depth(&tool, 50.0, 15.0);
        assert_eq!(limit, Limit::Holder);
        assert!((depth - 7.0).abs() < 1e-9, "{depth}");
        // On the clamp itself: nothing below its top.
        let (depth, limit) = scene.allowed_depth(&tool, 50.0, 5.0);
        assert_eq!(limit, Limit::Obstacle);
        assert!(depth < -12.0);
    }

    #[test]
    fn a_program_is_checked_line_by_line() {
        let obstacles = [clamp()];
        let scene = Scene {
            surface: &flat,
            stock_bottom: -10.0,
            through: 0.2,
            obstacles: &obstacles,
            margin: 1.0,
        };
        let tool = ToolAssembly::default();
        let source = "G90\nM3 S10000\nG0 Z5\nG0 X10 Y50\nG1 Z-6 F300\nG1 X30 F800\nG1 X50 Y20 Z-9\nG0 Z5\nG0 X50 Y5\nM5\n";
        let violations = check(&parse(source), &tool, &scene, 10);
        let lines: Vec<(usize, Limit)> = violations.iter().map(|v| (v.line, v.limit)).collect();
        // Line 7 (index 6) dives to -9 next to the clamp; line 9 (index 8)
        // flies over the clamp at Z5, below its 12 mm top.
        assert_eq!(
            lines,
            vec![(6, Limit::Holder), (8, Limit::Obstacle)],
            "{violations:?}"
        );
        assert!(violations[0].excess > 1.0);
    }
}
