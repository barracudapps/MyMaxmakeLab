use crate::ImportError;
use mml_core::document::RasterImage;

/// Longest side kept in memory; engraving finer than this is pointless
/// for a desktop laser and keeps undo snapshots light.
const MAX_SIDE: u32 = 6000;

/// Loads a picture as greyscale, transparent areas becoming white.
/// Returns the image and its natural size in mm (96 dpi when the file
/// carries no resolution).
pub fn import_raster(data: &[u8]) -> Result<(RasterImage, f64, f64), ImportError> {
    let mut picture =
        image::load_from_memory(data).map_err(|error| ImportError::Image(error.to_string()))?;
    if picture.width().max(picture.height()) > MAX_SIDE {
        picture = picture.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Lanczos3);
    }
    let luma = picture.to_luma_alpha8();
    let (width, height) = luma.dimensions();
    let pixels = luma
        .pixels()
        .map(|pixel| {
            let [value, alpha] = pixel.0;
            let alpha = alpha as f32 / 255.0;
            (value as f32 * alpha + 255.0 * (1.0 - alpha)).round() as u8
        })
        .collect();
    let millimetres_per_pixel = 25.4 / 96.0;
    Ok((
        RasterImage {
            width,
            height,
            pixels,
        },
        width as f64 * millimetres_per_pixel,
        height as f64 * millimetres_per_pixel,
    ))
}
