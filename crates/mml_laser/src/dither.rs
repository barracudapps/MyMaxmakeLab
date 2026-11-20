use mml_core::layer::{DitherMode, ImageParams};

/// Applies brightness, contrast and negative to grey values in 0..1
/// (1 = white) and returns burn levels in 0..1 (1 = full power).
pub fn prepare_levels(grey: &[f32], params: &ImageParams) -> Vec<f32> {
    let brightness = (params.brightness / 200.0) as f32;
    let contrast = ((params.contrast.clamp(-99.0, 99.0) / 100.0 + 1.0)
        * std::f64::consts::FRAC_PI_4)
        .tan() as f32;
    grey.iter()
        .map(|value| {
            let mut value = (value - 0.5) * contrast + 0.5 + brightness;
            if params.negative {
                value = 1.0 - value;
            }
            1.0 - value.clamp(0.0, 1.0)
        })
        .collect()
}

const BAYER_8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// (dx, dy, weight) error-diffusion kernels and their divisor.
fn kernel(mode: DitherMode) -> (&'static [(i32, i32, f32)], f32) {
    match mode {
        DitherMode::FloydSteinberg => {
            (&[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], 16.0)
        }
        DitherMode::Atkinson => (
            &[
                (1, 0, 1.0),
                (2, 0, 1.0),
                (-1, 1, 1.0),
                (0, 1, 1.0),
                (1, 1, 1.0),
                (0, 2, 1.0),
            ],
            8.0,
        ),
        DitherMode::Jarvis => (
            &[
                (1, 0, 7.0),
                (2, 0, 5.0),
                (-2, 1, 3.0),
                (-1, 1, 5.0),
                (0, 1, 7.0),
                (1, 1, 5.0),
                (2, 1, 3.0),
                (-2, 2, 1.0),
                (-1, 2, 3.0),
                (0, 2, 5.0),
                (1, 2, 3.0),
                (2, 2, 1.0),
            ],
            48.0,
        ),
        DitherMode::Stucki => (
            &[
                (1, 0, 8.0),
                (2, 0, 4.0),
                (-2, 1, 2.0),
                (-1, 1, 4.0),
                (0, 1, 8.0),
                (1, 1, 4.0),
                (2, 1, 2.0),
                (-2, 2, 1.0),
                (-1, 2, 2.0),
                (0, 2, 4.0),
                (1, 2, 2.0),
                (2, 2, 1.0),
            ],
            42.0,
        ),
        _ => (&[], 1.0),
    }
}

/// Turns burn levels into the levels actually sent: unchanged for
/// greyscale, 0 or 1 for every other mode.
pub fn dither(
    levels: &[f32],
    width: usize,
    height: usize,
    mode: DitherMode,
    threshold: u8,
) -> Vec<f32> {
    let cut = 1.0 - threshold as f32 / 255.0;
    match mode {
        DitherMode::Grayscale => levels.to_vec(),
        DitherMode::Threshold => levels.iter().map(|&level| f32::from(level > cut)).collect(),
        DitherMode::Halftone => levels
            .iter()
            .enumerate()
            .map(|(index, &level)| {
                // Screen rotated 45°, one dot every 6 cells.
                let (x, y) = ((index % width) as f32, (index / width) as f32);
                let (u, v) = (
                    (x + y) * std::f32::consts::FRAC_1_SQRT_2,
                    (x - y) * std::f32::consts::FRAC_1_SQRT_2,
                );
                let period = std::f32::consts::TAU / 6.0;
                let limit = ((u * period).cos() + (v * period).cos() + 2.0) / 4.0;
                f32::from(level > limit)
            })
            .collect(),
        DitherMode::LineScreen => levels
            .iter()
            .enumerate()
            .map(|(index, &level)| {
                let y = (index / width) as f32;
                // Triangle profile: thresholds evenly spread, so the line
                // width is proportional to the darkness.
                let limit = (2.0 * ((y + 0.5) / 6.0).fract() - 1.0).abs();
                f32::from(level > limit)
            })
            .collect(),
        DitherMode::Ordered => levels
            .iter()
            .enumerate()
            .map(|(index, &level)| {
                let (x, y) = (index % width, index / width);
                let limit = (BAYER_8[y % 8][x % 8] as f32 + 0.5) / 64.0;
                f32::from(level > limit)
            })
            .collect(),
        _ => {
            let (taps, divisor) = kernel(mode);
            let mut buffer = levels.to_vec();
            for y in 0..height {
                for x in 0..width {
                    let index = y * width + x;
                    let old = buffer[index];
                    let new = if old > 0.5 { 1.0 } else { 0.0 };
                    buffer[index] = new;
                    let error = old - new;
                    for &(dx, dy, weight) in taps {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx >= 0 && (nx as usize) < width && (ny as usize) < height {
                            buffer[ny as usize * width + nx as usize] += error * weight / divisor;
                        }
                    }
                }
            }
            buffer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_diffusion_preserves_mean_tone() {
        let levels = vec![0.25f32; 64 * 64];
        for mode in [
            DitherMode::FloydSteinberg,
            DitherMode::Jarvis,
            DitherMode::Stucki,
        ] {
            let out = dither(&levels, 64, 64, mode, 128);
            let mean = out.iter().sum::<f32>() / out.len() as f32;
            assert!((mean - 0.25).abs() < 0.02, "{mode:?} mean {mean}");
            assert!(out.iter().all(|&value| value == 0.0 || value == 1.0));
        }
    }

    #[test]
    fn screens_follow_the_tone() {
        for mode in [DitherMode::Halftone, DitherMode::LineScreen] {
            let light = dither(&vec![0.2f32; 60 * 60], 60, 60, mode, 128);
            let dark = dither(&vec![0.8f32; 60 * 60], 60, 60, mode, 128);
            let mean = |values: &[f32]| values.iter().sum::<f32>() / values.len() as f32;
            assert!(mean(&light) < 0.4 && mean(&dark) > 0.6, "{mode:?}");
        }
    }
}
