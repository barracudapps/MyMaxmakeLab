//! Box with rounded corners: the wall is one strip of plywood or MDF
//! that bends around the corners thanks to a living hinge (staggered
//! cuts), held by tabs of the bottom and lid that go through slots.

use crate::geometry::{Path, Point};
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundBoxParams {
    /// Inside width, depth and height, mm.
    pub inner: [f64; 3],
    /// Inside corner radius, mm.
    pub radius: f64,
    pub thickness: f64,
    pub lid: bool,
    /// Distance between hinge cut lines, mm: closer bends tighter.
    pub hinge_pitch: f64,
}

impl Default for RoundBoxParams {
    fn default() -> Self {
        Self {
            inner: [80.0, 60.0, 40.0],
            radius: 15.0,
            thickness: 3.0,
            lid: true,
            hinge_pitch: 1.5,
        }
    }
}

pub struct RoundBox {
    pub wall: Vec<Path>,
    pub bottom: Vec<Path>,
    pub lid: Option<Vec<Path>>,
    /// Length of the wall strip, mm.
    pub wall_length: f64,
}

fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Path {
    Path::new(
        vec![
            Point::new(x, y),
            Point::new(x + width, y),
            Point::new(x + width, y + height),
            Point::new(x, y + height),
        ],
        true,
    )
}

struct Layout {
    flat_x: f64,
    flat_y: f64,
    arc: f64,
    radius: f64,
    tab: f64,
}

impl Layout {
    fn new(params: &RoundBoxParams) -> Self {
        let [w, d, _] = params.inner;
        let radius = params.radius.clamp(params.thickness, w.min(d) / 2.0);
        Self {
            flat_x: w - 2.0 * radius,
            flat_y: d - 2.0 * radius,
            // The wall bends along its middle.
            arc: PI / 2.0 * (radius + params.thickness / 2.0),
            radius,
            tab: (params.thickness * 3.0).max(8.0),
        }
    }

    fn length(&self) -> f64 {
        2.0 * (self.flat_x + self.flat_y) + 4.0 * self.arc
    }

    /// Bent stretches of the strip, which starts in the middle of the
    /// front side and runs round to the right.
    fn arcs(&self) -> [(f64, f64); 4] {
        let mut start = self.flat_x / 2.0;
        let mut arcs = [(0.0, 0.0); 4];
        for (index, flat) in [self.flat_y, self.flat_x, self.flat_y, 0.0]
            .iter()
            .enumerate()
        {
            arcs[index] = (start, start + self.arc);
            start += self.arc + flat;
        }
        arcs
    }

    /// Tabs: (position along the strip, position on the panel, along X
    /// for front and back), only on sides long enough.
    fn tabs(&self, inner: [f64; 3]) -> Vec<(f64, Point, bool)> {
        let (w, d) = (inner[0], inner[1]);
        let [a, b, c, e] = self.arcs();
        let mut tabs = Vec::new();
        if self.flat_x / 2.0 > 2.0 * self.tab {
            tabs.push((
                self.flat_x / 4.0,
                Point::new(self.flat_x / 4.0, -d / 2.0),
                true,
            ));
            tabs.push((
                self.length() - self.flat_x / 4.0,
                Point::new(-self.flat_x / 4.0, -d / 2.0),
                true,
            ));
        }
        if self.flat_y > 2.0 * self.tab {
            tabs.push(((a.1 + b.0) / 2.0, Point::new(w / 2.0, 0.0), false));
            tabs.push(((c.1 + e.0) / 2.0, Point::new(-w / 2.0, 0.0), false));
        }
        if self.flat_x > 2.0 * self.tab {
            tabs.push(((b.1 + c.0) / 2.0, Point::new(0.0, d / 2.0), true));
        }
        tabs
    }
}

/// Rounded rectangle centred on the origin, with tabs sticking out
/// through the wall at the given points.
fn panel(params: &RoundBoxParams, layout: &Layout, tabs: &[(f64, Point, bool)]) -> Vec<Path> {
    let [w, d, _] = params.inner;
    let r = layout.radius;
    let t = params.thickness;
    let half = layout.tab / 2.0;
    let mut points = Vec::new();
    let steps = 12;
    // Corners counter-clockwise from the front right, each side then
    // receives its tabs.
    let corners = [
        (Point::new(w / 2.0 - r, -d / 2.0 + r), -PI / 2.0),
        (Point::new(w / 2.0 - r, d / 2.0 - r), 0.0),
        (Point::new(-w / 2.0 + r, d / 2.0 - r), PI / 2.0),
        (Point::new(-w / 2.0 + r, -d / 2.0 + r), PI),
    ];
    let tab_on = |from: Point, to: Point| -> Vec<Point> {
        // Tabs lying on the straight side from `from` to `to`.
        let direction = to - from;
        let length = direction.length();
        if length < 1e-9 {
            return Vec::new();
        }
        let unit = Point::new(direction.x / length, direction.y / length);
        // Outward normal of a counter-clockwise outline.
        let normal = Point::new(unit.y, -unit.x);
        let mut along: Vec<f64> = tabs
            .iter()
            .map(|(_, at, _)| *at - from)
            .filter(|offset| (offset.x * normal.x + offset.y * normal.y).abs() < 1e-6)
            .map(|offset| offset.x * unit.x + offset.y * unit.y)
            .filter(|position| *position > half && *position < length - half)
            .collect();
        along.sort_by(f64::total_cmp);
        let at = |s: f64, out: f64| {
            Point::new(
                from.x + unit.x * s + normal.x * out,
                from.y + unit.y * s + normal.y * out,
            )
        };
        along
            .into_iter()
            .flat_map(|s| {
                [
                    at(s - half, 0.0),
                    at(s - half, t),
                    at(s + half, t),
                    at(s + half, 0.0),
                ]
            })
            .collect()
    };
    let mut previous_end: Option<Point> = None;
    let mut first_start = None;
    for (center, start) in corners {
        let corner_start = Point::new(center.x + r * start.cos(), center.y + r * start.sin());
        if let Some(end) = previous_end {
            points.extend(tab_on(end, corner_start));
        } else {
            first_start = Some(corner_start);
        }
        for step in 0..=steps {
            let angle = start + PI / 2.0 * step as f64 / steps as f64;
            points.push(Point::new(
                center.x + r * angle.cos(),
                center.y + r * angle.sin(),
            ));
        }
        previous_end = points.last().copied();
    }
    if let (Some(end), Some(start)) = (previous_end, first_start) {
        points.extend(tab_on(end, start));
    }
    vec![Path::new(points, true)]
}

pub fn generate(params: &RoundBoxParams) -> RoundBox {
    let layout = Layout::new(params);
    let length = layout.length();
    let t = params.thickness;
    let height = params.inner[2] + 2.0 * t + if params.lid { t } else { 0.0 };
    let tabs = layout.tabs(params.inner);
    let mut wall = vec![rectangle(0.0, 0.0, length, height)];
    // The bottom sits one thickness above the lower edge, the lid one
    // thickness below the top: slots at those heights.
    let mut levels = vec![t];
    if params.lid {
        levels.push(height - 2.0 * t);
    }
    for (along, _, _) in &tabs {
        for level in &levels {
            wall.push(rectangle(along - layout.tab / 2.0, *level, layout.tab, t));
        }
    }
    // Living hinge: staggered cuts across the strip in the bent parts,
    // leaving bridges so the strip holds together.
    let pitch = params.hinge_pitch.max(0.5);
    let bridge = 3.0f64.max(t);
    let cut = ((height - 3.0 * bridge) / 2.0).max(bridge);
    for (start, end) in layout.arcs() {
        let columns = ((end - start) / pitch).floor() as usize;
        for column in 0..=columns {
            let x = start + column as f64 * pitch;
            if x > end + 1e-9 {
                break;
            }
            // Even columns: two cuts with bridges at both edges and in
            // the middle. Odd columns: bridges facing the middle of
            // those cuts, cuts open to the edges.
            let segments: Vec<(f64, f64)> = if column % 2 == 0 {
                vec![
                    (bridge, bridge + cut),
                    (2.0 * bridge + cut, 2.0 * bridge + 2.0 * cut),
                ]
            } else {
                let first = bridge + cut / 2.0;
                let second = 2.0 * bridge + 1.5 * cut;
                vec![
                    (0.0, first - bridge / 2.0),
                    (first + bridge / 2.0, second - bridge / 2.0),
                    (second + bridge / 2.0, height),
                ]
            };
            for (from, to) in segments {
                let (from, to) = (from.max(0.0), to.min(height));
                if to - from > 0.5 {
                    wall.push(Path::new(
                        vec![Point::new(x, from), Point::new(x, to)],
                        false,
                    ));
                }
            }
        }
    }
    let bottom = panel(params, &layout, &tabs);
    let lid = params.lid.then(|| panel(params, &layout, &tabs));
    RoundBox {
        wall,
        bottom,
        lid,
        wall_length: length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_length_follows_the_middle_of_the_strip() {
        let params = RoundBoxParams::default();
        let made = generate(&params);
        let expected = 2.0 * (50.0 + 30.0) + 2.0 * PI * 16.5;
        assert!((made.wall_length - expected).abs() < 1e-9);
        let outline = made.wall[0].bounds();
        assert!((outline.width() - expected).abs() < 1e-9);
        // Bottom, lid, and room between them for the inside height.
        assert!((outline.height() - (40.0 + 9.0)).abs() < 1e-9);
    }

    #[test]
    fn hinge_cuts_stay_in_the_bent_parts_and_slots_match_tabs() {
        let params = RoundBoxParams::default();
        let layout = Layout::new(&params);
        let made = generate(&params);
        let cuts: Vec<&Path> = made.wall.iter().filter(|path| !path.closed).collect();
        assert!(!cuts.is_empty());
        for cut in cuts {
            let x = cut.points[0].x;
            assert!(
                layout
                    .arcs()
                    .iter()
                    .any(|(a, b)| x >= a - 1e-9 && x <= b + 1e-9),
                "cut at {x} outside the corners"
            );
        }
        let slots = made.wall.iter().filter(|path| path.closed).count() - 1;
        let tabs = layout.tabs(params.inner).len();
        assert_eq!(slots, tabs * 2);
        // Each tab adds four corners to the panel outline.
        let plain = 4 * 13;
        assert_eq!(made.bottom[0].points.len(), plain + 4 * tabs);
    }
}
