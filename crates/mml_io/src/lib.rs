//! File import. Everything is returned in millimetres, machine axes
//! (Y up), with the drawing's bottom-left corner at the origin.

pub mod camera;
mod dxf_import;
mod hershey;
mod raster;
mod svg_import;
mod text;
mod trace;
pub mod vision;

pub use dxf_import::import_dxf;
pub use hershey::{hershey_families, hershey_text, is_hershey};
pub use raster::import_raster;
pub use svg_import::import_svg;
pub use text::{TextStyle, add_font_directory, bundled_families, font_families, text_to_paths};
pub use trace::trace_image;

use mml_core::geometry::Path;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid SVG: {0}")]
    Svg(String),
    #[error("invalid DXF: {0}")]
    Dxf(String),
    #[error("invalid image: {0}")]
    Image(String),
    #[error("the file contains no usable geometry")]
    Empty,
}

/// Paths sharing one colour, so that each colour can go to its own
/// layer like in LightBurn.
#[derive(Debug, Clone)]
pub struct ImportedGroup {
    pub color: Option<[u8; 3]>,
    pub paths: Vec<Path>,
}

pub(crate) const TOLERANCE_MM: f64 = 0.02;
