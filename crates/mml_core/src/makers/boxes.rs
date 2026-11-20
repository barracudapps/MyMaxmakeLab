//! Finger-jointed box sized for an object: give the inside dimensions
//! (the object plus some clearance) and the sheet thickness, get the
//! panels ready to cut, laid out flat.
//!
//! Joint scheme: the bottom (and the lid) sit inside the walls and push
//! tabs into slots of the walls; the front and back own the vertical
//! corners and receive the tabs of the sides. Every joint has an odd
//! number of fingers so its ends belong to the wall, which keeps all
//! corners consistent.

use crate::geometry::{Path, Point};
use crate::region;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxParams {
    /// Inside width (X), depth (Y) and height (Z), mm.
    pub inner: [f64; 3],
    pub thickness: f64,
    /// Target finger width; adjusted so each edge holds an odd count.
    pub finger: f64,
    pub lid: bool,
    /// Laser kerf: panels grow by half of it so joints stay tight.
    pub kerf: f64,
    pub spacing: f64,
}

impl Default for BoxParams {
    fn default() -> Self {
        Self {
            inner: [80.0, 60.0, 40.0],
            thickness: 3.0,
            finger: 10.0,
            lid: true,
            kerf: 0.1,
            spacing: 4.0,
        }
    }
}

pub struct Panel {
    pub name: &'static str,
    pub outline: Path,
}

/// Breakpoints of a finger edge of `length`: odd segments at `odd`
/// offset, even ones at `even`.
fn fingers(length: f64, finger: f64) -> (usize, f64) {
    let mut count = (length / finger.max(1.0)).round().max(1.0) as usize;
    if count.is_multiple_of(2) {
        count = if count > 1 { count - 1 } else { 1 };
    }
    (count, length / count as f64)
}

/// Appends an edge walked from `start` along `direction` (unit), with
/// segments of the inner region `[margin, margin + inner]` alternating
/// between offset 0 (even) and `odd_offset` along `normal` (odd).
#[allow(clippy::too_many_arguments)]
fn edge(
    points: &mut Vec<Point>,
    start: Point,
    direction: Point,
    normal: Point,
    margin: f64,
    inner: f64,
    finger: f64,
    odd_offset: f64,
) {
    let at = |along: f64, offset: f64| {
        Point::new(
            start.x + direction.x * along + normal.x * offset,
            start.y + direction.y * along + normal.y * offset,
        )
    };
    points.push(at(0.0, 0.0));
    let (count, size) = fingers(inner, finger);
    for index in 0..count {
        let offset = if index % 2 == 1 { odd_offset } else { 0.0 };
        points.push(at(margin + index as f64 * size, offset));
        points.push(at(margin + (index + 1) as f64 * size, offset));
    }
}

fn polygon(mut points: Vec<Point>) -> Path {
    points.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if points.len() > 1 && points.first().unwrap().distance(*points.last().unwrap()) < 1e-9 {
        points.pop();
    }
    // Drop points in the middle of straight runs.
    let mut cleaned: Vec<Point> = Vec::with_capacity(points.len());
    let count = points.len();
    for index in 0..count {
        let previous = points[(index + count - 1) % count];
        let here = points[index];
        let next = points[(index + 1) % count];
        if (here - previous).cross(next - here).abs() > 1e-9 {
            cleaned.push(here);
        }
    }
    Path::new(cleaned, true)
}

/// Panels of the box, each in its own coordinates.
pub fn panels(params: &BoxParams) -> Vec<Panel> {
    let t = params.thickness.max(0.1);
    let [w, d, h] = params.inner.map(|value| value.max(t));
    let f = params.finger;
    let lid_t = if params.lid { t } else { 0.0 };
    let (width, height) = (w + 2.0 * t, h + t + lid_t);
    let (right, up, left, down) = (
        Point::new(1.0, 0.0),
        Point::new(0.0, 1.0),
        Point::new(-1.0, 0.0),
        Point::new(0.0, -1.0),
    );

    // Bottom / lid: inner size, tabs pushed out on every edge.
    let plate = || {
        let mut points = Vec::new();
        edge(&mut points, Point::new(0.0, 0.0), right, down, 0.0, w, f, t);
        edge(&mut points, Point::new(w, 0.0), up, right, 0.0, d, f, t);
        edge(&mut points, Point::new(w, d), left, up, 0.0, w, f, t);
        edge(&mut points, Point::new(0.0, d), down, left, 0.0, d, f, t);
        polygon(points)
    };
    // Front / back: full width, slots for the bottom and lid, notches
    // for the side tabs.
    let wall = |top_slots: bool| {
        let mut points = Vec::new();
        edge(&mut points, Point::new(0.0, 0.0), right, up, t, w, f, t);
        points.push(Point::new(width, 0.0));
        edge(&mut points, Point::new(width, 0.0), up, left, t, h, f, t);
        points.push(Point::new(width, height));
        if top_slots {
            edge(
                &mut points,
                Point::new(width, height),
                left,
                down,
                t,
                w,
                f,
                t,
            );
        } else {
            points.push(Point::new(width, height));
        }
        points.push(Point::new(0.0, height));
        edge(
            &mut points,
            Point::new(0.0, height),
            down,
            right,
            height - t - h,
            h,
            f,
            t,
        );
        polygon(points)
    };
    // Sides: inner depth, slots for the bottom and lid, tabs into the
    // front and back.
    let side = |top_slots: bool| {
        let mut points = Vec::new();
        edge(&mut points, Point::new(0.0, 0.0), right, up, 0.0, d, f, t);
        points.push(Point::new(d, 0.0));
        edge(&mut points, Point::new(d, 0.0), up, right, t, h, f, t);
        points.push(Point::new(d, height));
        if top_slots {
            edge(&mut points, Point::new(d, height), left, down, 0.0, d, f, t);
        } else {
            points.push(Point::new(d, height));
        }
        points.push(Point::new(0.0, height));
        edge(
            &mut points,
            Point::new(0.0, height),
            down,
            left,
            height - t - h,
            h,
            f,
            t,
        );
        polygon(points)
    };
    let mut panels = vec![
        Panel {
            name: "bottom",
            outline: plate(),
        },
        Panel {
            name: "front",
            outline: wall(params.lid),
        },
        Panel {
            name: "back",
            outline: wall(params.lid),
        },
        Panel {
            name: "left",
            outline: side(params.lid),
        },
        Panel {
            name: "right",
            outline: side(params.lid),
        },
    ];
    if params.lid {
        panels.push(Panel {
            name: "lid",
            outline: plate(),
        });
    }
    if params.kerf > 0.0 {
        for panel in &mut panels {
            let grown = region::offset(
                &region::region_of(std::slice::from_ref(&panel.outline)),
                params.kerf / 2.0,
            );
            if let Some(polygon) = grown.0.first() {
                panel.outline = region::to_path(polygon.exterior());
            }
        }
    }
    panels
}

/// Panels laid out flat, lower-left at the origin, `spacing` apart.
pub fn layout(params: &BoxParams, max_width: f64) -> Vec<Panel> {
    let mut panels = panels(params);
    let bounds: Vec<_> = panels.iter().map(|panel| panel.outline.bounds()).collect();
    let sizes: Vec<(f64, f64)> = bounds
        .iter()
        .map(|rect| (rect.width(), rect.height()))
        .collect();
    let (corners, _) = crate::attach::shelf_pack(&sizes, max_width, params.spacing);
    for ((panel, rect), corner) in panels.iter_mut().zip(&bounds).zip(&corners) {
        for point in &mut panel.outline.points {
            point.x += corner.x - rect.min.x;
            point.y += corner.y - rect.min.y;
        }
    }
    panels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(path: &Path) -> f64 {
        let count = path.points.len();
        (0..count)
            .map(|index| path.points[index].cross(path.points[(index + 1) % count]))
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn panels_have_the_right_size_and_joints_add_up() {
        let params = BoxParams {
            inner: [80.0, 60.0, 40.0],
            thickness: 3.0,
            finger: 10.0,
            lid: true,
            kerf: 0.0,
            spacing: 4.0,
        };
        let panels = panels(&params);
        assert_eq!(panels.len(), 6);
        let bottom = panels[0].outline.bounds();
        assert!((bottom.width() - 86.0).abs() < 1e-9 && (bottom.height() - 66.0).abs() < 1e-9);
        let front = panels[1].outline.bounds();
        assert!((front.width() - 86.0).abs() < 1e-9 && (front.height() - 46.0).abs() < 1e-9);
        let side = panels[3].outline.bounds();
        assert!((side.width() - 66.0).abs() < 1e-9 && (side.height() - 46.0).abs() < 1e-9);
        // The material of all panels equals the box shell volume divided
        // by the thickness: tabs and slots exchange area exactly.
        let total: f64 = panels.iter().map(|panel| area(&panel.outline)).sum();
        let (outer, inner) = (86.0 * 66.0 * 46.0, 80.0 * 60.0 * 40.0);
        assert!((total * 3.0 - (outer - inner)).abs() < 1e-6, "{total}");
    }

    #[test]
    fn an_open_box_has_five_panels_and_a_plain_top_edge() {
        let params = BoxParams {
            lid: false,
            kerf: 0.0,
            ..BoxParams::default()
        };
        let panels = panels(&params);
        assert_eq!(panels.len(), 5);
        let total: f64 = panels.iter().map(|panel| area(&panel.outline)).sum();
        let (outer, inner) = (86.0 * 66.0 * 43.0, 80.0 * 60.0 * 40.0);
        assert!((total * 3.0 - (outer - inner)).abs() < 1e-6, "{total}");
    }

    #[test]
    fn kerf_grows_panels_and_layout_keeps_them_apart() {
        let params = BoxParams::default();
        let tight = panels(&BoxParams {
            kerf: 0.0,
            ..params
        });
        let grown = panels(&params);
        assert!(area(&grown[0].outline) > area(&tight[0].outline));
        let laid = layout(&params, 300.0);
        let rects: Vec<_> = laid.iter().map(|panel| panel.outline.bounds()).collect();
        for a in 0..rects.len() {
            for b in a + 1..rects.len() {
                let apart = rects[a].max.x <= rects[b].min.x
                    || rects[b].max.x <= rects[a].min.x
                    || rects[a].max.y <= rects[b].min.y
                    || rects[b].max.y <= rects[a].min.y;
                assert!(apart, "{} and {} overlap", laid[a].name, laid[b].name);
            }
        }
    }
}
