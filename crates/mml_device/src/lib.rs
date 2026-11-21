//! GRBL 1.1 machine control. A worker thread owns the connection; the
//! user interface talks to it through channels and never blocks.

mod controller;
pub mod detect;
pub mod discovery;
pub mod homing;
pub mod jog;
mod messages;
pub mod safety;
mod status;

pub use controller::{
    Command, Connection, Controller, Event, JobOutcome, Protocol, list_serial_ports,
};
pub use messages::{alarm_message, error_message, halt_message};
pub use safety::{Anomaly, JobGuard, travel_violations};
pub use status::{MachineState, MachineStatus, parse_status};

/// GRBL real-time commands, sent outside the line buffer.
pub mod realtime {
    pub const STATUS: u8 = b'?';
    pub const FEED_HOLD: u8 = b'!';
    pub const CYCLE_START: u8 = b'~';
    pub const SOFT_RESET: u8 = 0x18;
    pub const JOG_CANCEL: u8 = 0x85;
    pub const FEED_100: u8 = 0x90;
    pub const FEED_PLUS_10: u8 = 0x91;
    pub const FEED_MINUS_10: u8 = 0x92;
    pub const SPINDLE_100: u8 = 0x99;
    pub const SPINDLE_PLUS_10: u8 = 0x9A;
    pub const SPINDLE_MINUS_10: u8 = 0x9B;
}
