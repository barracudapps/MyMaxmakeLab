//! 3D probe routines and height maps.
//!
//! Probing uses G38.2 (alarm when nothing is touched) for references
//! that must exist, and G38.3 (no alarm) for surface mapping, so that
//! a grid larger than the piece also tells where the piece is.

use mml_core::geometry::{Point, Rect};
use mml_gcode::{MoveKind, Program};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProbeSettings {
    /// Thickness of the touch plate placed on the stock, mm.
    pub plate_thickness: f64,
    /// Side of the XYZ probe block, used for corner probing, mm.
    pub tool_diameter: f64,
    pub seek_feed: f64,
    pub slow_feed: f64,
    /// Maximum probing distance, mm.
    pub max_travel: f64,
    pub retract: f64,
    /// Lateral distance from the plate edge when probing X and Y.
    pub clearance: f64,
    /// Depth below the plate top to probe the plate sides.
    pub side_depth: f64,
}

impl Default for ProbeSettings {
    fn default() -> Self {
        Self {
            plate_thickness: 10.0,
            tool_diameter: 3.175,
            seek_feed: 150.0,
            slow_feed: 30.0,
            max_travel: 25.0,
            retract: 2.0,
            clearance: 8.0,
            side_depth: 4.0,
        }
    }
}

/// Probes Z on a touch plate then sets the work Z zero on the stock top.
pub fn z_origin_program(settings: &ProbeSettings) -> Vec<String> {
    vec![
        "G21 G91".into(),
        format!(
            "G38.2 Z-{:.3} F{:.0}",
            settings.max_travel, settings.seek_feed
        ),
        format!("G0 Z{:.3}", settings.retract),
        format!(
            "G38.2 Z-{:.3} F{:.0}",
            settings.retract * 2.0,
            settings.slow_feed
        ),
        format!("G10 L20 P1 Z{:.3}", settings.plate_thickness),
        format!("G0 Z{:.3}", settings.retract + 3.0),
        "G90".into(),
    ]
}

/// Probes the front-left corner of the stock with an XYZ block: Z on
/// the top, then X on the left side and Y on the front side. The tool
/// must start above the block, roughly at its centre.
pub fn corner_origin_program(settings: &ProbeSettings) -> Vec<String> {
    let radius = settings.tool_diameter / 2.0;
    let mut lines = z_origin_program(settings);
    lines.pop();
    let lift = settings.retract + 3.0;
    lines.extend([
        "G91".to_owned(),
        // X: move left of the block, go down and touch its left face.
        format!("G0 X-{:.3}", settings.clearance + 10.0),
        format!("G0 Z-{:.3}", lift + settings.side_depth),
        format!(
            "G38.2 X{:.3} F{:.0}",
            settings.clearance * 2.0,
            settings.seek_feed
        ),
        format!("G10 L20 P1 X{:.3}", -radius),
        format!("G0 X-{:.3}", settings.retract),
        format!("G0 Z{:.3}", lift + settings.side_depth),
        // Y: back over the block, then in front of it.
        format!("G0 X{:.3}", settings.clearance + 10.0),
        format!("G0 Y-{:.3}", settings.clearance + 10.0),
        format!("G0 Z-{:.3}", lift + settings.side_depth),
        format!(
            "G38.2 Y{:.3} F{:.0}",
            settings.clearance * 2.0,
            settings.seek_feed
        ),
        format!("G10 L20 P1 Y{:.3}", -radius),
        format!("G0 Y-{:.3}", settings.retract),
        format!("G0 Z{:.3}", lift + settings.side_depth),
        "G90".to_owned(),
    ]);
    lines
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridSpec {
    pub area: Rect,
    pub columns: usize,
    pub rows: usize,
    /// Height of the travel moves above the expected surface.
    pub safe_z: f64,
    /// Lowest Z to probe to before declaring "no surface here".
    pub min_z: f64,
    pub feed: f64,
}

impl GridSpec {
    pub fn points(&self) -> Vec<Point> {
        let (columns, rows) = (self.columns.max(2), self.rows.max(2));
        let mut points = Vec::with_capacity(columns * rows);
        for row in 0..rows {
            // Serpentine order shortens the travel.
            let order: Vec<usize> = if row % 2 == 0 {
                (0..columns).collect()
            } else {
                (0..columns).rev().collect()
            };
            for column in order {
                points.push(Point::new(
                    self.area.min.x + self.area.width() * column as f64 / (columns - 1) as f64,
                    self.area.min.y + self.area.height() * row as f64 / (rows - 1) as f64,
                ));
            }
        }
        points
    }
}

/// Work-coordinate program probing every grid point with G38.3.
pub fn grid_program(spec: &GridSpec) -> Vec<String> {
    let mut lines = vec!["G21 G90".to_owned(), format!("G0 Z{:.3}", spec.safe_z)];
    for point in spec.points() {
        lines.push(format!("G0 X{:.3} Y{:.3}", point.x, point.y));
        lines.push(format!("G38.3 Z{:.3} F{:.0}", spec.min_z, spec.feed));
        lines.push(format!("G0 Z{:.3}", spec.safe_z));
    }
    lines
}

/// Parses `[PRB:x,y,z:1]`; returns the machine position and whether
/// the probe touched.
pub fn parse_probe_report(line: &str) -> Option<([f64; 3], bool)> {
    let body = line.trim().strip_prefix("[PRB:")?.strip_suffix(']')?;
    let (coordinates, touched) = body.rsplit_once(':')?;
    let values: Vec<f64> = coordinates
        .split(',')
        .filter_map(|value| value.parse().ok())
        .collect();
    (values.len() >= 3).then(|| ([values[0], values[1], values[2]], touched.trim() == "1"))
}

/// Surface heights on a regular grid; `None` where nothing was touched
/// (outside the piece).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightMap {
    pub area: Rect,
    pub columns: usize,
    pub rows: usize,
    pub heights: Vec<Option<f64>>,
}

impl HeightMap {
    pub fn new(area: Rect, columns: usize, rows: usize) -> Self {
        let (columns, rows) = (columns.max(2), rows.max(2));
        Self {
            area,
            columns,
            rows,
            heights: vec![None; columns * rows],
        }
    }

    pub fn from_fn(
        area: Rect,
        columns: usize,
        rows: usize,
        height: impl Fn(Point) -> Option<f64>,
    ) -> Self {
        let mut map = Self::new(area, columns, rows);
        for row in 0..map.rows {
            for column in 0..map.columns {
                let point = map.node(column, row);
                map.heights[row * map.columns + column] = height(point);
            }
        }
        map
    }

    pub fn node(&self, column: usize, row: usize) -> Point {
        Point::new(
            self.area.min.x + self.area.width() * column as f64 / (self.columns - 1) as f64,
            self.area.min.y + self.area.height() * row as f64 / (self.rows - 1) as f64,
        )
    }

    /// Records a probe result at the nearest node.
    pub fn record(&mut self, point: Point, height: Option<f64>) {
        let column = ((point.x - self.area.min.x) / self.area.width().max(1e-9)
            * (self.columns - 1) as f64)
            .round();
        let row = ((point.y - self.area.min.y) / self.area.height().max(1e-9)
            * (self.rows - 1) as f64)
            .round();
        if column >= 0.0
            && row >= 0.0
            && (column as usize) < self.columns
            && (row as usize) < self.rows
        {
            self.heights[row as usize * self.columns + column as usize] = height;
        }
    }

    fn at(&self, column: usize, row: usize) -> Option<f64> {
        self.heights[row * self.columns + column]
    }

    /// Bilinear height; nodes without contact are ignored when at least
    /// one neighbour has one, so the edge of a piece stays usable.
    pub fn height(&self, point: Point) -> Option<f64> {
        let u = ((point.x - self.area.min.x) / self.area.width().max(1e-9)).clamp(0.0, 1.0)
            * (self.columns - 1) as f64;
        let v = ((point.y - self.area.min.y) / self.area.height().max(1e-9)).clamp(0.0, 1.0)
            * (self.rows - 1) as f64;
        let (c0, r0) = (
            (u.floor() as usize).min(self.columns - 2),
            (v.floor() as usize).min(self.rows - 2),
        );
        let (fu, fv) = (u - c0 as f64, v - r0 as f64);
        let corners = [
            (self.at(c0, r0), (1.0 - fu) * (1.0 - fv)),
            (self.at(c0 + 1, r0), fu * (1.0 - fv)),
            (self.at(c0, r0 + 1), (1.0 - fu) * fv),
            (self.at(c0 + 1, r0 + 1), fu * fv),
        ];
        let (sum, weight) = corners
            .iter()
            .filter_map(|(height, weight)| height.map(|height| (height * weight, *weight)))
            .fold((0.0, 0.0), |(sum, total), (value, weight)| {
                (sum + value, total + weight)
            });
        (weight > 1e-9).then(|| sum / weight)
    }

    /// Surface slope in degrees at `point` (0 = flat).
    pub fn slope_degrees(&self, point: Point) -> f64 {
        let step_x = self.area.width() / (self.columns - 1) as f64 / 2.0;
        let step_y = self.area.height() / (self.rows - 1) as f64 / 2.0;
        let sample = |dx: f64, dy: f64| self.height(Point::new(point.x + dx, point.y + dy));
        let (Some(left), Some(right), Some(front), Some(back)) = (
            sample(-step_x, 0.0),
            sample(step_x, 0.0),
            sample(0.0, -step_y),
            sample(0.0, step_y),
        ) else {
            return 0.0;
        };
        let gx = (right - left) / (2.0 * step_x).max(1e-9);
        let gy = (back - front) / (2.0 * step_y).max(1e-9);
        gx.hypot(gy).atan().to_degrees()
    }

    /// Outline of the touched nodes (convex hull), i.e. the footprint of
    /// the piece found under the grid.
    pub fn footprint(&self) -> Vec<Point> {
        let mut points: Vec<Point> = (0..self.rows)
            .flat_map(|row| (0..self.columns).map(move |column| (column, row)))
            .filter(|(column, row)| self.at(*column, *row).is_some())
            .map(|(column, row)| self.node(column, row))
            .collect();
        convex_hull(&mut points)
    }

    pub fn touched(&self) -> usize {
        self.heights
            .iter()
            .filter(|height| height.is_some())
            .count()
    }
}

fn convex_hull(points: &mut [Point]) -> Vec<Point> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut hull: Vec<Point> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iterator: Box<dyn Iterator<Item = &Point>> = if pass == 0 {
            Box::new(points.iter())
        } else {
            Box::new(points.iter().rev())
        };
        for point in iterator {
            while hull.len() >= start + 2 {
                let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                if (b - a).cross(*point - a) <= 0.0 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(*point);
        }
        hull.pop();
    }
    hull
}

#[derive(Debug, Clone, Copy)]
pub struct FollowOptions {
    /// Longest move kept straight; longer moves are split so the head
    /// follows the surface between grid nodes.
    pub max_segment: f64,
    /// Laser: raise power as 1 / cos(slope) on inclined faces (the spot
    /// spreads over a larger area), capped by `max_power_gain`.
    pub slope_power: bool,
    pub max_power_gain: f64,
    pub max_s: f64,
}

impl Default for FollowOptions {
    fn default() -> Self {
        Self {
            max_segment: 1.0,
            slope_power: false,
            max_power_gain: 1.5,
            max_s: 1000.0,
        }
    }
}

/// Rewrites the moves of `source` so Z follows the height map: every
/// move gets `Z = z + height(x, y)`. Non-motion lines are kept as is.
/// Points outside the touched area keep the nearest known height.
pub fn follow_surface(
    source: &str,
    program: &Program,
    map: &HeightMap,
    options: FollowOptions,
) -> String {
    let mut by_line: Vec<Vec<usize>> = vec![Vec::new(); program.line_count + 1];
    for (index, motion) in program.motions.iter().enumerate() {
        by_line[motion.line].push(index);
    }
    let fallback = map
        .heights
        .iter()
        .flatten()
        .copied()
        .fold(None, |best: Option<f64>, height| {
            Some(best.map_or(height, |best| best.max(height)))
        });
    let height_at = |point: Point| map.height(point).or(fallback).unwrap_or(0.0);
    let mut out = String::with_capacity(source.len() * 2);
    for (line_index, line) in source.lines().enumerate() {
        let motions = by_line
            .get(line_index)
            .filter(|motions| !motions.is_empty());
        let Some(motions) = motions else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        for &motion_index in motions {
            let motion = program.motions[motion_index];
            let from = Point::new(motion.from[0], motion.from[1]);
            let to = Point::new(motion.to[0], motion.to[1]);
            let pieces = if motion.kind == MoveKind::Feed {
                ((from.distance(to) / options.max_segment.max(0.05)).ceil() as usize).max(1)
            } else {
                1
            };
            for piece in 1..=pieces {
                let t = piece as f64 / pieces as f64;
                let point = from.lerp(to, t);
                let z = motion.from[2] + (motion.to[2] - motion.from[2]) * t + height_at(point);
                match motion.kind {
                    MoveKind::Rapid => {
                        let _ = writeln!(out, "G90 G0 X{:.3} Y{:.3} Z{:.3}", point.x, point.y, z);
                    }
                    MoveKind::Feed => {
                        let mut power = motion.power;
                        if options.slope_power && power > 0.0 {
                            let slope = map.slope_degrees(point).min(75.0).to_radians();
                            let gain = (1.0 / slope.cos()).min(options.max_power_gain);
                            power = (power * gain).min(options.max_s);
                        }
                        let _ = writeln!(
                            out,
                            "G90 G1 X{:.3} Y{:.3} Z{:.3} F{:.0} S{:.0}",
                            point.x, point.y, z, motion.feed, power
                        );
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tilted() -> HeightMap {
        let area = Rect::from_points(Point::new(0.0, 0.0), Point::new(10.0, 10.0));
        HeightMap::from_fn(area, 3, 3, |point| Some(point.x * 0.1))
    }

    #[test]
    fn probe_reports_are_parsed() {
        assert_eq!(
            parse_probe_report("[PRB:1.000,2.000,-3.500:1]"),
            Some(([1.0, 2.0, -3.5], true))
        );
        assert_eq!(
            parse_probe_report("[PRB:0.000,0.000,-20.000:0]"),
            Some(([0.0, 0.0, -20.0], false))
        );
    }

    #[test]
    fn heights_interpolate_and_slope_is_measured() {
        let map = tilted();
        assert!((map.height(Point::new(2.5, 7.0)).unwrap() - 0.25).abs() < 1e-9);
        let slope = map.slope_degrees(Point::new(5.0, 5.0));
        assert!((slope - 0.1f64.atan().to_degrees()).abs() < 1e-6);
    }

    #[test]
    fn footprint_finds_the_piece_under_a_larger_grid() {
        let area = Rect::from_points(Point::new(0.0, 0.0), Point::new(100.0, 100.0));
        let map = HeightMap::from_fn(area, 11, 11, |point| {
            (point.x >= 20.0 && point.x <= 60.0 && point.y >= 30.0 && point.y <= 50.0)
                .then_some(5.0)
        });
        let hull = map.footprint();
        let bounds = mml_core::geometry::Path::new(hull, true).bounds();
        assert_eq!(
            (bounds.min.x, bounds.max.x, bounds.min.y, bounds.max.y),
            (20.0, 60.0, 30.0, 50.0)
        );
    }

    #[test]
    fn moves_follow_the_surface_and_long_moves_are_split() {
        let source = "M4 S0\nG1 X10 Y0 F600 S500\nM5";
        let program = mml_gcode::parse(source);
        let out = follow_surface(
            source,
            &program,
            &tilted(),
            FollowOptions {
                max_segment: 2.5,
                ..FollowOptions::default()
            },
        );
        let followed = mml_gcode::parse(&out);
        assert_eq!(followed.motions.len(), 4);
        let last = followed.motions.last().unwrap();
        assert!((last.to[2] - 1.0).abs() < 1e-9);
        assert!(out.starts_with("M4 S0\n") && out.ends_with("M5\n"));
    }

    #[test]
    fn grid_uses_non_alarming_probe() {
        let spec = GridSpec {
            area: Rect::from_points(Point::new(0.0, 0.0), Point::new(10.0, 10.0)),
            columns: 2,
            rows: 2,
            safe_z: 5.0,
            min_z: -10.0,
            feed: 100.0,
        };
        let program = grid_program(&spec);
        assert_eq!(
            program
                .iter()
                .filter(|line| line.starts_with("G38.3"))
                .count(),
            4
        );
    }
}
