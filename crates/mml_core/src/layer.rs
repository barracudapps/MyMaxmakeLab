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
    /// Pause before this layer so the operator can change the tool or
    /// head; the job resumes from the machine button or the interface.
    #[serde(default)]
    pub pause_before: bool,
    /// Tool changer slot loaded before this layer (`M6 T…`) on machines
    /// that have one; ignored otherwise.
    #[serde(default)]
    pub tool: Option<u32>,
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
            pause_before: false,
            tool: None,
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
    /// Engraving with a V-bit: V-carving of vectors (depth follows the
    /// stroke width) or of pictures (depth follows darkness).
    CncVCarve(VCarveParams),
}

impl Operation {
    /// Name of the tool doing this operation in job records.
    pub fn tool_label(&self) -> String {
        match self {
            Operation::CncProfile(params) => {
                crate::stats::tool_label("End mill", params.tool_diameter, None)
            }
            Operation::CncPocket(params) => {
                crate::stats::tool_label("End mill", params.tool_diameter, None)
            }
            Operation::CncVCarve(params) => {
                crate::stats::tool_label("V-bit", params.tool_diameter, Some(params.angle))
            }
            _ => "Laser".into(),
        }
    }

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
            Self::CncVCarve(_) => "op.cnc_vcarve",
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
    /// Dashed or dotted line: burn `on` mm, skip `off` mm. Zero `off`
    /// means a solid line. Also used for perforations (fold lines).
    pub dash_on: f64,
    pub dash_off: f64,
    /// Uncut bridges keeping each part in the sheet, mm wide; 0 cuts
    /// the parts free.
    #[serde(default)]
    pub bridge_width: f64,
    /// Bridges per part; 0 places them automatically.
    #[serde(default)]
    pub bridge_count: u32,
}

impl Default for LineParams {
    fn default() -> Self {
        Self {
            speed: 600.0,
            power: 80.0,
            passes: 1,
            air_assist: true,
            z_step: 0.0,
            dash_on: 2.0,
            dash_off: 0.0,
            bridge_width: 0.0,
            bridge_count: 0,
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
    pub pattern: FillPattern,
}

/// How a filled area is swept by the laser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FillPattern {
    /// Parallel lines at `angle` (0 horizontal, 45 diagonal...).
    #[default]
    Lines,
    /// Circles around the centre of the design, `interval` apart.
    Circles,
    /// Rings following the outline inwards, like contour lines.
    Contours,
}

impl FillPattern {
    pub const ALL: [FillPattern; 3] = [Self::Lines, Self::Circles, Self::Contours];

    pub fn key(self) -> &'static str {
        match self {
            Self::Lines => "op.pattern_lines",
            Self::Circles => "op.pattern_circles",
            Self::Contours => "op.pattern_contours",
        }
    }
}

/// Path the laser follows over a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ImagePattern {
    #[default]
    Lines,
    /// Lines at 45°: hides the banding of some machines and gives a
    /// different texture on wood grain.
    Diagonal,
    /// Circles around the centre of the picture (spiral-art look).
    Circles,
}

impl ImagePattern {
    pub const ALL: [ImagePattern; 3] = [Self::Lines, Self::Diagonal, Self::Circles];

    pub fn key(self) -> &'static str {
        match self {
            Self::Lines => "op.pattern_lines",
            Self::Diagonal => "op.pattern_diagonal",
            Self::Circles => "op.pattern_circles",
        }
    }
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
            pattern: FillPattern::Lines,
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
    /// Clustered dots on a 45° screen, the look of old newspapers.
    Halftone,
    /// Horizontal lines thickening in dark areas, like a copper-plate
    /// engraving.
    LineScreen,
}

impl DitherMode {
    pub const ALL: [DitherMode; 9] = [
        Self::Grayscale,
        Self::Threshold,
        Self::Ordered,
        Self::FloydSteinberg,
        Self::Atkinson,
        Self::Jarvis,
        Self::Stucki,
        Self::Halftone,
        Self::LineScreen,
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
            Self::Halftone => "Halftone (vintage)",
            Self::LineScreen => "Line engraving",
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
    pub pattern: ImagePattern,
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
            pattern: ImagePattern::Lines,
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
pub struct VCarveParams {
    /// Included angle of the bit, degrees.
    pub angle: f64,
    /// Widest cutting diameter of the bit, mm.
    pub tool_diameter: f64,
    /// Depth limit; wide areas are flat-bottomed at this depth.
    pub max_depth: f64,
    /// Distance between raster lines, mm.
    pub line_spacing: f64,
    pub feed: f64,
    pub plunge: f64,
    pub spindle: f64,
}

impl Default for VCarveParams {
    fn default() -> Self {
        Self {
            angle: 60.0,
            tool_diameter: 6.0,
            max_depth: 1.5,
            line_spacing: 0.15,
            feed: 800.0,
            plunge: 200.0,
            spindle: 16000.0,
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
