//! Geometry and document model shared by every MyMaxmakeLab crate.
//!
//! All lengths are millimetres, all feeds are millimetres per minute and
//! the coordinate system is the machine one: origin at the front-left
//! corner of the bed, X to the right, Y to the back.

pub mod attach;
pub mod brush;
pub mod document;
pub mod geometry;
pub mod history;
pub mod layer;
pub mod machine;
pub mod makers;
pub mod ordering;
pub mod ornament;
pub mod project;
pub mod region;
pub mod stats;
pub mod warp;

pub use document::{Document, RasterImage, Shape, ShapeId, ShapeKind};
pub use geometry::{Path, Point, Rect, Transform};
pub use history::History;
pub use layer::{
    CncPocketParams, CncProfileParams, DitherMode, FillParams, ImageParams, Layer, LayerId,
    LineParams, Operation, ProfileSide, VCarveParams,
};
pub use machine::{MachineKind, MachineProfile, Origin};
pub use project::{Project, ProjectError};
