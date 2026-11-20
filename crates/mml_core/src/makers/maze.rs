//! Mazes: a perfect maze (one path between any two cells) carved by a
//! randomised depth-first search, output either as its walls (laser
//! engraving or cutting) or as its corridors (centre lines milled with
//! a flat or ball cutter as wide as a corridor, for a marble maze).

use crate::geometry::{Path, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MazeOutput {
    Walls,
    Corridors,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MazeParams {
    pub columns: usize,
    pub rows: usize,
    /// Size of one cell, mm.
    pub cell: f64,
    pub output: MazeOutput,
    pub seed: u64,
}

impl Default for MazeParams {
    fn default() -> Self {
        Self {
            columns: 15,
            rows: 10,
            cell: 8.0,
            output: MazeOutput::Walls,
            seed: 1,
        }
    }
}

pub struct Maze {
    pub paths: Vec<Path>,
    /// Centre line from the entrance to the exit.
    pub solution: Path,
}

struct Random(u64);

impl Random {
    fn below(&mut self, count: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) % count.max(1) as u64) as usize
    }
}

/// Carves the maze: `open[cell]` lists the open sides (0 east, 1
/// north, 2 west, 3 south) and `parent` the search tree.
fn carve(columns: usize, rows: usize, seed: u64) -> (Vec<[bool; 4]>, Vec<Option<usize>>) {
    let mut random = Random(seed ^ 0x0A2E);
    let count = columns * rows;
    let mut open = vec![[false; 4]; count];
    let mut parent = vec![None; count];
    let mut visited = vec![false; count];
    let mut stack = vec![0usize];
    visited[0] = true;
    while let Some(&cell) = stack.last() {
        let (x, y) = (cell % columns, cell / columns);
        let neighbours: Vec<(usize, usize)> = [
            (x + 1 < columns).then(|| (0, cell + 1)),
            (y + 1 < rows).then(|| (1, cell + columns)),
            (x > 0).then(|| (2, cell - 1)),
            (y > 0).then(|| (3, cell - columns)),
        ]
        .into_iter()
        .flatten()
        .filter(|(_, next)| !visited[*next])
        .collect();
        if neighbours.is_empty() {
            stack.pop();
            continue;
        }
        let (side, next) = neighbours[random.below(neighbours.len())];
        open[cell][side] = true;
        open[next][(side + 2) % 4] = true;
        visited[next] = true;
        parent[next] = Some(cell);
        stack.push(next);
    }
    (open, parent)
}

pub fn generate(params: &MazeParams) -> Maze {
    let (columns, rows) = (params.columns.max(2), params.rows.max(2));
    let cell = params.cell.max(0.5);
    let (open, parent) = carve(columns, rows, params.seed);
    let centre = |index: usize| {
        Point::new(
            ((index % columns) as f64 + 0.5) * cell,
            ((index / columns) as f64 + 0.5) * cell,
        )
    };
    // Entrance on the left of the bottom row, exit on the right of the
    // top row.
    let (entrance, exit) = (0, columns * rows - 1);
    let mut solution = vec![centre(exit) + Point::new(cell / 2.0, 0.0), centre(exit)];
    let mut walk = exit;
    while let Some(previous) = parent[walk] {
        solution.push(centre(previous));
        walk = previous;
    }
    solution.push(centre(entrance) - Point::new(cell / 2.0, 0.0));
    solution.reverse();

    let mut paths = Vec::new();
    match params.output {
        MazeOutput::Walls => {
            // Horizontal walls row by row, merged into runs.
            for line in 0..=rows {
                let mut run: Option<usize> = None;
                for column in 0..=columns {
                    // Outer edges are always walls.
                    let wall = column < columns
                        && (line == 0 || line == rows || !open[line * columns + column][3]);
                    match (wall, run) {
                        (true, None) => run = Some(column),
                        (false, Some(start)) => {
                            paths.push(segment(start, line, column, line, cell));
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
            for line in 0..=columns {
                let mut run: Option<usize> = None;
                for row in 0..=rows {
                    let wall = row < rows
                        && if line == 0 {
                            row != 0
                        } else if line == columns {
                            row != rows - 1
                        } else {
                            !open[row * columns + line][2]
                        };
                    match (wall, run) {
                        (true, None) => run = Some(row),
                        (false, Some(start)) => {
                            paths.push(segment(line, start, line, row, cell));
                            run = None;
                        }
                        _ => {}
                    }
                }
            }
        }
        MazeOutput::Corridors => {
            // Each branch of the tree as one polyline: follow a child
            // chain until it ends, starting new chains at forks.
            let mut children = vec![Vec::new(); columns * rows];
            for (child, parent) in parent.iter().enumerate() {
                if let Some(parent) = parent {
                    children[*parent].push(child);
                }
            }
            let mut branches: Vec<(usize, usize)> =
                children[entrance].iter().map(|&child| (entrance, child)).collect();
            while let Some((from, child)) = branches.pop() {
                let mut line = vec![centre(from), centre(child)];
                let mut here = child;
                while let Some((&next, rest)) = children[here].split_first() {
                    branches.extend(rest.iter().map(|&other| (here, other)));
                    line.push(centre(next));
                    here = next;
                }
                paths.push(Path::new(line, false));
            }
            paths.push(Path::new(
                vec![
                    centre(entrance) - Point::new(cell / 2.0, 0.0),
                    centre(entrance),
                ],
                false,
            ));
            paths.push(Path::new(
                vec![centre(exit), centre(exit) + Point::new(cell / 2.0, 0.0)],
                false,
            ));
        }
    }
    Maze {
        paths,
        solution: Path::new(solution, false),
    }
}

fn segment(x0: usize, y0: usize, x1: usize, y1: usize, cell: f64) -> Path {
    Path::new(
        vec![
            Point::new(x0 as f64 * cell, y0 as f64 * cell),
            Point::new(x1 as f64 * cell, y1 as f64 * cell),
        ],
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_maze_is_perfect_and_its_solution_crosses_no_wall() {
        let params = MazeParams {
            columns: 12,
            rows: 9,
            cell: 5.0,
            ..MazeParams::default()
        };
        let (open, parent) = carve(12, 9, 1);
        // A spanning tree: every cell but the root has a parent and the
        // open sides count two per edge.
        assert_eq!(parent.iter().filter(|p| p.is_none()).count(), 1);
        let sides: usize = open
            .iter()
            .map(|cell| cell.iter().filter(|side| **side).count())
            .sum();
        assert_eq!(sides, 2 * (12 * 9 - 1));
        let maze = generate(&params);
        // No solution step goes through a wall.
        let walls = &maze.paths;
        for step in maze.solution.points.windows(2) {
            let middle = Point::new((step[0].x + step[1].x) / 2.0, (step[0].y + step[1].y) / 2.0);
            for wall in walls {
                let (a, b) = (wall.points[0], wall.points[1]);
                let on_wall = if (a.y - b.y).abs() < 1e-9 {
                    (middle.y - a.y).abs() < 1e-9
                        && middle.x > a.x.min(b.x)
                        && middle.x < a.x.max(b.x)
                } else {
                    (middle.x - a.x).abs() < 1e-9
                        && middle.y > a.y.min(b.y)
                        && middle.y < a.y.max(b.y)
                };
                assert!(!on_wall, "solution crosses a wall at {middle:?}");
            }
        }
        let start = maze.solution.points[0];
        let end = *maze.solution.points.last().unwrap();
        assert!(start.x < 0.0 + 1e-9 && end.x > 60.0 - 1e-9);
    }

    #[test]
    fn corridors_cover_every_cell_once() {
        let params = MazeParams {
            columns: 8,
            rows: 6,
            output: MazeOutput::Corridors,
            ..MazeParams::default()
        };
        let maze = generate(&params);
        let segments: usize = maze.paths.iter().map(|path| path.points.len() - 1).sum();
        // Tree edges plus the entrance and exit stubs.
        assert_eq!(segments, 8 * 6 - 1 + 2);
        let other = generate(&MazeParams { seed: 2, ..params });
        assert_ne!(maze.solution.points.len() + maze.paths.len(), 0,);
        assert!(other.paths.len() > 1);
    }
}
