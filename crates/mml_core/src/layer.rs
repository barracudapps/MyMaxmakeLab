use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerId(pub Uuid);

impl LayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LayerId {
    fn default() -> Self {
        Self::new()
    }
}

/// A layer groups shapes that share one machining operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub color: [u8; 3],
    pub visible: bool,
    /// Excluded layers stay on the canvas but produce no G-code.
    pub output: bool,
    pub operation: Operation,
}

impl Layer {
    pub fn new(name: impl Into<String>, color: [u8; 3], operation: Operation) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            color,
            visible: true,
            output: true,
            operation,
        }
    }
}

/// Palette used for new layers, readable on light and dark canvases.
pub const LAYER_COLORS: [[u8; 3]; 10] = [
    [33, 102, 172],
    [214, 39, 40],
    [44, 160, 44],
    [255, 127, 14],
    [148, 103, 189],
    [23, 190, 207],
    [140, 86, 75],
    [227, 119, 194],
    [127, 127, 127],
    [188, 189, 34],
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    LaserLine(LineParams),
    LaserFill(FillParams),
    LaserImage(ImageParams),
    CncProfile(CncProfileParams),
    CncPocket(CncPocketParams),
}

impl Operation {
    pub fn is_laser(&self) -> bool {
        matches!(
            self,
            Self::LaserLine(_) | Self::LaserFill(_) | Self::LaserImage(_)
        )
    }

    pub fn kind_key(&self) -> &'static str {
        match self {
            Self::LaserLine(_) => "op.laser_line",
            Self::LaserFill(_) => "op.laser_fill",
            Self::LaserImage(_) => "op.laser_image",
            Self::CncProfile(_) => "op.cnc_profile",
            Self::CncPocket(_) => "op.cnc_pocket",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineParams {
    /// mm/min
    pub speed: f64,
    /// Percent of the machine maximum power.
    pub power: f64,
    pub passes: u32,
    pub air_assist: bool,
    /// Lowers Z by this amount after each pass (thick materials).
    pub z_step: f64,
}

impl Default for LineParams {
    fn default() -> Self {
        Self {
            speed: 600.0,
            power: 80.0,
            passes: 1,
            air_assist: true,
            z_step: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FillParams {
    pub speed: f64,
    pub power: f64,
    pub passes: u32,
    /// Distance between scan lines in mm.
    pub interval: f64,
    pub angle: f64,
    pub bidirectional: bool,
    pub crosshatch: bool,
    /// Extra travel with the laser off at both ends of a line, in mm,
    /// so that the head reaches full speed before burning.
    pub overscan: f64,
    pub air_assist: bool,
}

impl Default for FillParams {
    fn default() -> Self {
        Self {
            speed: 6000.0,
            power: 30.0,
            passes: 1,
            interval: 0.1,
            angle: 0.0,
            bidirectional: true,
            crosshatch: false,
            overscan: 2.5,
            air_assist: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DitherMode {
    /// Power follows the grey level (needs a laser able to modulate).
    Grayscale,
    Threshold,
    Ordered,
    FloydSteinberg,
    Atkinson,
    Jarvis,
    Stucki,
}

impl DitherMode {
    pub const ALL: [DitherMode; 7] = [
        Self::Grayscale,
        Self::Threshold,
        Self::Ordered,
        Self::FloydSteinberg,
        Self::Atkinson,
        Self::Jarvis,
        Self::Stucki,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Grayscale => "Grayscale",
            Self::Threshold => "Threshold",
            Self::Ordered => "Ordered (Bayer)",
            Self::FloydSteinberg => "Floyd–Steinberg",
            Self::Atkinson => "Atkinson",
            Self::Jarvis => "Jarvis",
            Self::Stucki => "Stucki",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageParams {
    pub speed: f64,
    pub power_min: f64,
    pub power_max: f64,
    pub passes: u32,
    /// Lines per millimetre is `dpi / 25.4`.
    pub dpi: f64,
    pub dither: DitherMode,
    pub threshold: u8,
    pub negative: bool,
    /// -100..100
    pub brightness: f64,
    /// -100..100
    pub contrast: f64,
    pub bidirectional: bool,
    pub overscan: f64,
    pub air_assist: bool,
}

impl Default for ImageParams {
    fn default() -> Self {
        Self {
            speed: 6000.0,
            power_min: 0.0,
            power_max: 65.0,
            passes: 1,
            dpi: 254.0,
            dither: DitherMode::FloydSteinberg,
            threshold: 128,
            negative: false,
            brightness: 0.0,
            contrast: 0.0,
            bidirectional: true,
            overscan: 2.5,
            air_assist: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileSide {
    Outside,
    Inside,
    On,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CncProfileParams {
    pub side: ProfileSide,
    pub tool_diameter: f64,
    /// Final depth below the stock top, positive value in mm.
    pub depth: f64,
    pub pass_depth: f64,
    pub feed: f64,
    pub plunge: f64,
    pub spindle: f64,
    pub climb: bool,
    pub tabs: u32,
    pub tab_width: f64,
    pub tab_height: f64,
}

impl Default for CncProfileParams {
    fn default() -> Self {
        Self {
            side: ProfileSide::Outside,
            tool_diameter: 3.175,
            depth: 3.0,
            pass_depth: 0.5,
            feed: 600.0,
            plunge: 150.0,
            spindle: 10000.0,
            climb: false,
            tabs: 0,
            tab_width: 4.0,
            tab_height: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CncPocketParams {
    pub tool_diameter: f64,
    pub depth: f64,
    pub pass_depth: f64,
    /// Fraction of the tool diameter between two rings (0.1..0.9).
    pub stepover: f64,
    pub feed: f64,
    pub plunge: f64,
    pub spindle: f64,
    pub climb: bool,
}

impl Default for CncPocketParams {
    fn default() -> Self {
        Self {
            tool_diameter: 3.175,
            depth: 1.0,
            pass_depth: 0.5,
            stepover: 0.4,
            feed: 600.0,
            plunge: 150.0,
            spindle: 10000.0,
            climb: false,
        }
    }
}
