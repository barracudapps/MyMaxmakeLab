//! Orientation guide: which way up a model should be machined on a
//! 3-axis machine. The six poses (each face up) are compared on what a
//! tool coming from above can reach, how tall the part stands (deeper
//! is slower and needs longer tools) and how much flat base sits on the
//! table (holding).

use crate::{Triangle, bounds, rotate_quarter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Quarter turns to apply with `rotate_quarter`: (axis, count).
    pub turns: [(usize, usize); 2],
    /// Share of the surface reachable from above, 0..1.
    pub reachable: f64,
    /// Height of the part in this pose, model units.
    pub height: f64,
    /// Share of the footprint resting flat on the table, 0..1.
    pub base: f64,
    /// Higher is better.
    pub score: f64,
}

/// The six face-up poses: identity, X 90/180/270, Y 90/270.
const POSES: [[(usize, usize); 2]; 6] = [
    [(0, 0), (0, 0)],
    [(0, 1), (0, 0)],
    [(0, 2), (0, 0)],
    [(0, 3), (0, 0)],
    [(1, 1), (0, 0)],
    [(1, 3), (0, 0)],
];

pub fn apply(triangles: &mut [Triangle], turns: [(usize, usize); 2]) {
    for (axis, count) in turns {
        for _ in 0..count {
            rotate_quarter(triangles, axis);
        }
    }
}

fn area_and_normal(triangle: &Triangle) -> (f64, [f64; 3]) {
    let [a, b, c] = triangle.map(|vertex| vertex.map(f64::from));
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length < 1e-12 {
        return (0.0, [0.0; 3]);
    }
    (length / 2.0, normal.map(|value| value / length))
}

fn evaluate(triangles: &[Triangle], turns: [(usize, usize); 2]) -> Pose {
    let mut turned = triangles.to_vec();
    apply(&mut turned, turns);
    let (low, high) = bounds(&turned);
    let size = ((high[0] - low[0]).max(high[1] - low[1]) as f64).max(1e-6);
    // Top view on a coarse grid: highest surface over each cell.
    let cells = 96usize;
    let step = size / cells as f64;
    let placed: Vec<[[f64; 3]; 3]> = turned
        .iter()
        .map(|triangle| {
            triangle.map(|vertex| {
                [
                    (vertex[0] - low[0]) as f64,
                    (vertex[1] - low[1]) as f64,
                    (vertex[2] - low[2]) as f64,
                ]
            })
        })
        .collect();
    let top = crate::rasterize(
        placed.iter().copied(),
        [0.0, 0.0],
        step,
        cells + 1,
        cells + 1,
    );
    let height = (high[2] - low[2]) as f64;
    let tolerance = (height * 0.02).max(step);
    let mut total = 0.0;
    let mut reachable = 0.0;
    let mut base_area = 0.0;
    for (triangle, corners) in turned.iter().zip(&placed) {
        let (area, normal) = area_and_normal(triangle);
        total += area;
        let centre = [
            (corners[0][0] + corners[1][0] + corners[2][0]) / 3.0,
            (corners[0][1] + corners[1][1] + corners[2][1]) / 3.0,
            (corners[0][2] + corners[1][2] + corners[2][2]) / 3.0,
        ];
        if normal[2] < -0.98 && centre[2] < tolerance {
            // Resting on the table: nothing to machine there.
            base_area += area;
            total -= area;
            continue;
        }
        // Facing up (or vertical) and nothing above it where the tool
        // comes down: just outside a wall, on the surface otherwise.
        if normal[2] > -0.05 {
            let horizontal = normal[0].hypot(normal[1]);
            let reach = if horizontal > 0.5 {
                step * 1.5 / horizontal
            } else {
                0.0
            };
            let probe = [centre[0] + normal[0] * reach, centre[1] + normal[1] * reach];
            let column = (probe[0] / step).round();
            let row = (probe[1] / step).round();
            let outside = column < 0.0 || row < 0.0 || column > cells as f64 || row > cells as f64;
            let visible = outside
                || top[row as usize * (cells + 1) + column as usize]
                    .is_none_or(|z| centre[2] >= z - tolerance);
            if visible {
                reachable += area;
            }
        }
    }
    let footprint = top.iter().filter(|cell| cell.is_some()).count() as f64 * step * step;
    let reachable = if total > 0.0 { reachable / total } else { 0.0 };
    let base = if footprint > 0.0 {
        (base_area / footprint).min(1.0)
    } else {
        0.0
    };
    let largest = ((high[0] - low[0])
        .max(high[1] - low[1])
        .max(high[2] - low[2]) as f64)
        .max(1e-6);
    // Reach matters most, then a low part, then a stable base.
    let score = reachable * 0.65 + (1.0 - height / largest) * 0.2 + base * 0.15;
    Pose {
        turns,
        reachable,
        height,
        base,
        score,
    }
}

/// The six poses, best first.
pub fn rank_poses(triangles: &[Triangle]) -> Vec<Pose> {
    let mut poses: Vec<Pose> = POSES
        .iter()
        .map(|&turns| evaluate(triangles, turns))
        .collect();
    poses.sort_by(|a, b| b.score.total_cmp(&a.score));
    poses
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cuboid(min: [f32; 3], max: [f32; 3]) -> Vec<Triangle> {
        let v = |i: usize| -> [f32; 3] {
            [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ]
        };
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        quads
            .iter()
            .flat_map(|q| [[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]])
            .collect()
    }

    #[test]
    fn a_tall_post_is_laid_down_and_a_mushroom_stands_on_its_foot() {
        // A post 10 x 10 x 60 standing up: lying down is lower.
        let post = cuboid([0.0, 0.0, 0.0], [10.0, 10.0, 60.0]);
        let best = rank_poses(&post)[0];
        assert!(best.height < 15.0, "{best:?}");
        // A T shape (a cap on a stem): cap down leaves no undercut, cap up
        // hides the underside of the cap from the tool.
        let mut tee = cuboid([0.0, 0.0, 0.0], [40.0, 40.0, 5.0]);
        tee.extend(cuboid([15.0, 15.0, 5.0], [25.0, 25.0, 30.0]));
        let ranked = rank_poses(&tee);
        let best = ranked[0];
        let mut turned = tee.clone();
        apply(&mut turned, best.turns);
        let (low, _) = bounds(&turned);
        // The wide cap is at the bottom: its lower face rests on the table.
        let wide_at_bottom = turned
            .iter()
            .flatten()
            .filter(|vertex| (vertex[2] - low[2]).abs() < 1e-3)
            .any(|vertex| (vertex[0] - low[0]).abs() < 1e-3);
        assert!(wide_at_bottom, "{best:?}");
        assert!(best.reachable > 0.95 && best.base > 0.9, "{best:?}");
        let cap_up = ranked
            .iter()
            .find(|pose| pose.turns == [(0, 2), (0, 0)])
            .unwrap();
        assert!(cap_up.reachable < best.reachable);
    }
}
