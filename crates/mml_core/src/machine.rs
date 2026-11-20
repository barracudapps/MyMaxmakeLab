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
    /// Automatic tool changer: CNC layers load their tool with `M6 T…`
    /// instead of pausing for the operator.
    pub tool_changer: bool,
    /// 5-axis table: A tilts the cradle, C turns the table on it.
    pub trunnion: Option<Trunnion>,
    /// Collet nut or spindle nose diameter, for collision checks, mm.
    pub holder_diameter: f64,
    /// Tool setter used to measure the real stick-out of each tool.
    pub tool_setter: ToolSetter,
}

/// Measures how far an installed tool really sticks out of the collet.
/// Calibrated once with a gauge of known stick-out touching the setter;
/// afterwards a longer tool touches with the spindle higher, by exactly
/// the difference of length.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolSetter {
    /// Stick-out of the calibration gauge, measured with calipers, mm.
    pub gauge_length: f64,
    /// Machine Z when the gauge touched the setter.
    pub reference_z: Option<f64>,
}

impl Default for ToolSetter {
    fn default() -> Self {
        Self {
            gauge_length: 30.0,
            reference_z: None,
        }
    }
}

impl ToolSetter {
    /// Real stick-out of a tool that touched the setter at machine Z
    /// `touch_z`; `None` until calibrated.
    pub fn stick_out(&self, touch_z: f64) -> Option<f64> {
        self.reference_z
            .map(|reference| self.gauge_length + (touch_z - reference))
    }
}

/// Table-table 5-axis kinematics (cradle tilting around X by A, table
/// turning around its own axis by C). Work zero is set on the pivot,
/// where the A and C axes cross; part coordinates have their origin
/// at the centre of the table surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Trunnion {
    /// Table surface above the A axis, mm (negative when below).
    pub table_height: f64,
    pub table_radius: f64,
    /// Travel of the tilt axis, degrees.
    pub a_min: f64,
    pub a_max: f64,
}

impl Default for Trunnion {
    fn default() -> Self {
        Self {
            table_height: 0.0,
            table_radius: 50.0,
            a_min: -100.0,
            a_max: 100.0,
        }
    }
}

impl Trunnion {
    /// Part point to work coordinates for the angles A and C, degrees.
    /// Positive angles follow the right-hand rule around +X and +Z.
    pub fn to_machine(&self, point: [f64; 3], a: f64, c: f64) -> [f64; 3] {
        let (sin_c, cos_c) = c.to_radians().sin_cos();
        let (sin_a, cos_a) = a.to_radians().sin_cos();
        let [x, y, z] = point;
        let z = z + self.table_height;
        let (x, y) = (x * cos_c - y * sin_c, x * sin_c + y * cos_c);
        [x, y * cos_a - z * sin_a, y * sin_a + z * cos_a]
    }

    pub fn to_part(&self, point: [f64; 3], a: f64, c: f64) -> [f64; 3] {
        let (sin_c, cos_c) = c.to_radians().sin_cos();
        let (sin_a, cos_a) = a.to_radians().sin_cos();
        let [x, y, z] = point;
        let (y, z) = (y * cos_a + z * sin_a, -y * sin_a + z * cos_a);
        let (x, y) = (x * cos_c + y * sin_c, -x * sin_c + y * cos_c);
        [x, y, z - self.table_height]
    }

    /// Angles that turn the part direction `normal` towards the tool
    /// (+Z), within the A travel; `None` when the machine cannot.
    pub fn orient(&self, normal: [f64; 3]) -> Option<(f64, f64)> {
        let [x, y, z] = normal;
        let length = (x * x + y * y + z * z).sqrt();
        if length < 1e-9 {
            return None;
        }
        let rho = x.hypot(y) / length;
        let a = rho.atan2(z / length).to_degrees();
        let c = if rho < 1e-9 {
            0.0
        } else {
            x.atan2(y).to_degrees()
        };
        let wrap = |angle: f64| 180.0 - (180.0 - angle).rem_euclid(360.0);
        [(a, wrap(c)), (-a, wrap(c + 180.0))]
            .into_iter()
            .find(|(a, _)| (self.a_min - 1e-9..=self.a_max + 1e-9).contains(a))
    }
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
            tool_changer: false,
            trunnion: None,
            holder_diameter: 20.0,
            tool_setter: ToolSetter::default(),
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

    /// Makera Carvera: Smoothieware based, 6-slot tool changer that
    /// also measures each tool, connected over Wi-Fi (port 2222) or USB.
    pub fn makera_carvera() -> Self {
        Self {
            name: "Makera Carvera".into(),
            width: 300.0,
            height: 200.0,
            z_travel: 140.0,
            max_s: 15000.0,
            tool_changer: true,
            ..Self::generic_cnc()
        }
    }

    pub fn presets() -> Vec<MachineProfile> {
        vec![
            Self::generic_laser(),
            Self::generic_cnc(),
            Self::generic_hybrid(),
            Self::makera_carvera(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trunnion_turns_each_face_to_the_tool_and_back() {
        let trunnion = Trunnion {
            table_height: 12.0,
            ..Trunnion::default()
        };
        for normal in [
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.3, 0.5, 0.8],
            [0.0, 0.0, 1.0],
        ] {
            let (a, c) = trunnion.orient(normal).unwrap();
            let base = trunnion.to_machine([0.0; 3], a, c);
            let tip = trunnion.to_machine(normal, a, c);
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            let up = [0, 1, 2].map(|axis| (tip[axis] - base[axis]) / length);
            assert!(
                up[0].abs() < 1e-9 && up[1].abs() < 1e-9 && (up[2] - 1.0).abs() < 1e-9,
                "{normal:?} -> {up:?}"
            );
            let point = [3.0, -7.0, 5.0];
            let back = trunnion.to_part(trunnion.to_machine(point, a, c), a, c);
            assert!((0..3).all(|axis| (back[axis] - point[axis]).abs() < 1e-9));
        }
        // Front face: A90 C180 or A-90 C0; a cradle tilting one way only
        // must use the other.
        assert_eq!(trunnion.orient([0.0, -1.0, 0.0]), Some((90.0, 180.0)));
        let one_way = Trunnion {
            a_min: -100.0,
            a_max: 10.0,
            ..trunnion
        };
        let (a, c) = one_way.orient([0.0, -1.0, 0.0]).unwrap();
        assert!((a + 90.0).abs() < 1e-9 && c.abs() < 1e-9);
        let limited = Trunnion {
            a_min: -30.0,
            a_max: 30.0,
            ..trunnion
        };
        assert_eq!(limited.orient([1.0, 0.0, 0.0]), None);
    }

    #[test]
    fn the_setter_gives_the_real_stick_out() {
        let mut setter = ToolSetter {
            gauge_length: 30.0,
            reference_z: None,
        };
        assert_eq!(setter.stick_out(-40.0), None, "not calibrated");
        setter.reference_z = Some(-45.0);
        // A tool touching 6 mm higher than the gauge sticks out 6 mm more.
        assert_eq!(setter.stick_out(-39.0), Some(36.0));
        assert_eq!(setter.stick_out(-52.5), Some(22.5));
    }
}
