//! CNC toolpaths. Z0 is the top of the stock, depths are positive
//! values going down.

mod emit;

pub use emit::{CncSettings, emit_pocket, emit_profile, pass_depths};
pub use mml_core::region::{offset, region_of};
