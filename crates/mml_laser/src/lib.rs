//! Laser toolpaths written straight to G-code.

mod dither;
mod fill;
mod image;
mod line;

pub use dither::{dither, prepare_levels};
pub use fill::{emit_fill, scan_intervals};
pub use image::{RasterGrid, emit_image, rasterize};
pub use line::{emit_line, emit_line_shapes};

/// Machine facts every laser operation needs.
#[derive(Debug, Clone, Copy)]
pub struct LaserSettings {
    /// S value for 100 % power (GRBL `$30`).
    pub max_s: f64,
    /// Use M4 so GRBL scales power with the actual speed.
    pub dynamic_power: bool,
}

impl LaserSettings {
    pub fn s_value(&self, percent: f64) -> f64 {
        (percent.clamp(0.0, 100.0) / 100.0 * self.max_s).round()
    }
}
