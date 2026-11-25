//! Resizing a sample to the user's stock: the drawing is scaled, while
//! the slots keep the width of the plywood and the pin holes their 2 mm,
//! and the parts are shared out on boards of the user's size.
//!
//! A sample is built at its own size inside a [`Fit`]: slot widths,
//! pin holes and boards are divided by the scale while it is built, then
//! the whole document is scaled, so they come out at their real size.

use crate::shapes::{BOARD, BOARD_AT};
use std::cell::{Cell, RefCell};

/// Stock and size a sample is made for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// 1 is the size of the drawing in the guide.
    pub scale: f64,
    /// Plywood thickness, mm: the width of every slot.
    pub thickness: f64,
    /// Usable board, mm (width, height).
    pub board: (f64, f64),
}

impl Default for Fit {
    fn default() -> Self {
        Self {
            scale: 1.0,
            thickness: crate::puzzles3d::THICKNESS,
            board: BOARD,
        }
    }
}

impl Fit {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

thread_local! {
    static CURRENT: Cell<Fit> = Cell::new(Fit::default());
    /// Sizes of the parts handed out on boards, and how many did not fit.
    static PARTS: RefCell<(Vec<(f64, f64)>, usize)> = const { RefCell::new((Vec::new(), 0)) };
}

pub(crate) fn current() -> Fit {
    CURRENT.with(Cell::get)
}

/// Width of a slot in the drawing being built.
pub(crate) fn slot() -> f64 {
    let fit = current();
    fit.thickness / fit.scale
}

/// A length that must come out at `real` mm whatever the scale.
pub(crate) fn real(real: f64) -> f64 {
    real / current().scale
}

/// The board in the drawing being built.
pub(crate) fn board() -> (f64, f64) {
    let fit = current();
    (fit.board.0 / fit.scale, fit.board.1 / fit.scale)
}

/// Notes a part given to the boards, with the room it needs around.
pub(crate) fn note_part(width: f64, height: f64) {
    PARTS.with(|parts| parts.borrow_mut().0.push((width, height)));
}

/// Notes parts that no empty board could take.
pub(crate) fn note_too_big(count: usize) {
    PARTS.with(|parts| parts.borrow_mut().1 += count);
}

/// Runs `build` inside `fit`, and returns its result with the sizes of
/// the parts and how many found no board.
pub(crate) fn within<T>(fit: Fit, build: impl FnOnce() -> T) -> (T, Vec<(f64, f64)>, usize) {
    let before = CURRENT.with(|current| current.replace(fit));
    PARTS.with(|parts| *parts.borrow_mut() = (Vec::new(), 0));
    let result = build();
    CURRENT.with(|current| current.set(before));
    let (sizes, dropped) = PARTS.with(|parts| std::mem::take(&mut *parts.borrow_mut()));
    (result, sizes, dropped)
}

/// Margin the boards keep around their parts, in the drawing.
pub(crate) const MARGIN: f64 = 5.0;

/// The largest scale at which every part (of the sizes given, at scale
/// 1) fits a board, turned if need be.
pub fn largest_scale(sizes: &[(f64, f64)], board: (f64, f64)) -> f64 {
    sizes
        .iter()
        .map(|(width, height)| {
            // Scaled, the board margin shrinks with the part.
            let upright = (board.0 / (width + 2.0 * MARGIN)).min(board.1 / (height + 2.0 * MARGIN));
            let turned = (board.0 / (height + 2.0 * MARGIN)).min(board.1 / (width + 2.0 * MARGIN));
            upright.max(turned)
        })
        .fold(f64::INFINITY, f64::min)
        // The nesting works on a grid: leave it a little room.
        * 0.97
}

/// Where scaling leaves things in place: the board corner.
pub(crate) fn anchor() -> mml_core::geometry::Point {
    mml_core::geometry::Point::new(BOARD_AT.0, BOARD_AT.1)
}

/// A length for texts: "3", "2.5" (decimal comma but in English).
pub fn format_mm(language: &str, value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if language == "en" {
        text
    } else {
        text.replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_largest_scale_lets_the_biggest_part_fit_turned() {
        // 200 x 100 on a board of 105 x 210: it fits once turned, at the
        // scale where 100 + margins fills 105.
        let scale = largest_scale(&[(200.0, 100.0)], (105.0, 210.0));
        assert!((scale - 105.0 / 110.0 * 0.97).abs() < 1e-9);
        assert_eq!(largest_scale(&[], (100.0, 100.0)), f64::INFINITY);
    }

    #[test]
    fn slots_and_boards_follow_the_fit_while_building() {
        let fit = Fit {
            scale: 0.5,
            thickness: 2.0,
            board: (200.0, 140.0),
        };
        let ((slot, board), _, _) = within(fit, || (slot(), board()));
        assert_eq!(slot, 4.0);
        assert_eq!(board, (400.0, 280.0));
        assert_eq!(current(), Fit::default());
        assert_eq!(format_mm("fr", 2.5, 1), "2,5");
        assert_eq!(format_mm("en", 3.0, 0), "3");
    }
}
