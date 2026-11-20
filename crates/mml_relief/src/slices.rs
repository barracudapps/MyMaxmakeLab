//! Stacked slices: a 3D model cut into layers as thick as the sheet,
//! each layer a flat part to cut, then stacked and glued in order (a 3D
//! puzzle, a topographic model, a sculpture from plywood).

use crate::{Triangle, bounds};

/// Closed outlines of each layer, bottom first, in model millimetres
/// scaled so the model is `width` wide (X); the lower-left corner of
/// the model footprint is at the origin.
pub fn slice_mesh(triangles: &[Triangle], width: f64, thickness: f64) -> Vec<Vec<Vec<[f64; 2]>>> {
    let (low, high) = bounds(triangles);
    let scale = width / ((high[0] - low[0]) as f64).max(1e-6);
    let height = (high[2] - low[2]) as f64 * scale;
    let thickness = thickness.max(0.1);
    let layers = (height / thickness).round().max(1.0) as usize;
    let place = |vertex: [f32; 3]| -> [f64; 3] {
        [
            (vertex[0] - low[0]) as f64 * scale,
            (vertex[1] - low[1]) as f64 * scale,
            (vertex[2] - low[2]) as f64 * scale,
        ]
    };
    let placed: Vec<[[f64; 3]; 3]> = triangles
        .iter()
        .map(|triangle| triangle.map(place))
        .collect();
    (0..layers)
        .map(|layer| {
            // Cut through the middle of the layer.
            let z = height * (layer as f64 + 0.5) / layers as f64;
            chain(section(&placed, z))
        })
        .collect()
}

/// Segments where the plane `z` crosses the triangles.
fn section(triangles: &[[[f64; 3]; 3]], z: f64) -> Vec<([f64; 2], [f64; 2])> {
    let mut segments = Vec::new();
    for triangle in triangles {
        let mut points = Vec::with_capacity(2);
        for index in 0..3 {
            let a = triangle[index];
            let b = triangle[(index + 1) % 3];
            // Half-open test so a vertex exactly on the plane counts once.
            if (a[2] > z) != (b[2] > z) {
                let t = (z - a[2]) / (b[2] - a[2]);
                points.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        if points.len() == 2 {
            segments.push((points[0], points[1]));
        }
    }
    segments
}

/// Joins segments sharing end points into closed loops.
fn chain(segments: Vec<([f64; 2], [f64; 2])>) -> Vec<Vec<[f64; 2]>> {
    use std::collections::HashMap;
    let key = |point: [f64; 2]| {
        (
            (point[0] * 1e4).round() as i64,
            (point[1] * 1e4).round() as i64,
        )
    };
    let mut by_start: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, segment) in segments.iter().enumerate() {
        by_start.entry(key(segment.0)).or_default().push(index);
        by_start.entry(key(segment.1)).or_default().push(index);
    }
    let mut used = vec![false; segments.len()];
    let mut loops = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut points = vec![segments[start].0, segments[start].1];
        loop {
            let end = key(*points.last().unwrap());
            let next = by_start
                .get(&end)
                .and_then(|candidates| candidates.iter().copied().find(|&index| !used[index]));
            let Some(next) = next else { break };
            used[next] = true;
            let (a, b) = segments[next];
            points.push(if key(a) == end { b } else { a });
        }
        if points.len() > 3 && key(points[0]) == key(*points.last().unwrap()) {
            points.pop();
            loops.push(points);
        }
    }
    loops
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Square pyramid 20 x 20 at the base, 10 high.
    fn pyramid() -> Vec<Triangle> {
        let apex = [10.0, 10.0, 10.0];
        let base = [
            [0.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [20.0, 20.0, 0.0],
            [0.0, 20.0, 0.0],
        ];
        let mut triangles: Vec<Triangle> = (0..4)
            .map(|index| [base[index], base[(index + 1) % 4], apex])
            .collect();
        triangles.push([base[0], base[2], base[1]]);
        triangles.push([base[0], base[3], base[2]]);
        triangles
    }

    #[test]
    fn a_pyramid_gives_shrinking_square_layers() {
        let layers = slice_mesh(&pyramid(), 20.0, 2.5);
        assert_eq!(layers.len(), 4);
        let widths: Vec<f64> = layers
            .iter()
            .map(|loops| {
                assert_eq!(loops.len(), 1, "one outline per layer");
                let xs: Vec<f64> = loops[0].iter().map(|point| point[0]).collect();
                xs.iter().copied().fold(f64::MIN, f64::max)
                    - xs.iter().copied().fold(f64::MAX, f64::min)
            })
            .collect();
        // Cut at 1.25, 3.75, 6.25, 8.75 mm: width 20 * (1 - z / 10).
        for (width, z) in widths.iter().zip([1.25, 3.75, 6.25, 8.75]) {
            assert!(
                (width - 20.0 * (1.0 - z / 10.0)).abs() < 1e-6,
                "{width} at {z}"
            );
        }
    }
}
