//! 2D jigsaw puzzle: a grid of pieces with the classic round knobs,
//! each knob pointing a random way with slightly varied shape so no two
//! pieces are interchangeable. Cut lines run across the whole board
//! (one polyline per row and per column), which is what a laser wants.

use crate::geometry::{Path, Point, bounds_of};

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

/// A puzzle inside a shape: the grid of `params` (its size is the
/// shape's) laid over the shape's closed outlines, cut lines kept only
/// inside, and the outlines as the border. `None` without a closed
/// outline. Bits of cut shorter than 1 mm, left where a knob grazes the
/// border, are dropped.
pub fn generate_inside(outline: &[Path], params: &JigsawParams) -> Option<Vec<Path>> {
    let border: Vec<Path> = outline
        .iter()
        .filter(|path| path.closed && path.points.len() > 2)
        .cloned()
        .collect();
    let bounds = bounds_of(&border);
    if border.is_empty() || bounds.width() < 1.0 || bounds.height() < 1.0 {
        return None;
    }
    let (columns, rows) = (params.columns.max(1), params.rows.max(1));
    let piece = (
        bounds.width() / columns as f64,
        bounds.height() / rows as f64,
    );
    // One piece more each way, so the grid still covers the shape once
    // shifted by up to a piece.
    let grid = JigsawParams {
        width: bounds.width() + piece.0,
        height: bounds.height() + piece.1,
        columns: columns + 1,
        rows: rows + 1,
        ..*params
    };
    let mut cuts = generate(&grid);
    // The last path is the rectangular border: the shape replaces it.
    cuts.pop();
    let area = crate::region::region_of(&border);
    let raster = Raster::new(&border, bounds);
    // The grid position whose smallest piece is the largest: pieces
    // barely touching the border become rare.
    let shifted = |fx: f64, fy: f64| -> Vec<Path> {
        let (dx, dy) = (bounds.min.x - fx * piece.0, bounds.min.y - fy * piece.1);
        let moved: Vec<Path> = cuts
            .iter()
            .map(|path| {
                Path::new(
                    path.points
                        .iter()
                        .map(|point| Point::new(point.x + dx, point.y + dy))
                        .collect(),
                    false,
                )
            })
            .collect();
        crate::region::clip_lines(&moved, &area)
            .into_iter()
            .filter(|path| path.length() >= 1.0)
            .collect()
    };
    let phases = [0.0, 0.25, 0.5, 0.75];
    let mut best: Option<(f64, Vec<Path>)> = None;
    for fy in phases {
        for fx in phases {
            let candidate = shifted(fx, fy);
            let smallest = raster.smallest_piece(&candidate);
            if best.as_ref().is_none_or(|(score, _)| smallest > *score) {
                best = Some((smallest, candidate));
            }
        }
    }
    let chosen = best.map(|(_, paths)| paths).unwrap_or_default();
    // Pieces still too small join their largest neighbour: the cut
    // between them goes.
    let mut paths = raster.merge_small(&chosen, 0.3 * piece.0 * piece.1);
    paths.retain(|path| path.length() >= 1.0);
    paths.extend(border);
    Some(paths)
}

/// The inside of a shape on a fine grid, to measure the pieces cut
/// lines make in it.
struct Raster {
    origin: Point,
    cell: f64,
    width: usize,
    height: usize,
    inside: Vec<bool>,
}

impl Raster {
    fn new(border: &[Path], bounds: crate::geometry::Rect) -> Self {
        let cell = (bounds.width().max(bounds.height()) / 240.0).max(0.2);
        let width = (bounds.width() / cell).ceil() as usize + 1;
        let height = (bounds.height() / cell).ceil() as usize + 1;
        let origin = bounds.min;
        let mut inside = vec![false; width * height];
        for y in 0..height {
            for x in 0..width {
                let centre = Point::new(
                    origin.x + (x as f64 + 0.5) * cell,
                    origin.y + (y as f64 + 0.5) * cell,
                );
                inside[y * width + x] = crate::geometry::contains_even_odd(border, centre);
            }
        }
        Self {
            origin,
            cell,
            width,
            height,
            inside,
        }
    }

    fn cell_of(&self, point: Point) -> Option<usize> {
        let x = ((point.x - self.origin.x) / self.cell).floor();
        let y = ((point.y - self.origin.y) / self.cell).floor();
        (x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| y as usize * self.width + x as usize)
    }

    /// Pieces the cuts make: a label per free cell (`usize::MAX` on
    /// cuts and outside) and the cell count of each label.
    fn label(&self, cuts: &[Path]) -> (Vec<usize>, Vec<usize>) {
        let mut free = self.inside.clone();
        for cut in cuts {
            for (a, b) in cut.segments() {
                let steps = (a.distance(b) / (self.cell * 0.5)).ceil().max(1.0) as usize;
                for step in 0..=steps {
                    if let Some(index) = self.cell_of(a.lerp(b, step as f64 / steps as f64)) {
                        free[index] = false;
                    }
                }
            }
        }
        let mut labels = vec![usize::MAX; free.len()];
        let mut sizes = Vec::new();
        let mut stack = Vec::new();
        for start in 0..free.len() {
            if !free[start] || labels[start] != usize::MAX {
                continue;
            }
            let label = sizes.len();
            labels[start] = label;
            stack.push(start);
            let mut count = 0usize;
            while let Some(index) = stack.pop() {
                count += 1;
                let (x, y) = (index % self.width, index / self.width);
                let neighbours = [
                    (x > 0).then(|| index - 1),
                    (x + 1 < self.width).then(|| index + 1),
                    (y > 0).then(|| index - self.width),
                    (y + 1 < self.height).then(|| index + self.width),
                ];
                for next in neighbours.into_iter().flatten() {
                    if free[next] && labels[next] == usize::MAX {
                        labels[next] = label;
                        stack.push(next);
                    }
                }
            }
            sizes.push(count);
        }
        (labels, sizes)
    }

    /// Area of the smallest piece, mm²; specks of a cell or two along
    /// the cuts are not pieces.
    fn smallest_piece(&self, cuts: &[Path]) -> f64 {
        let (_, sizes) = self.label(cuts);
        sizes
            .iter()
            .filter(|count| **count > 2)
            .min()
            .map_or(0.0, |count| *count as f64 * self.cell * self.cell)
    }

    /// Joins every piece under `minimum` mm² to the neighbour it shares
    /// the longest cut with, and drops the stretches of cut that now
    /// run inside one piece.
    fn merge_small(&self, cuts: &[Path], minimum: f64) -> Vec<Path> {
        let (labels, sizes) = self.label(cuts);
        let area = |count: usize| count as f64 * self.cell * self.cell;
        let mut parent: Vec<usize> = (0..sizes.len()).collect();
        fn root(parent: &mut [usize], mut label: usize) -> usize {
            while parent[label] != label {
                parent[label] = parent[parent[label]];
                label = parent[label];
            }
            label
        }
        let mut merged_size = sizes.clone();
        // Smallest first, so slivers join real pieces.
        let mut order: Vec<usize> = (0..sizes.len()).filter(|label| sizes[*label] > 2).collect();
        order.sort_by_key(|label| sizes[*label]);
        let reach = 3isize;
        for label in order {
            let own = root(&mut parent, label);
            if area(merged_size[own]) >= minimum {
                continue;
            }
            // Neighbours across the cut, by how many cells touch.
            let mut contacts: std::collections::HashMap<usize, usize> = Default::default();
            for (index, cell_label) in labels.iter().enumerate() {
                if *cell_label != label {
                    continue;
                }
                let (x, y) = ((index % self.width) as isize, (index / self.width) as isize);
                for (dx, dy) in [(reach, 0), (-reach, 0), (0, reach), (0, -reach)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= self.width as isize || ny >= self.height as isize {
                        continue;
                    }
                    let other = labels[ny as usize * self.width + nx as usize];
                    if other != usize::MAX && sizes[other] > 2 {
                        let other = root(&mut parent, other);
                        if other != own {
                            *contacts.entry(other).or_default() += 1;
                        }
                    }
                }
            }
            if let Some((&neighbour, _)) = contacts.iter().max_by_key(|(_, count)| **count) {
                parent[own] = neighbour;
                merged_size[neighbour] += merged_size[own];
            }
        }
        // Keep a stretch of cut only where its two sides are different
        // pieces (or the border).
        let side = |point: Point| -> Option<usize> {
            let label = labels[self.cell_of(point)?];
            (label != usize::MAX).then_some(label)
        };
        let offset = self.cell * 2.0;
        let mut kept = Vec::new();
        for cut in cuts {
            let mut current: Vec<Point> = Vec::new();
            for (a, b) in cut.segments() {
                let direction = (b - a).normalized();
                let normal = Point::new(-direction.y, direction.x);
                let middle = a.lerp(b, 0.5);
                let left = side(Point::new(
                    middle.x + normal.x * offset,
                    middle.y + normal.y * offset,
                ));
                let right = side(Point::new(
                    middle.x - normal.x * offset,
                    middle.y - normal.y * offset,
                ));
                let joined = match (left, right) {
                    (Some(left), Some(right)) => {
                        root(&mut parent, left) == root(&mut parent, right)
                    }
                    _ => false,
                };
                if joined {
                    if current.len() > 1 {
                        kept.push(Path::new(std::mem::take(&mut current), false));
                    }
                    current.clear();
                } else {
                    if current.is_empty() {
                        current.push(a);
                    }
                    current.push(b);
                }
            }
            if current.len() > 1 {
                kept.push(Path::new(current, false));
            }
        }
        kept
    }
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

    #[test]
    fn a_puzzle_fits_inside_a_round_shape() {
        let circle = Path::new(
            (0..96)
                .map(|index| {
                    let angle = index as f64 / 96.0 * std::f64::consts::TAU;
                    Point::new(100.0 + 60.0 * angle.cos(), 50.0 + 60.0 * angle.sin())
                })
                .collect(),
            true,
        );
        let params = JigsawParams {
            columns: 4,
            rows: 4,
            ..JigsawParams::default()
        };
        let paths = generate_inside(std::slice::from_ref(&circle), &params).unwrap();
        // The border is the circle itself.
        assert_eq!(paths.last(), Some(&circle));
        let cuts = &paths[..paths.len() - 1];
        assert!(cuts.len() >= 6, "{} cuts", cuts.len());
        for cut in cuts {
            assert!(!cut.closed);
            for point in &cut.points {
                let distance = ((point.x - 100.0).powi(2) + (point.y - 50.0).powi(2)).sqrt();
                assert!(distance <= 60.0 + 1e-6, "cut outside at {point:?}");
            }
        }
        assert_eq!(generate_inside(&[], &params), None);
    }

    #[test]
    fn the_grid_is_placed_to_avoid_slivers_along_the_border() {
        // A disc: an unlucky grid leaves tiny pieces where it grazes it.
        let circle: Vec<Path> = vec![Path::new(
            (0..128)
                .map(|index| {
                    let angle = index as f64 / 128.0 * std::f64::consts::TAU;
                    Point::new(50.0 + 45.0 * angle.cos(), 50.0 + 45.0 * angle.sin())
                })
                .collect(),
            true,
        )];
        let params = JigsawParams {
            columns: 5,
            rows: 5,
            ..JigsawParams::default()
        };
        let bounds = bounds_of(&circle);
        let raster = Raster::new(&circle, bounds);
        let paths = generate_inside(&circle, &params).unwrap();
        let chosen = raster.smallest_piece(&paths[..paths.len() - 1]);
        // At least a fifth of a nominal piece (18 x 18 mm).
        assert!(chosen > 0.2 * 18.0 * 18.0, "smallest piece {chosen:.1} mm²");
    }
}
