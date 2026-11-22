//! Puzzle following the lines of a drawing, like wooden puzzles for
//! children: the heavy lines become the cuts between pieces, the outline
//! of the drawing its border, and thin lines (eyes, details) stay as a
//! drawing to engrave. Works the same on vectors and on pictures: the
//! drawing is rasterised, heavy lines are what survives an opening
//! (erosion then dilation) by half their width, the filled silhouette is
//! split by them, and the borders between the resulting regions, which
//! run along the middle of the heavy lines, are traced back to vectors.

use crate::document::{Shape, ShapeKind};
use crate::geometry::{Path, Point, Rect, simplify_polyline};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinePuzzleParams {
    /// Lines at least this wide, mm, are cuts; thinner ones are details.
    pub heavy: f64,
    /// Pieces smaller than a square of this side, mm, join a neighbour.
    pub min_piece: f64,
    /// Tray around the puzzle: margin from the outline, mm.
    pub frame: Option<f64>,
}

impl Default for LinePuzzleParams {
    fn default() -> Self {
        Self {
            heavy: 1.2,
            min_piece: 12.0,
            frame: Some(12.0),
        }
    }
}

pub struct LinePuzzle {
    /// Cut lines: the outline of the drawing and the borders between
    /// pieces.
    pub cuts: Vec<Path>,
    pub pieces: usize,
    /// Rectangle of the tray, when asked.
    pub frame: Option<Path>,
}

/// The drawing on a fine grid: dark cells are ink, body cells are
/// covered by the drawing at all (ink, or the colours of a picture).
pub struct Mask {
    origin: Point,
    cell: f64,
    width: usize,
    height: usize,
    dark: Vec<bool>,
    body: Vec<bool>,
}

/// Grey below which a picture is ink.
const INK: u8 = 128;
/// Grey below which a picture is drawn on (lighter is the paper).
const PAPER: u8 = 240;

impl Mask {
    /// An empty grid over `bounds` and `margin` around it, about 600
    /// cells along its long side.
    pub fn covering(bounds: Rect, margin: f64) -> Self {
        let span = bounds.width().max(bounds.height()) + 2.0 * margin;
        let cell = (span / 600.0).max(0.1);
        let origin = Point::new(bounds.min.x - margin, bounds.min.y - margin);
        let width = ((bounds.width() + 2.0 * margin) / cell).ceil() as usize + 1;
        let height = ((bounds.height() + 2.0 * margin) / cell).ceil() as usize + 1;
        Self {
            origin,
            cell,
            width,
            height,
            dark: vec![false; width * height],
            body: vec![false; width * height],
        }
    }

    fn centre(&self, x: usize, y: usize) -> Point {
        Point::new(
            self.origin.x + (x as f64 + 0.5) * self.cell,
            self.origin.y + (y as f64 + 0.5) * self.cell,
        )
    }

    /// Cells whose centre may lie in `rect`.
    fn range(&self, rect: Rect) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let to = |value: f64, origin: f64, size: usize| {
            ((value - origin) / self.cell)
                .floor()
                .clamp(0.0, size as f64) as usize
        };
        (
            to(rect.min.x, self.origin.x, self.width)
                ..to(rect.max.x, self.origin.x, self.width) + 1,
            to(rect.min.y, self.origin.y, self.height)
                ..to(rect.max.y, self.origin.y, self.height) + 1,
        )
    }

    /// Cells whose centre is inside `closed` (even-odd), row by row:
    /// crossings of each row with the edges, filled in pairs.
    fn fill_even_odd(&self, closed: &[Path]) -> Vec<usize> {
        let (_, rows) = self.range(crate::geometry::bounds_of(closed));
        let mut inside = Vec::new();
        let mut crossings = Vec::new();
        for y in rows.filter(|y| *y < self.height) {
            let row = self.origin.y + (y as f64 + 0.5) * self.cell;
            crossings.clear();
            for path in closed {
                let count = path.points.len();
                for index in 0..count {
                    let a = path.points[index];
                    let b = path.points[(index + 1) % count];
                    if (a.y <= row) != (b.y <= row) {
                        crossings.push(a.x + (row - a.y) / (b.y - a.y) * (b.x - a.x));
                    }
                }
            }
            crossings.sort_by(f64::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                let first = ((pair[0] - self.origin.x) / self.cell - 0.5)
                    .ceil()
                    .max(0.0) as usize;
                let last = ((pair[1] - self.origin.x) / self.cell - 0.5).floor();
                if last < 0.0 {
                    continue;
                }
                let last = (last as usize).min(self.width - 1);
                inside.extend((first..=last).map(|x| y * self.width + x));
            }
        }
        inside
    }

    fn mark_disc(&mut self, point: Point, radius: f64) {
        let reach = (radius / self.cell).ceil() as isize;
        let cx = ((point.x - self.origin.x) / self.cell).floor() as isize;
        let cy = ((point.y - self.origin.y) / self.cell).floor() as isize;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.width as isize || y >= self.height as isize {
                    continue;
                }
                let (x, y) = (x as usize, y as usize);
                if self.centre(x, y).distance(point) <= radius.max(self.cell * 0.5) {
                    self.dark[y * self.width + x] = true;
                    self.body[y * self.width + x] = true;
                }
            }
        }
    }

    /// Adds a shape as ink: closed outlines filled (even-odd), open
    /// lines drawn `line_width` wide, pictures dark where below half
    /// grey and part of the drawing where not paper white. A cell of a
    /// picture takes its darkest pixel, so lines thinner than a cell
    /// are not lost between two samples.
    pub fn add_shape(&mut self, shape: &Shape, line_width: f64) {
        if let ShapeKind::Image {
            image,
            width,
            height,
        } = &shape.kind
        {
            let Some(inverse) = shape.transform.inverse() else {
                return;
            };
            // Samples per cell side, enough to see every pixel.
            let (sx, sy) = shape.transform.scale_factors();
            let pixel = (width / image.width as f64 * sx.abs())
                .min(height / image.height as f64 * sy.abs())
                .max(1e-6);
            let samples = ((self.cell / pixel).ceil() as usize + 1).clamp(1, 8);
            let (columns, rows) = self.range(shape.bounds());
            for y in rows.clone() {
                for x in columns.clone() {
                    if x >= self.width || y >= self.height {
                        continue;
                    }
                    let centre = self.centre(x, y);
                    let mut darkest = u8::MAX;
                    for j in 0..samples {
                        for i in 0..samples {
                            let offset =
                                |k: usize| ((k as f64 + 0.5) / samples as f64 - 0.5) * self.cell;
                            let local = inverse
                                .apply(Point::new(centre.x + offset(i), centre.y + offset(j)));
                            let u = (local.x + width / 2.0) / width;
                            let v = (height / 2.0 - local.y) / height;
                            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                                continue;
                            }
                            let px = ((u * image.width as f64) as u32).min(image.width - 1);
                            let py = ((v * image.height as f64) as u32).min(image.height - 1);
                            darkest = darkest.min(image.pixel(px, py));
                        }
                    }
                    let index = y * self.width + x;
                    if darkest < INK {
                        self.dark[index] = true;
                    }
                    if darkest < PAPER {
                        self.body[index] = true;
                    }
                }
            }
            return;
        }
        let paths = shape.world_paths();
        let closed: Vec<Path> = paths
            .iter()
            .filter(|path| path.closed && path.points.len() > 2)
            .cloned()
            .collect();
        // A closed outline is either a heavy line drawn as a thin filled
        // area (filled), or a plain outline drawn as a line (stroked):
        // the mean width of its area, 2 x area / perimeter, tells.
        let mut stroked: Vec<&Path> = paths.iter().filter(|path| !path.closed).collect();
        if !closed.is_empty() {
            let inside = self.fill_even_odd(&closed);
            let area = inside.len() as f64 * self.cell * self.cell;
            let perimeter: f64 = closed.iter().map(Path::length).sum();
            if 2.0 * area / perimeter.max(1e-9) <= 3.0 * line_width {
                for index in inside {
                    self.dark[index] = true;
                    self.body[index] = true;
                }
            } else {
                stroked.extend(paths.iter().filter(|path| path.closed));
            }
        }
        for open in stroked {
            for (a, b) in open.segments() {
                let steps = (a.distance(b) / (self.cell * 0.5)).ceil().max(1.0) as usize;
                for step in 0..=steps {
                    self.mark_disc(a.lerp(b, step as f64 / steps as f64), line_width / 2.0);
                }
            }
        }
    }
}

impl Mask {
    /// Typical width of the ink lines, mm: twice the distance to the
    /// paper along the middle of the lines (cells at least as far from
    /// it as their neighbours), median over the drawing. Wide patches of
    /// colour are left out. `None` without lines.
    pub fn line_width(&self) -> Option<f64> {
        let (width, height) = (self.width, self.height);
        let background: Vec<bool> = self.dark.iter().map(|dark| !dark).collect();
        let distance = distance_to(&background, width, height);
        let widest = (3.0 / self.cell) as f32;
        let mut widths: Vec<f32> = (0..width * height)
            .filter(|index| self.dark[*index] && distance[*index] <= widest)
            .filter(|index| {
                neighbours(*index, width, height).all(|next| distance[next] <= distance[*index])
            })
            .map(|index| distance[index])
            .collect();
        if widths.len() < 20 {
            return None;
        }
        let middle = widths.len() / 2;
        let (_, median, _) = widths.select_nth_unstable_by(middle, f32::total_cmp);
        // A line one cell wide is at distance 1 from the paper.
        Some((2.0 * *median as f64 - 1.0) * self.cell)
    }
}

/// Distance, in cells, from every cell to the nearest `source` cell
/// (two-pass chamfer with the exact diagonal, within 8 % of Euclidean).
fn distance_to(sources: &[bool], width: usize, height: usize) -> Vec<f32> {
    const DIAGONAL: f32 = std::f32::consts::SQRT_2;
    let far = (width + height) as f32;
    let mut distance: Vec<f32> = sources.iter().map(|&s| if s { 0.0 } else { far }).collect();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let mut best = distance[index];
            if x > 0 {
                best = best.min(distance[index - 1] + 1.0);
            }
            if y > 0 {
                best = best.min(distance[index - width] + 1.0);
                if x > 0 {
                    best = best.min(distance[index - width - 1] + DIAGONAL);
                }
                if x + 1 < width {
                    best = best.min(distance[index - width + 1] + DIAGONAL);
                }
            }
            distance[index] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let index = y * width + x;
            let mut best = distance[index];
            if x + 1 < width {
                best = best.min(distance[index + 1] + 1.0);
            }
            if y + 1 < height {
                best = best.min(distance[index + width] + 1.0);
                if x + 1 < width {
                    best = best.min(distance[index + width + 1] + DIAGONAL);
                }
                if x > 0 {
                    best = best.min(distance[index + width - 1] + DIAGONAL);
                }
            }
            distance[index] = best;
        }
    }
    distance
}

fn neighbours(index: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (index % width, index / width);
    [
        (x > 0).then(|| index - 1),
        (x + 1 < width).then(|| index + 1),
        (y > 0).then(|| index - width),
        (y + 1 < height).then(|| index + width),
    ]
    .into_iter()
    .flatten()
}

fn root(parent: &mut [usize], mut label: usize) -> usize {
    while parent[label] != label {
        parent[label] = parent[parent[label]];
        label = parent[label];
    }
    label
}

/// Chaikin corner cutting, ends kept for open lines; a corner is cut
/// at most `reach` from its vertex, so long straight sides keep sharp
/// corners and only the staircase of the grid is rounded.
fn smooth(points: &[Point], closed: bool, passes: usize, reach: f64) -> Vec<Point> {
    let mut points = points.to_vec();
    for _ in 0..passes {
        if points.len() < 3 {
            break;
        }
        let count = points.len();
        let mut out = Vec::with_capacity(count * 2);
        if !closed {
            out.push(points[0]);
        }
        let segments = if closed { count } else { count - 1 };
        for index in 0..segments {
            let a = points[index];
            let b = points[(index + 1) % count];
            let length = a.distance(b).max(1e-12);
            let cut = (length * 0.25).min(reach) / length;
            out.push(a.lerp(b, cut));
            out.push(a.lerp(b, 1.0 - cut));
        }
        if !closed {
            out.push(points[count - 1]);
        }
        points = out;
    }
    points
}

/// Cuts of a puzzle following the heavy lines of the drawing in `mask`;
/// `None` when the drawing encloses nothing.
pub fn generate(mask: &Mask, params: &LinePuzzleParams) -> Option<LinePuzzle> {
    let (width, height, cell) = (mask.width, mask.height, mask.cell);
    let count = width * height;
    let half = (params.heavy / 2.0 / cell) as f32;
    // Heavy lines: the ink that survives an opening by half their width.
    let background: Vec<bool> = mask.dark.iter().map(|dark| !dark).collect();
    let to_background = distance_to(&background, width, height);
    let core: Vec<bool> = (0..count)
        .map(|index| mask.dark[index] && to_background[index] > half)
        .collect();
    let to_core = distance_to(&core, width, height);
    // Ink much wider than a line (a dark patch of colour) is an area of
    // its own, a piece, not a line to cut along.
    let patch = (half * 4.0).max(half + 3.0);
    let patch_core: Vec<bool> = (0..count)
        .map(|index| mask.dark[index] && to_background[index] > patch)
        .collect();
    let to_patch = distance_to(&patch_core, width, height);
    let area: Vec<bool> = (0..count)
        .map(|index| mask.dark[index] && to_patch[index] <= patch + 1.0)
        .collect();
    let heavy: Vec<bool> = (0..count)
        .map(|index| mask.dark[index] && to_core[index] <= half + 1.0 && !area[index])
        .collect();
    // Silhouette: the drawing closed over small gaps, holes filled.
    let gap = half.max(1.0) + 1.0;
    let to_ink = distance_to(&mask.body, width, height);
    let grown: Vec<bool> = to_ink.iter().map(|d| *d <= gap).collect();
    let not_grown: Vec<bool> = grown.iter().map(|g| !g).collect();
    let to_outside_grown = distance_to(&not_grown, width, height);
    let closed: Vec<bool> = (0..count)
        .map(|index| grown[index] && to_outside_grown[index] > gap)
        .zip(mask.body.iter())
        .map(|(inside, body)| inside || *body)
        .collect();
    let mut outside = vec![false; count];
    let mut queue = VecDeque::new();
    for x in 0..width {
        for y in [0, height - 1] {
            queue.push_back(y * width + x);
        }
    }
    for y in 0..height {
        for x in [0, width - 1] {
            queue.push_back(y * width + x);
        }
    }
    while let Some(index) = queue.pop_front() {
        if outside[index] || closed[index] {
            continue;
        }
        outside[index] = true;
        queue.extend(neighbours(index, width, height).filter(|next| !outside[*next]));
    }
    let silhouette: Vec<bool> = outside.iter().map(|o| !o).collect();
    // Regions between the heavy lines.
    let mut labels = vec![usize::MAX; count];
    let mut sizes = Vec::new();
    for start in 0..count {
        if !silhouette[start] || heavy[start] || labels[start] != usize::MAX {
            continue;
        }
        let label = sizes.len();
        labels[start] = label;
        let mut stack = vec![start];
        let mut size = 0usize;
        while let Some(index) = stack.pop() {
            size += 1;
            for next in neighbours(index, width, height) {
                // Patches and the rest are separate pieces even without
                // a line between them.
                if silhouette[next]
                    && !heavy[next]
                    && area[next] == area[start]
                    && labels[next] == usize::MAX
                {
                    labels[next] = label;
                    stack.push(next);
                }
            }
        }
        sizes.push(size);
    }
    if sizes.is_empty() {
        return None;
    }
    // The heavy lines are shared out between the regions on each side,
    // so the borders run along their middle.
    let mut queue: VecDeque<usize> = (0..count)
        .filter(|index| labels[*index] != usize::MAX)
        .collect();
    while let Some(index) = queue.pop_front() {
        let label = labels[index];
        for next in neighbours(index, width, height) {
            if silhouette[next] && labels[next] == usize::MAX {
                labels[next] = label;
                sizes[label] += 1;
                queue.push_back(next);
            }
        }
    }
    // Small pieces join the neighbour they touch most.
    let minimum = ((params.min_piece / cell).powi(2)) as usize;
    let mut parent: Vec<usize> = (0..sizes.len()).collect();
    let mut merged = sizes.clone();
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|label| sizes[*label]);
    let mut contacts: HashMap<(usize, usize), usize> = HashMap::new();
    let mut dropped: HashSet<usize> = HashSet::new();
    for index in 0..count {
        let label = labels[index];
        if label == usize::MAX {
            continue;
        }
        for next in neighbours(index, width, height) {
            let other = labels[next];
            if other != usize::MAX && other != label {
                *contacts.entry((label, other)).or_default() += 1;
            }
        }
    }
    for label in order {
        let own = root(&mut parent, label);
        if merged[own] >= minimum {
            continue;
        }
        let mut best: Option<(usize, usize)> = None;
        for (&(a, b), &touching) in &contacts {
            let (a, b) = (root(&mut parent, a), root(&mut parent, b));
            if a == own && b != own && best.is_none_or(|(_, most)| touching > most) {
                best = Some((b, touching));
            }
        }
        match best {
            Some((neighbour, _)) => {
                parent[own] = neighbour;
                merged[neighbour] += merged[own];
            }
            // A speck touching nothing (a signature, a stray mark) is no
            // piece: it stays in the drawing, uncut.
            None => {
                dropped.insert(own);
            }
        }
    }
    let piece_of = |label: usize, parent: &mut Vec<usize>| -> i64 {
        if label == usize::MAX {
            return -1;
        }
        let own = root(parent, label);
        if dropped.contains(&own) {
            -1
        } else {
            own as i64
        }
    };
    let final_labels: Vec<i64> = labels
        .iter()
        .map(|label| piece_of(*label, &mut parent))
        .collect();
    let pieces = final_labels
        .iter()
        .filter(|label| **label >= 0)
        .collect::<HashSet<_>>()
        .len();
    // Borders between different pieces (or a piece and the outside),
    // along cell edges, chained into lines between junctions.
    let corner = |x: usize, y: usize| y * (width + 1) + x;
    let mut edges: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut add = |a: usize, b: usize| {
        edges.entry(a).or_default().push(b);
        edges.entry(b).or_default().push(a);
    };
    for y in 0..height {
        for x in 0..width {
            let here = final_labels[y * width + x];
            if x + 1 < width {
                let right = final_labels[y * width + x + 1];
                if here != right {
                    add(corner(x + 1, y), corner(x + 1, y + 1));
                }
            }
            if y + 1 < height {
                let above = final_labels[(y + 1) * width + x];
                if here != above {
                    add(corner(x, y + 1), corner(x + 1, y + 1));
                }
            }
        }
    }
    let to_point = |index: usize| {
        let (x, y) = (index % (width + 1), index / (width + 1));
        Point::new(
            mask.origin.x + x as f64 * cell,
            mask.origin.y + y as f64 * cell,
        )
    };
    let mut used: HashSet<(usize, usize)> = HashSet::new();
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut lines: Vec<(Vec<usize>, bool)> = Vec::new();
    let walk = |start: usize, first: usize, used: &mut HashSet<(usize, usize)>| {
        let mut chain = vec![start, first];
        used.insert(key(start, first));
        let (mut previous, mut current) = (start, first);
        loop {
            let next_corners = &edges[&current];
            if next_corners.len() != 2 || current == start {
                break;
            }
            let next = if next_corners[0] == previous {
                next_corners[1]
            } else {
                next_corners[0]
            };
            if used.contains(&key(current, next)) {
                break;
            }
            used.insert(key(current, next));
            chain.push(next);
            previous = current;
            current = next;
        }
        let closed = chain.len() > 2 && chain.first() == chain.last();
        (chain, closed)
    };
    let mut nodes: Vec<usize> = edges
        .iter()
        .filter(|(_, next)| next.len() != 2)
        .map(|(corner, _)| *corner)
        .collect();
    nodes.sort_unstable();
    for node in nodes {
        let next: Vec<usize> = edges[&node].clone();
        for first in next {
            if !used.contains(&key(node, first)) {
                lines.push(walk(node, first, &mut used));
            }
        }
    }
    let mut rest: Vec<usize> = edges.keys().copied().collect();
    rest.sort_unstable();
    for start in rest {
        let next: Vec<usize> = edges[&start].clone();
        for first in next {
            if !used.contains(&key(start, first)) {
                lines.push(walk(start, first, &mut used));
            }
        }
    }
    let cuts: Vec<Path> = lines
        .into_iter()
        .filter_map(|(chain, closed)| {
            let mut points: Vec<Point> = chain.into_iter().map(to_point).collect();
            if closed {
                points.pop();
            }
            // Pixel steps go, corners of the drawing stay.
            let simple = simplify_polyline(&points, cell * 1.5);
            let smoothed = smooth(&simple, closed, 3, cell * 3.0);
            (smoothed.len() > 1).then(|| Path::new(smoothed, closed))
        })
        .filter(|path| path.length() > cell * 2.0)
        .collect();
    let frame = params.frame.map(|margin| {
        let mut bounds = Rect::EMPTY;
        for y in 0..height {
            for x in 0..width {
                if final_labels[y * width + x] >= 0 {
                    bounds.include(mask.centre(x, y));
                }
            }
        }
        let (min, max) = (bounds.min, bounds.max);
        Path::new(
            vec![
                Point::new(min.x - margin, min.y - margin),
                Point::new(max.x + margin, min.y - margin),
                Point::new(max.x + margin, max.y + margin),
                Point::new(min.x - margin, max.y + margin),
            ],
            true,
        )
    });
    Some(LinePuzzle {
        cuts,
        pieces,
        frame,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LayerId;

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

    #[test]
    fn heavy_lines_split_the_drawing_and_thin_ones_do_not() {
        // A 100 x 60 drawing: 2 mm outline, a 2 mm bar down the middle
        // and a 0.3 mm line across (a detail).
        let paths = vec![
            rectangle(0.0, 0.0, 100.0, 60.0),
            rectangle(2.0, 2.0, 96.0, 56.0),
        ];
        let outline = Shape::new("outline", LayerId::new(), ShapeKind::Paths { paths });
        let bar = Shape::new(
            "bar",
            LayerId::new(),
            ShapeKind::Paths {
                paths: vec![rectangle(49.0, 0.0, 2.0, 60.0)],
            },
        );
        let detail = Shape::new(
            "detail",
            LayerId::new(),
            ShapeKind::Paths {
                paths: vec![rectangle(0.0, 29.85, 100.0, 0.3)],
            },
        );
        let bounds = rectangle(0.0, 0.0, 100.0, 60.0).bounds();
        let mut mask = Mask::covering(bounds, 5.0);
        for shape in [&outline, &bar, &detail] {
            mask.add_shape(shape, 1.5);
        }
        let puzzle = generate(&mask, &LinePuzzleParams::default()).unwrap();
        assert_eq!(puzzle.pieces, 2);
        // Away from the outline, the only cut runs down the middle of
        // the bar (the outline itself is split where that cut meets it).
        let mut inner = 0;
        let samples = puzzle.cuts.iter().flat_map(|cut| {
            cut.segments()
                .flat_map(|(a, b)| (0..=10).map(move |step| a.lerp(b, step as f64 / 10.0)))
                .collect::<Vec<_>>()
        });
        for point in samples {
            if point.x > 5.0 && point.x < 95.0 && point.y > 5.0 && point.y < 55.0 {
                assert!((point.x - 50.0).abs() < 1.5, "{point:?}");
                inner += 1;
            }
        }
        assert!(inner > 0);
        let frame = puzzle.frame.unwrap().bounds();
        assert!(frame.width() > 120.0 && frame.width() < 130.0);
    }

    #[test]
    fn open_lines_are_cuts_and_tiny_regions_join_a_neighbour() {
        // A circle cut in four by a cross of open lines, plus a tiny
        // nook closed by a short stroke.
        let circle: Vec<Point> = (0..96)
            .map(|index| {
                let angle = index as f64 / 96.0 * std::f64::consts::TAU;
                Point::new(50.0 + 40.0 * angle.cos(), 50.0 + 40.0 * angle.sin())
            })
            .collect();
        let lines = vec![
            Path::new(circle, true),
            Path::new(vec![Point::new(10.0, 50.0), Point::new(90.0, 50.0)], false),
            Path::new(vec![Point::new(50.0, 10.0), Point::new(50.0, 90.0)], false),
            Path::new(vec![Point::new(50.0, 54.0), Point::new(54.0, 50.0)], false),
        ];
        let drawing = Shape::new("drawing", LayerId::new(), ShapeKind::Paths { paths: lines });
        let mut mask = Mask::covering(drawing.bounds(), 5.0);
        mask.add_shape(&drawing, 1.5);
        let puzzle = generate(
            &mask,
            &LinePuzzleParams {
                frame: None,
                ..LinePuzzleParams::default()
            },
        )
        .unwrap();
        // The corner nook (about 8 mm²) is not a piece of its own.
        assert_eq!(puzzle.pieces, 4);
        assert!(puzzle.frame.is_none());
    }

    #[test]
    fn pictures_keep_thin_outlines_light_colours_and_drop_specks() {
        // 800 x 600 px over 200 x 150 mm (4 px per mm): a light grey
        // rectangle with a 2 px black outline split by a 2 px line, and a
        // tiny ring far from it (a signature).
        let (columns, rows) = (800u32, 600u32);
        let mut pixels = vec![255u8; (columns * rows) as usize];
        let mut paint = |x: u32, y: u32, grey: u8| pixels[(y * columns + x) as usize] = grey;
        for y in 100..500 {
            for x in 100..700 {
                let edge =
                    !(102..698).contains(&x) || !(102..498).contains(&y) || (399..401).contains(&x);
                paint(x, y, if edge { 0 } else { 200 });
            }
        }
        for y in 20..32 {
            for x in 20..32 {
                if !(22..30).contains(&x) || !(22..30).contains(&y) {
                    paint(x, y, 0);
                }
            }
        }
        let image = crate::document::RasterImage {
            width: columns,
            height: rows,
            pixels,
        };
        let picture = Shape::new(
            "picture",
            LayerId::new(),
            ShapeKind::Image {
                image: std::sync::Arc::new(image),
                width: 200.0,
                height: 150.0,
            },
        );
        let mut mask = Mask::covering(picture.bounds(), 5.0);
        mask.add_shape(&picture, 1.0);
        // Lines of 0.5 mm, finer than a cell: about one cell wide.
        let line = mask.line_width().unwrap();
        assert!(line > 0.2 && line < 0.8, "{line}");
        let puzzle = generate(
            &mask,
            &LinePuzzleParams {
                heavy: line * 1.3,
                min_piece: 12.0,
                frame: Some(10.0),
            },
        )
        .unwrap();
        assert_eq!(puzzle.pieces, 2);
        // The ring is no piece and the tray ignores it: it fits the
        // 150 x 100 mm rectangle and its 10 mm margin.
        let frame = puzzle.frame.unwrap().bounds();
        assert!((frame.width() - 170.0).abs() < 2.0, "{frame:?}");
        assert!((frame.height() - 120.0).abs() < 2.0, "{frame:?}");
        for cut in &puzzle.cuts {
            let bounds = cut.bounds();
            assert!(bounds.min.x > -80.0 && bounds.max.y < 55.0, "{bounds:?}");
        }
    }
}
