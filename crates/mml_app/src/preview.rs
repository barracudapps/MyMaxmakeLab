//! Renders a parsed program into a picture: it looks like the burned
//! result and scales to millions of moves, unlike drawing vectors.

use eframe::egui::{Color32, ColorImage};
use mml_core::geometry::Rect;
use mml_gcode::{MoveKind, Program};

pub struct PreviewImage {
    pub image: ColorImage,
    pub bounds: Rect,
}

/// `done_line`: moves up to this source line are drawn in the strong
/// colour (progress while running or simulating), the rest faint.
pub fn render(
    program: &Program,
    max_s: f64,
    done_line: Option<usize>,
    cnc: bool,
    dark: bool,
) -> Option<PreviewImage> {
    let mut bounds = program.cut_bounds();
    if bounds.is_empty() {
        return None;
    }
    let margin = 1.0;
    bounds.min.x -= margin;
    bounds.min.y -= margin;
    bounds.max.x += margin;
    bounds.max.y += margin;
    let longest = bounds.width().max(bounds.height());
    let pixel = (longest / 2048.0).max(0.05);
    let width = (bounds.width() / pixel).ceil() as usize + 1;
    let height = (bounds.height() / pixel).ceil() as usize + 1;
    let mut strong = vec![0u8; width * height];
    let mut faint = vec![0u8; width * height];
    let min_z = program
        .motions
        .iter()
        .map(|motion| motion.to[2])
        .fold(0.0f64, f64::min);
    for motion in program
        .motions
        .iter()
        .filter(|motion| motion.kind == MoveKind::Feed && motion.power > 0.0)
    {
        let intensity = if cnc {
            let depth = if min_z < 0.0 {
                (-motion.to[2] / -min_z).clamp(0.0, 1.0)
            } else {
                1.0
            };
            (90.0 + 165.0 * depth) as u8
        } else {
            ((motion.power / max_s.max(1.0)).clamp(0.0, 1.0) * 255.0) as u8
        };
        let target = if done_line.is_none_or(|line| motion.line <= line) {
            &mut strong
        } else {
            &mut faint
        };
        let (x0, y0) = (
            (motion.from[0] - bounds.min.x) / pixel,
            (bounds.max.y - motion.from[1]) / pixel,
        );
        let (x1, y1) = (
            (motion.to[0] - bounds.min.x) / pixel,
            (bounds.max.y - motion.to[1]) / pixel,
        );
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()) * 2.0).ceil().max(1.0) as usize;
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let x = (x0 + (x1 - x0) * t) as usize;
            let y = (y0 + (y1 - y0) * t) as usize;
            if x < width && y < height {
                let slot = &mut target[y * width + x];
                *slot = (*slot).max(intensity);
            }
        }
    }
    // Burn colours readable on both canvas themes.
    let (strong_color, faint_color) = match (cnc, dark) {
        (true, true) => ([90, 170, 255], [120, 150, 190]),
        (true, false) => ([30, 110, 200], [120, 150, 190]),
        (false, true) => ([255, 176, 80], [170, 140, 110]),
        (false, false) => ([70, 35, 10], [150, 120, 95]),
    };
    let pixels = strong
        .iter()
        .zip(&faint)
        .map(|(&strong_value, &faint_value)| {
            if strong_value > 0 {
                let [r, g, b] = strong_color;
                Color32::from_rgba_unmultiplied(r, g, b, strong_value.max(40))
            } else if faint_value > 0 {
                let [r, g, b] = faint_color;
                Color32::from_rgba_unmultiplied(r, g, b, (faint_value / 3).max(20))
            } else {
                Color32::TRANSPARENT
            }
        })
        .collect();
    Some(PreviewImage {
        image: ColorImage::new([width, height], pixels),
        bounds,
    })
}
