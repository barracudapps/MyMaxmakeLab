use geo::{MultiPolygon, Polygon};
use mml_core::geometry::{Path, Point};
use mml_core::layer::{CncPocketParams, CncProfileParams, ProfileSide};
use mml_core::ordering::order_paths;
use mml_core::region::{offset, region_of, to_path};
use mml_gcode::GcodeWriter;

#[derive(Debug, Clone, Copy)]
pub struct CncSettings {
    /// Rapid height above the stock top.
    pub safe_z: f64,
}

/// Depth of each pass, the last one exactly at `depth`.
pub fn pass_depths(depth: f64, pass_depth: f64) -> Vec<f64> {
    if depth <= 0.0 {
        return Vec::new();
    }
    let passes = (depth / pass_depth.max(0.01) - 1e-9).ceil().max(1.0) as usize;
    (1..=passes)
        .map(|pass| depth * pass as f64 / passes as f64)
        .collect()
}

/// Orients a ring so that the finished wall lies on the right of the
/// direction of travel (climb milling with a clockwise spindle) or on
/// the left (conventional).
fn orient(mut path: Path, wall_inside: bool, climb: bool) -> Path {
    let counter_clockwise = path.signed_area() > 0.0;
    // Wall inside the ring is on the left when running counter-clockwise.
    let wall_on_right = wall_inside != counter_clockwise;
    if wall_on_right != climb {
        path.points.reverse();
    }
    path
}

fn polygon_rings(polygon: &Polygon<f64>) -> (Path, Vec<Path>) {
    (
        to_path(polygon.exterior()),
        polygon.interiors().iter().map(to_path).collect(),
    )
}

pub fn emit_profile(
    writer: &mut GcodeWriter,
    head: &mut Point,
    shapes: Vec<Vec<Path>>,
    params: &CncProfileParams,
    settings: &CncSettings,
) {
    let radius = params.tool_diameter / 2.0;
    let mut rings: Vec<Path> = Vec::new();
    for paths in &shapes {
        match params.side {
            ProfileSide::On => {
                rings.extend(paths.iter().filter(|path| path.points.len() >= 2).cloned())
            }
            ProfileSide::Outside | ProfileSide::Inside => {
                let outside = params.side == ProfileSide::Outside;
                let distance = if outside { radius } else { -radius };
                for polygon in offset(&region_of(paths), distance) {
                    let (exterior, interiors) = polygon_rings(&polygon);
                    rings.push(orient(exterior, outside, params.climb));
                    rings.extend(
                        interiors
                            .into_iter()
                            .map(|ring| orient(ring, !outside, params.climb)),
                    );
                }
            }
        }
    }
    if rings.is_empty() {
        return;
    }
    let depths = pass_depths(params.depth, params.pass_depth);
    let tab_top = -(params.depth - params.tab_height.clamp(0.0, params.depth));
    writer.spindle_on(params.spindle);
    writer.rapid_z(settings.safe_z);
    for ring in order_paths(rings, *head) {
        let tabs = if params.tabs > 0 && ring.closed {
            tab_intervals(&ring, params.tabs, params.tab_width + params.tool_diameter)
        } else {
            Vec::new()
        };
        let start = ring.points[0];
        writer.rapid(start.x, start.y);
        for &depth in &depths {
            let z = -depth;
            writer.linear_z(z, params.plunge);
            let z_floor = |s: f64| {
                if z < tab_top && tabs.iter().any(|(from, to)| s >= *from && s <= *to) {
                    tab_top
                } else {
                    z
                }
            };
            cut_ring(
                writer,
                &ring,
                z,
                &tabs,
                &z_floor,
                params.feed,
                params.plunge,
            );
            if !ring.closed {
                writer.rapid_z(settings.safe_z);
                writer.rapid(start.x, start.y);
            }
        }
        writer.rapid_z(settings.safe_z);
        *head = start;
    }
    writer.tool_off();
}

/// Evenly spaced tab intervals along the ring, as arc-length ranges.
fn tab_intervals(ring: &Path, count: u32, width: f64) -> Vec<(f64, f64)> {
    let length = ring.length();
    if length <= width * count as f64 * 2.0 {
        return Vec::new();
    }
    (0..count)
        .map(|index| {
            let center = length * (index as f64 + 0.5) / count as f64;
            (center - width / 2.0, center + width / 2.0)
        })
        .collect()
}

fn cut_ring(
    writer: &mut GcodeWriter,
    ring: &Path,
    z: f64,
    tabs: &[(f64, f64)],
    z_at: &dyn Fn(f64) -> f64,
    feed: f64,
    plunge: f64,
) {
    let mut travelled = 0.0;
    let mut current_z = z;
    for (a, b) in ring.segments() {
        let length = a.distance(b);
        // Split the segment where it enters or leaves a tab.
        let mut cuts: Vec<f64> = tabs
            .iter()
            .flat_map(|(from, to)| [*from, *to])
            .filter(|s| *s > travelled && *s < travelled + length)
            .collect();
        cuts.sort_by(f64::total_cmp);
        cuts.push(travelled + length);
        let mut previous = travelled;
        for s in cuts {
            let middle = (previous + s) / 2.0;
            let target_z = z_at(middle);
            if (target_z - current_z).abs() > 1e-9 {
                writer.linear_z(target_z, plunge);
                current_z = target_z;
            }
            let point = a.lerp(b, (s - travelled) / length.max(1e-12));
            writer.linear(point.x, point.y, feed, None);
            previous = s;
        }
        travelled += length;
    }
    if (current_z - z).abs() > 1e-9 {
        writer.linear_z(z, plunge);
    }
}

pub fn emit_pocket(
    writer: &mut GcodeWriter,
    head: &mut Point,
    shapes: Vec<Vec<Path>>,
    params: &CncPocketParams,
    settings: &CncSettings,
) {
    let radius = params.tool_diameter / 2.0;
    let step = params.tool_diameter * params.stepover.clamp(0.05, 0.95);
    let mut pockets: Vec<Vec<Path>> = Vec::new();
    for paths in &shapes {
        for area in offset(&region_of(paths), -radius) {
            let mut rings = Vec::new();
            let mut level = MultiPolygon::new(vec![area]);
            while !level.0.is_empty() && rings.len() < 10_000 {
                for polygon in &level {
                    let (exterior, interiors) = polygon_rings(polygon);
                    rings.push(orient(exterior, false, params.climb));
                    rings.extend(
                        interiors
                            .into_iter()
                            .map(|ring| orient(ring, true, params.climb)),
                    );
                }
                level = offset(&level, -step);
            }
            // Innermost first: start in the middle and finish on the wall.
            rings.reverse();
            pockets.push(rings);
        }
    }
    if pockets.is_empty() {
        return;
    }
    let depths = pass_depths(params.depth, params.pass_depth);
    writer.spindle_on(params.spindle);
    writer.rapid_z(settings.safe_z);
    for rings in pockets {
        for &depth in &depths {
            let mut position: Option<Point> = None;
            for ring in &rings {
                let ring = nearest_start(ring, position.unwrap_or(*head));
                let start = ring.points[0];
                let linked =
                    position.is_some_and(|point| point.distance(start) <= params.tool_diameter);
                if !linked {
                    writer.rapid_z(settings.safe_z);
                    writer.rapid(start.x, start.y);
                    writer.linear_z(-depth, params.plunge);
                } else {
                    writer.linear(start.x, start.y, params.feed, None);
                }
                for point in ring.points.iter().skip(1).chain(std::iter::once(&start)) {
                    writer.linear(point.x, point.y, params.feed, None);
                }
                position = Some(start);
            }
            if let Some(point) = position {
                *head = point;
            }
        }
        writer.rapid_z(settings.safe_z);
    }
    writer.tool_off();
}

fn nearest_start(ring: &Path, from: Point) -> Path {
    let index = ring
        .points
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.distance(from).total_cmp(&b.1.distance(from)))
        .map_or(0, |(index, _)| index);
    let mut ring = ring.clone();
    ring.points.rotate_left(index);
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64) -> Path {
        Path::new(
            vec![
                Point::new(0.0, 0.0),
                Point::new(size, 0.0),
                Point::new(size, size),
                Point::new(0.0, size),
            ],
            true,
        )
    }

    fn settings() -> CncSettings {
        CncSettings { safe_z: 5.0 }
    }

    #[test]
    fn pass_depths_end_exactly_at_depth() {
        assert_eq!(pass_depths(1.0, 0.4), vec![1.0 / 3.0, 2.0 / 3.0, 1.0]);
        assert_eq!(pass_depths(1.0, 0.5), vec![0.5, 1.0]);
    }

    #[test]
    fn outside_profile_clears_the_part_by_the_tool_radius() {
        let params = CncProfileParams {
            tool_diameter: 2.0,
            depth: 1.0,
            pass_depth: 1.0,
            ..Default::default()
        };
        let mut writer = GcodeWriter::new();
        emit_profile(
            &mut writer,
            &mut Point::default(),
            vec![vec![square(10.0)]],
            &params,
            &settings(),
        );
        let bounds = mml_gcode::parse(&writer.finish()).cut_bounds();
        assert!((bounds.min.x + 1.0).abs() < 1e-6 && (bounds.max.x - 11.0).abs() < 1e-6);
    }

    #[test]
    fn climb_outside_profile_runs_clockwise() {
        let ring = orient(square(10.0), true, true);
        assert!(ring.signed_area() < 0.0);
    }

    #[test]
    fn tabs_lift_the_tool_on_the_last_pass() {
        let params = CncProfileParams {
            tool_diameter: 2.0,
            depth: 3.0,
            pass_depth: 1.5,
            tabs: 4,
            tab_height: 1.0,
            ..Default::default()
        };
        let mut writer = GcodeWriter::new();
        emit_profile(
            &mut writer,
            &mut Point::default(),
            vec![vec![square(40.0)]],
            &params,
            &settings(),
        );
        let program = mml_gcode::parse(&writer.finish());
        let at_tab_top = program
            .motions
            .iter()
            .filter(|motion| (motion.to[2] + 2.0).abs() < 1e-9 && motion.from[2] == motion.to[2])
            .count();
        assert!(at_tab_top >= 4);
    }

    #[test]
    fn pocket_stays_inside_the_wall() {
        let params = CncPocketParams {
            tool_diameter: 2.0,
            depth: 1.0,
            pass_depth: 1.0,
            ..Default::default()
        };
        let mut writer = GcodeWriter::new();
        emit_pocket(
            &mut writer,
            &mut Point::default(),
            vec![vec![square(20.0)]],
            &params,
            &settings(),
        );
        let bounds = mml_gcode::parse(&writer.finish()).cut_bounds();
        assert!((bounds.min.x - 1.0).abs() < 1e-6 && (bounds.max.y - 19.0).abs() < 1e-6);
    }
}
