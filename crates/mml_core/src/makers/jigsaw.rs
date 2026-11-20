//! 2D jigsaw puzzle: a grid of pieces with the classic round knobs,
//! each knob pointing a random way with slightly varied shape so no two
//! pieces are interchangeable. Cut lines run across the whole board
//! (one polyline per row and per column), which is what a laser wants.

use crate::geometry::{Path, Point};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JigsawParams {
    pub width: f64,
    pub height: f64,
    pub columns: usize,
    pub rows: usize,
    /// Knob size relative to the piece (0.1 to 0.3).
    pub knob: f64,
    pub seed: u64,
}

impl Default for JigsawParams {
    fn default() -> Self {
        Self {
            width: 200.0,
            height: 150.0,
            columns: 8,
            rows: 6,
            knob: 0.2,
            seed: 1,
        }
    }
}

struct Random(u64);

impl Random {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One edge with a knob, as points along `t` in [0, 1] (along the
/// edge) and `n` (across it, in edge lengths; positive is the knob
/// side).
fn knob_profile(random: &mut Random, size: f64) -> Vec<(f64, f64)> {
    let flip = if random.unit() < 0.5 { 1.0 } else { -1.0 };
    let centre = 0.5 + (random.unit() - 0.5) * 0.08;
    // Mushroom: flared base, narrow neck, round head wider than it.
    let radius = size * 0.75 * (0.9 + random.unit() * 0.2);
    let neck = radius * (0.5 + random.unit() * 0.1);
    let height = size * 1.2 * (1.0 + random.unit() * 0.15);
    let wobble = (random.unit() - 0.5) * size * 0.1;
    let control = [
        (0.0, 0.0),
        (centre - radius * 2.0, wobble),
        (centre - neck * 1.2, height * 0.05),
        (centre - neck, height * 0.3),
        (centre - radius, height * 0.62),
        (centre - radius * 0.7, height * 0.93),
        (centre, height),
        (centre + radius * 0.7, height * 0.93),
        (centre + radius, height * 0.62),
        (centre + neck, height * 0.3),
        (centre + neck * 1.2, height * 0.05),
        (centre + radius * 2.0, -wobble),
        (1.0, 0.0),
    ];
    // Catmull-Rom through the control points.
    let mut out = Vec::new();
    for index in 0..control.len() - 1 {
        let p0 = control[index.saturating_sub(1)];
        let p1 = control[index];
        let p2 = control[index + 1];
        let p3 = control[(index + 2).min(control.len() - 1)];
        for step in 0..8 {
            let s = step as f64 / 8.0;
            let blend = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b
                    + (-a + c) * s
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * s * s
                    + (-a + 3.0 * b - 3.0 * c + d) * s * s * s)
            };
            out.push((
                blend(p0.0, p1.0, p2.0, p3.0),
                flip * blend(p0.1, p1.1, p2.1, p3.1),
            ));
        }
    }
    out.push((1.0, 0.0));
    out
}

/// Cut lines: inner rows and columns with knobs, plus the border.
pub fn generate(params: &JigsawParams) -> Vec<Path> {
    let (columns, rows) = (params.columns.max(1), params.rows.max(1));
    let (piece_width, piece_height) = (params.width / columns as f64, params.height / rows as f64);
    let size = params.knob.clamp(0.1, 0.3);
    let mut random = Random(params.seed ^ 0x0015_A770);
    let mut paths = Vec::new();
    // Horizontal cuts between rows.
    for row in 1..rows {
        let y = row as f64 * piece_height;
        let mut points = Vec::new();
        for column in 0..columns {
            let x0 = column as f64 * piece_width;
            for (index, (t, n)) in knob_profile(&mut random, size).into_iter().enumerate() {
                if index == 0 && column > 0 {
                    continue;
                }
                points.push(Point::new(
                    x0 + t * piece_width,
                    y + n * piece_height.min(piece_width),
                ));
            }
        }
        paths.push(Path::new(points, false));
    }
    // Vertical cuts between columns.
    for column in 1..columns {
        let x = column as f64 * piece_width;
        let mut points = Vec::new();
        for row in 0..rows {
            let y0 = row as f64 * piece_height;
            for (index, (t, n)) in knob_profile(&mut random, size).into_iter().enumerate() {
                if index == 0 && row > 0 {
                    continue;
                }
                points.push(Point::new(
                    x + n * piece_height.min(piece_width),
                    y0 + t * piece_height,
                ));
            }
        }
        paths.push(Path::new(points, false));
    }
    paths.push(Path::new(
        vec![
            Point::new(0.0, 0.0),
            Point::new(params.width, 0.0),
            Point::new(params.width, params.height),
            Point::new(0.0, params.height),
        ],
        true,
    ));
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_span_the_board_and_knobs_stay_within_their_piece() {
        let params = JigsawParams::default();
        let paths = generate(&params);
        assert_eq!(paths.len(), (6 - 1) + (8 - 1) + 1);
        let (piece_width, piece_height) = (25.0, 25.0);
        for (index, path) in paths.iter().take(5).enumerate() {
            let y = (index + 1) as f64 * piece_height;
            assert!(
                (path.points[0].x).abs() < 1e-9
                    && (path.points.last().unwrap().x - 200.0).abs() < 1e-9
            );
            for point in &path.points {
                // A knob never reaches the next cut line.
                assert!((point.y - y).abs() < piece_height * 0.45, "{point:?}");
                assert!(point.x >= -1e-9 && point.x <= 200.0 + 1e-9);
            }
        }
        let _ = piece_width;
    }

    #[test]
    fn the_seed_changes_the_knobs() {
        let a = generate(&JigsawParams::default());
        let b = generate(&JigsawParams {
            seed: 9,
            ..JigsawParams::default()
        });
        assert_eq!(a, generate(&JigsawParams::default()));
        assert_ne!(a, b);
    }
}
