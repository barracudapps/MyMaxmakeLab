//! Laser test grid: squares burnt at increasing power (rows) and speed
//! (columns), to find the settings of a new material in one run.

use crate::geometry::{Path, Point};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestGridParams {
    /// Power range in percent, bottom row to top row.
    pub power: (f64, f64),
    /// Speed range in mm/min, left column to right column.
    pub speed: (f64, f64),
    pub rows: usize,
    pub columns: usize,
    /// Side of a square, mm.
    pub cell: f64,
    pub gap: f64,
}

impl Default for TestGridParams {
    fn default() -> Self {
        Self {
            power: (10.0, 100.0),
            speed: (600.0, 6000.0),
            rows: 5,
            columns: 5,
            cell: 10.0,
            gap: 3.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCell {
    pub power: f64,
    pub speed: f64,
    pub square: Path,
}

fn step(range: (f64, f64), index: usize, count: usize) -> f64 {
    if count <= 1 {
        return range.0;
    }
    range.0 + (range.1 - range.0) * index as f64 / (count - 1) as f64
}

/// The squares, lower-left at the origin; labels go left of the rows and
/// above the columns (negative X and above the grid).
pub fn cells(params: &TestGridParams) -> Vec<TestCell> {
    let (rows, columns) = (params.rows.max(1), params.columns.max(1));
    let pitch = params.cell + params.gap;
    let mut out = Vec::with_capacity(rows * columns);
    for row in 0..rows {
        for column in 0..columns {
            let (x, y) = (column as f64 * pitch, row as f64 * pitch);
            out.push(TestCell {
                power: step(params.power, row, rows).round(),
                speed: step(params.speed, column, columns).round(),
                square: Path::new(
                    vec![
                        Point::new(x, y),
                        Point::new(x + params.cell, y),
                        Point::new(x + params.cell, y + params.cell),
                        Point::new(x, y + params.cell),
                    ],
                    true,
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_hold_the_extreme_settings() {
        let params = TestGridParams::default();
        let grid = cells(&params);
        assert_eq!(grid.len(), 25);
        assert_eq!((grid[0].power, grid[0].speed), (10.0, 600.0));
        assert_eq!((grid[24].power, grid[24].speed), (100.0, 6000.0));
        let last = grid[24].square.bounds();
        assert!((last.max.x - (4.0 * 13.0 + 10.0)).abs() < 1e-9);
    }
}
