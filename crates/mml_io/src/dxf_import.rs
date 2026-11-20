use crate::{ImportError, ImportedGroup, TOLERANCE_MM};
use dxf::entities::EntityType;
use dxf::enums::Units;
use mml_core::geometry::{Path, Point, arc_segments};
use std::f64::consts::TAU;

const JOIN_TOLERANCE: f64 = 0.01;

pub fn import_dxf(data: &[u8]) -> Result<Vec<ImportedGroup>, ImportError> {
    let mut reader = std::io::Cursor::new(data);
    let drawing =
        dxf::Drawing::load(&mut reader).map_err(|error| ImportError::Dxf(error.to_string()))?;
    let scale = match drawing.header.default_drawing_units {
        Units::Inches => 25.4,
        Units::Feet => 304.8,
        Units::Centimeters => 10.0,
        Units::Meters => 1000.0,
        _ => 1.0,
    };
    let mut closed = Vec::new();
    let mut open = Vec::new();
    for entity in drawing.entities() {
        for path in entity_paths(&entity.specific) {
            if path.closed {
                closed.push(path)
            } else {
                open.push(path)
            }
        }
    }
    closed.extend(join_chains(open));
    let mut paths: Vec<Path> = closed
        .into_iter()
        .map(|path| {
            Path::new(
                path.points.into_iter().map(|point| point * scale).collect(),
                path.closed,
            )
        })
        .filter(|path| path.points.len() >= 2)
        .collect();
    if paths.is_empty() {
        return Err(ImportError::Empty);
    }
    let bounds = mml_core::geometry::bounds_of(&paths);
    for path in &mut paths {
        for point in &mut path.points {
            *point = *point - bounds.min;
        }
    }
    Ok(vec![ImportedGroup { color: None, paths }])
}

fn mirrored(normal_z: f64, point: Point) -> Point {
    // Entities with an extrusion of -Z are drawn mirrored in X.
    if normal_z < 0.0 {
        Point::new(-point.x, point.y)
    } else {
        point
    }
}

fn arc_points(center: Point, radius: f64, start: f64, sweep: f64) -> Vec<Point> {
    let count = arc_segments(radius, sweep, TOLERANCE_MM);
    (0..=count)
        .map(|index| {
            let angle = start + sweep * index as f64 / count as f64;
            center + Point::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

fn bulge_points(from: Point, to: Point, bulge: f64) -> Vec<Point> {
    if bulge.abs() < 1e-9 {
        return vec![to];
    }
    let sweep = 4.0 * bulge.atan();
    let chord = to - from;
    let length = chord.length();
    let radius = length / (2.0 * (sweep / 2.0).sin()).abs();
    let middle = from.lerp(to, 0.5);
    let offset = radius * (sweep / 2.0).cos();
    let normal = chord.normalized().perpendicular();
    let center = middle + normal * (offset * sweep.signum());
    let start = (from.y - center.y).atan2(from.x - center.x);
    let mut points = arc_points(center, radius.abs(), start, sweep);
    points.remove(0);
    if let Some(last) = points.last_mut() {
        *last = to;
    }
    points
}

fn polyline(vertices: &[(Point, f64)], closed: bool) -> Path {
    let mut points = Vec::new();
    if let Some((first, _)) = vertices.first() {
        points.push(*first);
    }
    for pair in vertices.windows(2) {
        points.extend(bulge_points(pair[0].0, pair[1].0, pair[0].1));
    }
    if closed && vertices.len() > 1 {
        let (last, bulge) = *vertices.last().unwrap();
        points.extend(bulge_points(last, vertices[0].0, bulge));
    }
    Path::new(points, closed).cleaned(1e-9)
}

fn entity_paths(entity: &EntityType) -> Vec<Path> {
    match entity {
        EntityType::Line(line) => vec![Path::new(
            vec![
                Point::new(line.p1.x, line.p1.y),
                Point::new(line.p2.x, line.p2.y),
            ],
            false,
        )],
        EntityType::Circle(circle) => {
            let center = mirrored(
                circle.normal.z,
                Point::new(circle.center.x, circle.center.y),
            );
            let mut points = arc_points(center, circle.radius, 0.0, TAU);
            points.pop();
            vec![Path::new(points, true)]
        }
        EntityType::Arc(arc) => {
            let mut sweep = (arc.end_angle - arc.start_angle).to_radians();
            if sweep <= 0.0 {
                sweep += TAU;
            }
            let center = Point::new(arc.center.x, arc.center.y);
            let points = arc_points(center, arc.radius, arc.start_angle.to_radians(), sweep);
            vec![Path::new(
                points
                    .into_iter()
                    .map(|point| mirrored(arc.normal.z, point))
                    .collect(),
                false,
            )]
        }
        EntityType::Ellipse(ellipse) => {
            let center = Point::new(ellipse.center.x, ellipse.center.y);
            let major = Point::new(ellipse.major_axis.x, ellipse.major_axis.y);
            let minor = major.perpendicular() * ellipse.minor_axis_ratio;
            let mut sweep = ellipse.end_parameter - ellipse.start_parameter;
            if sweep <= 0.0 {
                sweep += TAU;
            }
            let full = (sweep - TAU).abs() < 1e-6;
            let count = arc_segments(major.length(), sweep, TOLERANCE_MM);
            let mut points: Vec<Point> = (0..=count)
                .map(|index| {
                    let t = ellipse.start_parameter + sweep * index as f64 / count as f64;
                    mirrored(ellipse.normal.z, center + major * t.cos() + minor * t.sin())
                })
                .collect();
            if full {
                points.pop();
            }
            vec![Path::new(points, full)]
        }
        EntityType::LwPolyline(poly) => {
            let vertices: Vec<(Point, f64)> = poly
                .vertices
                .iter()
                .map(|vertex| {
                    (
                        mirrored(poly.extrusion_direction.z, Point::new(vertex.x, vertex.y)),
                        vertex.bulge,
                    )
                })
                .collect();
            vec![polyline(&vertices, poly.is_closed())]
        }
        EntityType::Polyline(poly) => {
            let vertices: Vec<(Point, f64)> = poly
                .vertices()
                .map(|vertex| {
                    (
                        Point::new(vertex.location.x, vertex.location.y),
                        vertex.bulge,
                    )
                })
                .collect();
            vec![polyline(&vertices, poly.is_closed())]
        }
        EntityType::Spline(spline) => spline_path(spline).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Rational B-spline evaluated with de Boor's algorithm.
fn spline_path(spline: &dxf::entities::Spline) -> Option<Path> {
    let degree = spline.degree_of_curve.max(1) as usize;
    let controls: Vec<Point> = spline
        .control_points
        .iter()
        .map(|point| Point::new(point.x, point.y))
        .collect();
    if controls.len() <= degree {
        let fit: Vec<Point> = spline
            .fit_points
            .iter()
            .map(|point| Point::new(point.x, point.y))
            .collect();
        return (fit.len() >= 2).then(|| Path::new(fit, spline.is_closed()));
    }
    let knots = &spline.knot_values;
    if knots.len() != controls.len() + degree + 1 {
        return Some(Path::new(controls, spline.is_closed()));
    }
    let weights: Vec<f64> = if spline.weight_values.len() == controls.len() {
        spline.weight_values.clone()
    } else {
        vec![1.0; controls.len()]
    };
    let (start, end) = (knots[degree], knots[controls.len()]);
    let samples = (controls.len() * 24).clamp(16, 4000);
    let points = (0..=samples)
        .map(|index| {
            let t = start + (end - start) * index as f64 / samples as f64;
            de_boor(
                t.min(end - 1e-12).max(start),
                degree,
                knots,
                &controls,
                &weights,
            )
        })
        .collect();
    Some(Path::new(points, spline.is_closed()).cleaned(1e-9))
}

fn de_boor(t: f64, degree: usize, knots: &[f64], controls: &[Point], weights: &[f64]) -> Point {
    let span = (degree..controls.len())
        .rfind(|&index| knots[index] <= t)
        .unwrap_or(degree);
    let mut points: Vec<(Point, f64)> = (0..=degree)
        .map(|j| {
            let index = span - degree + j;
            (controls[index] * weights[index], weights[index])
        })
        .collect();
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let index = span - degree + j;
            let denominator = knots[index + degree + 1 - r] - knots[index];
            let alpha = if denominator.abs() < 1e-12 {
                0.0
            } else {
                (t - knots[index]) / denominator
            };
            let (a, wa) = points[j - 1];
            let (b, wb) = points[j];
            points[j] = (
                a * (1.0 - alpha) + b * alpha,
                wa * (1.0 - alpha) + wb * alpha,
            );
        }
    }
    let (point, weight) = points[degree];
    point * (1.0 / weight)
}

/// Joins open paths whose ends touch, which is how most CAD exports
/// store outlines (one LINE or ARC per edge).
fn join_chains(mut open: Vec<Path>) -> Vec<Path> {
    let mut result = Vec::new();
    while let Some(mut chain) = open.pop() {
        loop {
            let head = chain.points[0];
            let tail = *chain.points.last().unwrap();
            let found = open.iter().position(|path| {
                let (a, b) = (path.points[0], *path.points.last().unwrap());
                [a, b].iter().any(|end| {
                    end.distance(tail) <= JOIN_TOLERANCE || end.distance(head) <= JOIN_TOLERANCE
                })
            });
            let Some(index) = found else { break };
            let mut next = open.swap_remove(index);
            let (a, b) = (next.points[0], *next.points.last().unwrap());
            if a.distance(tail) <= JOIN_TOLERANCE {
                chain.points.extend(next.points.drain(1..));
            } else if b.distance(tail) <= JOIN_TOLERANCE {
                next.points.reverse();
                chain.points.extend(next.points.drain(1..));
            } else if b.distance(head) <= JOIN_TOLERANCE {
                next.points.pop();
                next.points.append(&mut chain.points);
                chain.points = next.points;
            } else {
                next.points.reverse();
                next.points.pop();
                next.points.append(&mut chain.points);
                chain.points = next.points;
            }
        }
        let closes = chain.points.len() > 2
            && chain.points[0].distance(*chain.points.last().unwrap()) <= JOIN_TOLERANCE;
        chain.closed = closes;
        result.push(chain.cleaned(JOIN_TOLERANCE / 10.0));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_lines_are_joined_into_a_closed_outline() {
        let segment = |a: (f64, f64), b: (f64, f64)| {
            Path::new(vec![Point::new(a.0, a.1), Point::new(b.0, b.1)], false)
        };
        let joined = join_chains(vec![
            segment((0.0, 0.0), (10.0, 0.0)),
            segment((10.0, 10.0), (10.0, 0.0)),
            segment((10.0, 10.0), (0.0, 10.0)),
            segment((0.0, 0.0), (0.0, 10.0)),
        ]);
        assert_eq!(joined.len(), 1);
        assert!(joined[0].closed);
        assert!((joined[0].signed_area().abs() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn half_circle_bulge_has_the_right_radius() {
        let points = bulge_points(Point::new(0.0, 0.0), Point::new(10.0, 0.0), 1.0);
        let center = Point::new(5.0, 0.0);
        assert!(
            points
                .iter()
                .all(|point| (point.distance(center) - 5.0).abs() < 1e-6)
        );
    }
}
