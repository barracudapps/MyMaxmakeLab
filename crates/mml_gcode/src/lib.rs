//! G-code for GRBL 1.1 controllers.
//!
//! The application previews the G-code it is about to send by parsing it
//! back with [`parse`], so the preview can never drift from the program.

mod analysis;
mod optimize;
mod parser;
mod writer;

pub use analysis::{JobStats, MotionSettings, analyze};
pub use optimize::{OptimizeOptions, OptimizeReport, optimize};
pub use parser::{Motion, MoveKind, Program, parse};
pub use writer::GcodeWriter;
