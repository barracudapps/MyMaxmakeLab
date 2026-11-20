use crate::LaserSettings;
use crate::dither::{dither, prepare_levels};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Point, Transform};
use mml_core::layer::{DitherMode, ImageParams};
use mml_gcode::GcodeWriter;

/// Burn levels on the engraving grid, row 0 at the top of the picture.
#[derive(Debug, Clone)]
pub struct RasterGrid {
    pub columns: usize,
    pub rows: usize,
    /// Grid pitch in mm.
    pub step: f64,
    pub levels: Vec<f32>,
    /// Maps grid millimetres (x right, y down from the top-left corner)
    /// to machine coordinates.
    pub to_world: Transform,
}

/// Resamples the picture of `shape` at the engraving resolution and
/// dithers it. The grid follows the shape rotation, so rotated pictures
/// are engraved along their own axes.
pub fn rasterize(shape: &Shape, params: &ImageParams) -> Option<RasterGrid> {
    let ShapeKind::Image {
        image,
        width,
        height,
    } = &shape.kind
    else {
        return None;
    };
    let (sx, sy) = shape.transform.scale_factors();
    let world_width = width * sx.abs();
    let world_height = height * sy.abs();
    let step = 25.4 / params.dpi.clamp(25.0, 1270.0);
    let columns = (world_width / step).round().max(1.0) as usize;
    let rows = (world_height / step).round().max(1.0) as usize;
    if columns * rows > 60_000_000 {
        return None;
    }
    let (source_width, source_height) = (image.width as f64, image.height as f64);
    let mut grey = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        let v = (row as f64 + 0.5) / rows as f64 * source_height - 0.5;
        for column in 0..columns {
            let u = (column as f64 + 0.5) / columns as f64 * source_width - 0.5;
            grey.push(bilinear(image, u, v) / 255.0);
        }
    }
    let levels = prepare_levels(&grey, params);
    let levels = dither(&levels, columns, rows, params.dither, params.threshold);
    // Grid mm -> shape local (centred, Y up) -> machine.
    let to_local = Transform {
        a: width / world_width,
        b: 0.0,
        c: 0.0,
        d: -height / world_height,
        e: -width / 2.0,
        f: height / 2.0,
    };
    Some(RasterGrid {
        columns,
        rows,
        step,
        levels,
        to_world: to_local.then(shape.transform),
    })
}

fn bilinear(image: &mml_core::RasterImage, u: f64, v: f64) -> f32 {
    let max_x = image.width as f64 - 1.0;
    let max_y = image.height as f64 - 1.0;
    let (u, v) = (u.clamp(0.0, max_x), v.clamp(0.0, max_y));
    let (x0, y0) = (u.floor() as u32, v.floor() as u32);
    let (x1, y1) = (
        (x0 + 1).min(image.width - 1),
        (y0 + 1).min(image.height - 1),
    );
    let (fx, fy) = ((u - x0 as f64) as f32, (v - y0 as f64) as f32);
    let top = image.pixel(x0, y0) as f32 * (1.0 - fx) + image.pixel(x1, y0) as f32 * fx;
    let bottom = image.pixel(x0, y1) as f32 * (1.0 - fx) + image.pixel(x1, y1) as f32 * fx;
    top * (1.0 - fy) + bottom * fy
}

pub fn emit_image(
    writer: &mut GcodeWriter,
    head: &mut Point,
    shape: &Shape,
    params: &ImageParams,
    settings: &LaserSettings,
) {
    let Some(grid) = rasterize(shape, params) else {
        return;
    };
    let power_of = |level: f32| -> f64 {
        if level <= 1.0 / 512.0 {
            return 0.0;
        }
        let percent = if params.dither == DitherMode::Grayscale {
            params.power_min + (params.power_max - params.power_min) * level as f64
        } else {
            params.power_max
        };
        settings.s_value(percent)
    };
    if params.air_assist {
        writer.air(true);
    }
    writer.laser_on(settings.dynamic_power);
    let step = grid.step;
    let mut started = false;
    for _ in 0..params.passes.max(1) {
        let mut forward = true;
        for row in 0..grid.rows {
            let line = &grid.levels[row * grid.columns..(row + 1) * grid.columns];
            let powers: Vec<f64> = line.iter().map(|&level| power_of(level)).collect();
            let Some(first) = powers.iter().position(|&power| power > 0.0) else {
                continue;
            };
            let last = powers.iter().rposition(|&power| power > 0.0).unwrap();
            let y = (row as f64 + 0.5) * step;
            let point = |x: f64| grid.to_world.apply(Point::new(x, y));
            // Runs of equal power as (start column, end column, power).
            let mut runs = Vec::new();
            let mut start = first;
            for column in first + 1..=last + 1 {
                if column > last || powers[column] != powers[start] {
                    runs.push((start, column, powers[start]));
                    start = column;
                }
            }
            if !forward {
                runs.reverse();
            }
            let (sign, entry_x) = if forward {
                (1.0, first as f64 * step)
            } else {
                (-1.0, (last + 1) as f64 * step)
            };
            let entry = point(entry_x - sign * params.overscan);
            if started {
                writer.linear(entry.x, entry.y, params.speed, Some(0.0));
            } else {
                writer.rapid(entry.x, entry.y);
                started = true;
            }
            let begin = point(entry_x);
            writer.linear(begin.x, begin.y, params.speed, Some(0.0));
            for (from, to, power) in runs {
                let end_x = if forward {
                    to as f64 * step
                } else {
                    from as f64 * step
                };
                let end = point(end_x);
                writer.linear(end.x, end.y, params.speed, Some(power));
            }
            let exit_x = if forward {
                (last + 1) as f64 * step
            } else {
                first as f64 * step
            };
            let exit = point(exit_x + sign * params.overscan);
            writer.linear(exit.x, exit.y, params.speed, Some(0.0));
            *head = exit;
            if params.bidirectional {
                forward = !forward;
            }
        }
    }
    writer.tool_off();
    if params.air_assist {
        writer.air(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::document::{Document, RasterImage};
    use std::sync::Arc;

    #[test]
    fn black_pixels_burn_where_the_picture_is() {
        // 2x1 picture: black on the left, white on the right.
        let image = RasterImage {
            width: 2,
            height: 1,
            pixels: vec![0, 255],
        };
        let document = Document::default();
        let mut shape = Shape::new(
            "img",
            document.layers[0].id,
            ShapeKind::Image {
                image: Arc::new(image),
                width: 10.0,
                height: 1.0,
            },
        );
        shape.transform = Transform::translate(5.0, 0.5);
        let params = ImageParams {
            dpi: 25.4,
            dither: DitherMode::Threshold,
            overscan: 0.0,
            ..ImageParams::default()
        };
        let mut writer = GcodeWriter::new();
        let settings = LaserSettings {
            max_s: 1000.0,
            dynamic_power: true,
        };
        emit_image(
            &mut writer,
            &mut Point::default(),
            &shape,
            &params,
            &settings,
        );
        let program = mml_gcode::parse(&writer.finish());
        let bounds = program.cut_bounds();
        assert!(bounds.min.x.abs() < 1e-9);
        assert!((bounds.max.x - 5.0).abs() < 1e-9);
    }
}
