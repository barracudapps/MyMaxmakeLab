use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MachineKind {
    Laser,
    Cnc,
}

/// Corner of the bed where the machine has its home position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    FrontLeft,
    FrontRight,
    RearLeft,
    RearRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MachineProfile {
    pub name: String,
    pub kind: MachineKind,
    pub width: f64,
    pub height: f64,
    pub z_travel: f64,
    pub origin: Origin,
    /// GRBL `$30`: S value that means 100 % power or full spindle speed.
    pub max_s: f64,
    /// GRBL `$32`; laser jobs use M4 (dynamic power) when enabled.
    pub laser_mode: bool,
    pub start_gcode: String,
    pub end_gcode: String,
    /// Safe height for rapid moves above the stock, CNC only.
    pub safe_z: f64,
    /// Laser jobs run from the current head position instead of the
    /// machine origin.
    pub relative_to_current: bool,
}

impl Default for MachineProfile {
    fn default() -> Self {
        Self::generic_laser()
    }
}

impl MachineProfile {
    pub fn generic_laser() -> Self {
        Self {
            name: "GRBL laser".into(),
            kind: MachineKind::Laser,
            width: 400.0,
            height: 400.0,
            z_travel: 0.0,
            origin: Origin::FrontLeft,
            max_s: 1000.0,
            laser_mode: true,
            start_gcode: String::new(),
            end_gcode: String::new(),
            safe_z: 5.0,
            relative_to_current: false,
        }
    }

    pub fn generic_cnc() -> Self {
        Self {
            name: "GRBL CNC".into(),
            kind: MachineKind::Cnc,
            width: 300.0,
            height: 180.0,
            z_travel: 45.0,
            laser_mode: false,
            max_s: 24000.0,
            ..Self::generic_laser()
        }
    }

    /// Router with an interchangeable laser head (HiMill with iLam
    /// module and similar): CNC kinematics, laser mode for laser layers,
    /// jobs pause for the head change between them.
    pub fn generic_hybrid() -> Self {
        Self {
            name: "GRBL CNC + laser".into(),
            laser_mode: true,
            max_s: 1000.0,
            ..Self::generic_cnc()
        }
    }

    pub fn presets() -> Vec<MachineProfile> {
        vec![
            Self::generic_laser(),
            Self::generic_cnc(),
            Self::generic_hybrid(),
        ]
    }
}
