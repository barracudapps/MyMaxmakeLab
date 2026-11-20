use mml_core::document::RasterImage;
use mml_core::geometry::{Path, Point};
use std::collections::HashMap;

/// Vectorises the dark areas of a picture (pixels below `threshold`).
/// Paths are returned in the local frame of an image shape of
/// `width` x `height` mm, so the traced shape can reuse the picture
/// transform. `smoothing` is the simplification tolerance in pixels.
pub fn trace_image(
    image: &RasterImage,
    threshold: u8,
    smoothing: f64,
    width: f64,
    height: f64,
) -> Vec<Path> {
    let (w, h) = (image.width as i64, image.height as i64);
    let dark = |x: i64, y: i64| {
        x >= 0 && y >= 0 && x < w && y < h && image.pixel(x as u32, y as u32) < threshold
    };
    // Boundary edges oriented clockwise around dark pixels (image axes,
    // Y down), keyed by their start vertex.
    let mut outgoing: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            if !dark(x, y) {
                continue;
            }
            let mut edge =
                |from: (i64, i64), to: (i64, i64)| outgoing.entry(from).or_default().push(to);
            if !dark(x, y - 1) {
                edge((x, y), (x + 1, y));
            }
            if !dark(x + 1, y) {
                edge((x + 1, y), (x + 1, y + 1));
            }
            if !dark(x, y + 1) {
                edge((x + 1, y + 1), (x, y + 1));
            }
            if !dark(x - 1, y) {
                edge((x, y + 1), (x, y));
            }
        }
    }
    let mut loops = Vec::new();
    while let Some((&start, _)) = outgoing.iter().find(|(_, targets)| !targets.is_empty()) {
        let mut ring = vec![start];
        let mut previous = start;
        let mut current = take_edge(&mut outgoing, start, None);
        while let Some(vertex) = current {
            if vertex == start {
                break;
            }
            ring.push(vertex);
            let incoming = (vertex.0 - previous.0, vertex.1 - previous.1);
            previous = vertex;
            current = take_edge(&mut outgoing, vertex, Some(incoming));
        }
        if ring.len() >= 4 {
            loops.push(ring);
        }
    }
    let to_local = |(x, y): (i64, i64)| {
        Point::new(
            (x as f64 / w as f64 - 0.5) * width,
            (0.5 - y as f64 / h as f64) * height,
        )
    };
    loops
        .into_iter()
        .map(|ring| {
            let points: Vec<Point> = ring
                .iter()
                .map(|&(x, y)| Point::new(x as f64, y as f64))
                .collect();
            let simplified = simplify_closed(&points, smoothing.max(0.0));
            let local = simplified
                .into_iter()
                .map(|point| {
                    to_local((0, 0))
                        + Point::new(point.x / w as f64 * width, -point.y / h as f64 * height)
                })
                .collect();
            Path::new(local, true)
        })
        .filter(|path| path.points.len() >= 3)
        .collect()
}

fn take_edge(
    outgoing: &mut HashMap<(i64, i64), Vec<(i64, i64)>>,
    vertex: (i64, i64),
    incoming: Option<(i64, i64)>,
) -> Option<(i64, i64)> {
    let targets = outgoing.get_mut(&vertex)?;
    if targets.is_empty() {
        return None;
    }
    // At a saddle, turn right (image axes) so diagonal pixels stay apart.
    let index = match incoming {
        Some((dx, dy)) if targets.len() > 1 => targets
            .iter()
            .position(|&(x, y)| (x - vertex.0, y - vertex.1) == (-dy, dx))
            .unwrap_or(0),
        _ => 0,
    };
    Some(targets.swap_remove(index))
}

fn simplify_closed(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() < 4 || tolerance <= 0.0 {
        return points.to_vec();
    }
    // Split at the vertex farthest from the first one and simplify both
    // halves as open polylines.
    let far = (1..points.len())
        .max_by(|a, b| {
            points[*a]
                .distance(points[0])
                .total_cmp(&points[*b].distance(points[0]))
        })
        .unwrap();
    let mut first: Vec<Point> = points[..=far].to_vec();
    let mut second: Vec<Point> = points[far..].to_vec();
    second.push(points[0]);
    first = douglas_peucker(&first, tolerance);
    second = douglas_peucker(&second, tolerance);
    first.pop();
    second.pop();
    first.extend(second);
    first
}

fn douglas_peucker(points: &[Point], tolerance: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (first, last) = (points[0], *points.last().unwrap());
    let (index, distance) = points[1..points.len() - 1]
        .iter()
        .enumerate()
        .map(|(index, point)| {
            (
                index + 1,
                mml_core::geometry::distance_to_segment(*point, first, last),
            )
        })
        .fold(
            (0, 0.0),
            |best, item| if item.1 > best.1 { item } else { best },
        );
    if distance <= tolerance {
        return vec![first, last];
    }
    let mut left = douglas_peucker(&points[..=index], tolerance);
    let right = douglas_peucker(&points[index..], tolerance);
    left.pop();
    left.extend(right);
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_with_hole_gives_two_rings() {
        // 6x6 dark square with a 2x2 light hole in the middle.
        let mut pixels = vec![0u8; 36];
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            pixels[y * 6 + x] = 255;
        }
        let image = RasterImage {
            width: 6,
            height: 6,
            pixels,
        };
        let paths = trace_image(&image, 128, 0.1, 6.0, 6.0);
        assert_eq!(paths.len(), 2);
        let mut areas: Vec<f64> = paths.iter().map(|path| path.signed_area().abs()).collect();
        areas.sort_by(f64::total_cmp);
        assert!((areas[0] - 4.0).abs() < 1e-9 && (areas[1] - 36.0).abs() < 1e-9);
    }
}
