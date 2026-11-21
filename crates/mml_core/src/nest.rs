//! Nesting: parts placed on a sheet by their real outline, not their
//! bounding box, so an L fits into another L and round parts share the
//! corners between them. Each part is rasterised on a grid (a cell is
//! marked as soon as the part touches it, then grown by the spacing),
//! and parts go one by one, biggest first, to the lowest then leftmost
//! free place, trying each allowed rotation.

use crate::geometry::{Path, Point, Rect, Transform, bounds_of, contains_even_odd};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NestParams {
    /// Usable sheet, mm, from (0, 0).
    pub sheet: (f64, f64),
    /// Empty band along the sheet edges, mm.
    pub margin: f64,
    /// Minimum gap between parts, mm.
    pub spacing: f64,
    /// Quarter turns allowed (a part keeps its angle otherwise).
    pub rotate: bool,
}

/// Where each part went: the transform to add to it, or `None` when it
/// did not fit on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Nesting {
    pub placements: Vec<Option<Transform>>,
    /// Height of the sheet used, mm, from its lower edge.
    pub used_height: f64,
}

/// Occupied cells of a part, row by row, as runs [start, end).
struct Mask {
    width: usize,
    height: usize,
    rows: Vec<Vec<(usize, usize)>>,
    /// Lower left corner of the grid, in the part's coordinates.
    origin: Point,
    cells: usize,
}

fn rasterise(paths: &[Path], cell: f64, grow: usize) -> Option<Mask> {
    let bounds = bounds_of(paths);
    if bounds.is_empty() {
        return None;
    }
    let pad = grow as f64 * cell;
    let origin = Point::new(bounds.min.x - pad, bounds.min.y - pad);
    let width = ((bounds.width() + 2.0 * pad) / cell).floor() as usize + 1;
    let height = ((bounds.height() + 2.0 * pad) / cell).floor() as usize + 1;
    let mut grid = vec![false; width * height];
    let mark = |point: Point, grid: &mut Vec<bool>| {
        let x = ((point.x - origin.x) / cell).floor();
        let y = ((point.y - origin.y) / cell).floor();
        if x >= 0.0 && y >= 0.0 && (x as usize) < width && (y as usize) < height {
            grid[y as usize * width + x as usize] = true;
        }
    };
    // Outline: every cell an edge crosses.
    for path in paths {
        for (a, b) in path.segments() {
            let steps = (a.distance(b) / (cell * 0.25)).ceil().max(1.0) as usize;
            for step in 0..=steps {
                mark(a.lerp(b, step as f64 / steps as f64), &mut grid);
            }
        }
        if let Some(point) = path.points.first() {
            mark(*point, &mut grid);
        }
    }
    // Inside: cell centres within the closed outlines (holes stay free).
    let closed: Vec<Path> = paths.iter().filter(|path| path.closed).cloned().collect();
    if !closed.is_empty() {
        for y in 0..height {
            for x in 0..width {
                let centre = Point::new(
                    origin.x + (x as f64 + 0.5) * cell,
                    origin.y + (y as f64 + 0.5) * cell,
                );
                if contains_even_odd(&closed, centre) {
                    grid[y * width + x] = true;
                }
            }
        }
    }
    // Spacing: grow by a disc of `grow` cells.
    if grow > 0 {
        let source = grid.clone();
        let reach = grow as isize;
        let offsets: Vec<(isize, isize)> = (-reach..=reach)
            .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
            .filter(|(dx, dy)| dx * dx + dy * dy <= reach * reach)
            .collect();
        for y in 0..height {
            for x in 0..width {
                if !source[y * width + x] {
                    continue;
                }
                for (dx, dy) in &offsets {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                        grid[ny as usize * width + nx as usize] = true;
                    }
                }
            }
        }
    }
    let mut cells = 0;
    let rows = (0..height)
        .map(|y| {
            let mut runs = Vec::new();
            let mut start = None;
            for x in 0..=width {
                let filled = x < width && grid[y * width + x];
                match (filled, start) {
                    (true, None) => start = Some(x),
                    (false, Some(from)) => {
                        runs.push((from, x));
                        cells += x - from;
                        start = None;
                    }
                    _ => {}
                }
            }
            runs
        })
        .collect();
    Some(Mask {
        width,
        height,
        rows,
        origin,
        cells,
    })
}

/// The sheet: occupied cells per row with prefix sums, so a run is
/// checked in constant time.
struct Sheet {
    width: usize,
    height: usize,
    filled: Vec<Vec<bool>>,
    sums: Vec<Vec<u32>>,
}

impl Sheet {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            filled: vec![vec![false; width]; height],
            sums: vec![vec![0; width + 1]; height],
        }
    }

    fn free(&self, mask: &Mask, x: usize, y: usize) -> bool {
        if x + mask.width > self.width || y + mask.height > self.height {
            return false;
        }
        mask.rows.iter().enumerate().all(|(row, runs)| {
            let sums = &self.sums[y + row];
            runs.iter().all(|(from, to)| sums[x + to] == sums[x + from])
        })
    }

    fn place(&mut self, mask: &Mask, x: usize, y: usize) {
        for (row, runs) in mask.rows.iter().enumerate() {
            let line = &mut self.filled[y + row];
            for (from, to) in runs {
                line[x + from..x + to]
                    .iter_mut()
                    .for_each(|cell| *cell = true);
            }
            let sums = &mut self.sums[y + row];
            for column in 0..self.width {
                sums[column + 1] = sums[column] + u32::from(line[column]);
            }
        }
    }
}

/// Places `parts` (each a set of paths in sheet coordinates) on the
/// sheet. Grid cells are a 300th of the sheet's long side, at least
/// 0.25 mm, so a part may keep up to one cell more than `spacing`.
pub fn nest(parts: &[Vec<Path>], params: &NestParams) -> Nesting {
    let (sheet_width, sheet_height) = params.sheet;
    let usable_width = sheet_width - 2.0 * params.margin;
    let usable_height = sheet_height - 2.0 * params.margin;
    let mut placements = vec![None; parts.len()];
    if usable_width <= 0.0 || usable_height <= 0.0 {
        return Nesting {
            placements,
            used_height: 0.0,
        };
    }
    let cell = (usable_width.max(usable_height) / 300.0).max(0.25);
    // Each part is grown by half the spacing: two neighbours then keep
    // the whole spacing between them, and none sticks to the margin.
    let grow = (params.spacing / 2.0 / cell).ceil() as usize;
    let mut sheet = Sheet::new(
        (usable_width / cell).floor() as usize,
        (usable_height / cell).floor() as usize,
    );
    let angles: &[f64] = if params.rotate {
        &[0.0, 90.0, 180.0, 270.0]
    } else {
        &[0.0]
    };
    struct Candidate {
        angle: f64,
        mask: Mask,
        pivot: Point,
    }
    let mut prepared: Vec<(usize, Vec<Candidate>)> = parts
        .iter()
        .enumerate()
        .map(|(index, paths)| {
            let pivot = bounds_of(paths).center();
            let candidates = angles
                .iter()
                .filter_map(|&angle| {
                    let turn = Transform::rotate_degrees(angle).around(pivot);
                    let turned: Vec<Path> =
                        paths.iter().map(|path| path.transformed(&turn)).collect();
                    rasterise(&turned, cell, grow).map(|mask| Candidate { angle, mask, pivot })
                })
                .collect();
            (index, candidates)
        })
        .collect();
    // Biggest first: small parts then fill the gaps left between them.
    prepared.sort_by_key(|(_, candidates)| {
        std::cmp::Reverse(
            candidates
                .first()
                .map_or(0, |candidate| candidate.mask.cells),
        )
    });
    let mut top = 0usize;
    for (index, candidates) in prepared {
        // Lowest top edge first, then leftmost: the sheet fills from its
        // lower edge and leaves the rest in one piece.
        let mut best: Option<(usize, usize, usize, &Candidate)> = None;
        for candidate in &candidates {
            let mask = &candidate.mask;
            'search: for y in 0..sheet.height.saturating_sub(mask.height) + 1 {
                if let Some((best_top, _, _, _)) = best
                    && y + mask.height > best_top
                {
                    break;
                }
                for x in 0..sheet.width.saturating_sub(mask.width) + 1 {
                    if sheet.free(mask, x, y) {
                        let candidate_top = y + mask.height;
                        let better = best.is_none_or(|(best_top, best_x, _, _)| {
                            (candidate_top, x) < (best_top, best_x)
                        });
                        if better {
                            best = Some((candidate_top, x, y, candidate));
                        }
                        break 'search;
                    }
                }
            }
        }
        let Some((candidate_top, x, y, candidate)) = best else {
            continue;
        };
        sheet.place(&candidate.mask, x, y);
        top = top.max(candidate_top);
        let target = Point::new(
            params.margin + x as f64 * cell,
            params.margin + y as f64 * cell,
        );
        let origin = candidate.mask.origin;
        placements[index] = Some(
            Transform::rotate_degrees(candidate.angle)
                .around(candidate.pivot)
                .then(Transform::translate(
                    target.x - origin.x,
                    target.y - origin.y,
                )),
        );
    }
    let grow_mm = grow as f64 * cell;
    Nesting {
        placements,
        used_height: (params.margin + top as f64 * cell - grow_mm).max(0.0),
    }
}

/// Groups shapes into parts: a shape whose bounds lie inside another's
/// (a hole, an engraving on the part) goes with it. Returns, for each
/// shape, the index of its part.
pub fn group_parts(bounds: &[Rect]) -> Vec<usize> {
    let inside = |inner: &Rect, outer: &Rect| {
        inner.min.x >= outer.min.x - 1e-6
            && inner.min.y >= outer.min.y - 1e-6
            && inner.max.x <= outer.max.x + 1e-6
            && inner.max.y <= outer.max.y + 1e-6
    };
    let area = |rect: &Rect| rect.width() * rect.height();
    let mut part = vec![usize::MAX; bounds.len()];
    let mut order: Vec<usize> = (0..bounds.len()).collect();
    order.sort_by(|&a, &b| area(&bounds[b]).total_cmp(&area(&bounds[a])));
    let mut next = 0;
    for &index in &order {
        // The smallest larger shape that contains it gives its part.
        let container = order
            .iter()
            .take_while(|&&other| other != index)
            .filter(|&&other| inside(&bounds[index], &bounds[other]))
            .last();
        part[index] = match container {
            Some(&other) => part[other],
            None => {
                next += 1;
                next - 1
            }
        };
    }
    part
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn placed(paths: &[Path], transform: &Transform) -> Rect {
        let moved: Vec<Path> = paths
            .iter()
            .map(|path| path.transformed(transform))
            .collect();
        bounds_of(&moved)
    }

    #[test]
    fn parts_stay_on_the_sheet_and_apart() {
        let parts: Vec<Vec<Path>> = (0..6)
            .map(|index| vec![rectangle(500.0 + index as f64, -40.0, 40.0, 30.0)])
            .collect();
        let params = NestParams {
            sheet: (150.0, 120.0),
            margin: 5.0,
            spacing: 3.0,
            rotate: false,
        };
        let nesting = nest(&parts, &params);
        let rects: Vec<Rect> = parts
            .iter()
            .zip(&nesting.placements)
            .map(|(paths, placement)| placed(paths, placement.as_ref().expect("fits")))
            .collect();
        for (index, rect) in rects.iter().enumerate() {
            assert!(
                rect.min.x >= 5.0 - 1e-6 && rect.min.y >= 5.0 - 1e-6,
                "{rect:?}"
            );
            assert!(
                rect.max.x <= 145.0 + 1e-6 && rect.max.y <= 115.0 + 1e-6,
                "{rect:?}"
            );
            for other in &rects[index + 1..] {
                let gap_x = (other.min.x - rect.max.x).max(rect.min.x - other.max.x);
                let gap_y = (other.min.y - rect.max.y).max(rect.min.y - other.max.y);
                assert!(gap_x.max(gap_y) >= 3.0 - 1e-6, "{rect:?} {other:?}");
            }
        }
        // Three per row of 150 mm: two rows.
        assert!(nesting.used_height < 80.0, "{}", nesting.used_height);
    }

    #[test]
    fn a_small_part_goes_into_a_hole() {
        // A frame with a large window, and a part that fits in it.
        let frame = vec![
            rectangle(0.0, 0.0, 100.0, 100.0),
            rectangle(20.0, 20.0, 60.0, 60.0).reversed(),
        ];
        let small = vec![rectangle(0.0, 0.0, 40.0, 40.0)];
        let params = NestParams {
            sheet: (110.0, 110.0),
            margin: 0.0,
            spacing: 2.0,
            rotate: false,
        };
        let nesting = nest(&[frame, small.clone()], &params);
        let rect = placed(&small, nesting.placements[1].as_ref().expect("fits"));
        assert!(rect.min.x >= 20.0 && rect.max.x <= 80.0, "{rect:?}");
        assert!(rect.min.y >= 20.0 && rect.max.y <= 80.0, "{rect:?}");
    }

    #[test]
    fn rotation_lets_a_long_part_fit_and_too_big_ones_are_left_out() {
        let long = vec![rectangle(0.0, 0.0, 30.0, 90.0)];
        let huge = vec![rectangle(0.0, 0.0, 500.0, 500.0)];
        let params = NestParams {
            sheet: (100.0, 40.0),
            margin: 0.0,
            spacing: 1.0,
            rotate: true,
        };
        let nesting = nest(&[long.clone(), huge], &params);
        let rect = placed(&long, nesting.placements[0].as_ref().expect("turned"));
        assert!((rect.width() - 90.0).abs() < 1e-6, "{rect:?}");
        assert!(nesting.placements[1].is_none());
    }

    #[test]
    fn holes_and_engravings_stay_with_their_part() {
        let bounds = [
            rectangle(0.0, 0.0, 50.0, 50.0).bounds(),
            rectangle(10.0, 10.0, 5.0, 5.0).bounds(),
            rectangle(80.0, 0.0, 20.0, 20.0).bounds(),
            rectangle(85.0, 5.0, 2.0, 2.0).bounds(),
        ];
        let parts = group_parts(&bounds);
        assert_eq!(parts[0], parts[1]);
        assert_eq!(parts[2], parts[3]);
        assert_ne!(parts[0], parts[2]);
    }
}
