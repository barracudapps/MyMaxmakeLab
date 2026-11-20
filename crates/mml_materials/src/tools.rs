//! Cutting tools: end mills, ball mills, V-bits and drills, with their
//! cutting data, created by hand or imported from Fusion 360.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolKind {
    FlatEndMill,
    BallEndMill,
    /// V-bit or engraving bit; `angle` is the included angle in degrees.
    VBit {
        angle: f64,
    },
    Drill,
}

impl ToolKind {
    pub fn key(&self) -> &'static str {
        match self {
            Self::FlatEndMill => "tool.flat",
            Self::BallEndMill => "tool.ball",
            Self::VBit { .. } => "tool.vbit",
            Self::Drill => "tool.drill",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tool {
    pub id: Uuid,
    pub name: String,
    pub kind: ToolKind,
    /// Cutting diameter, mm (for a V-bit: the largest diameter).
    pub diameter: f64,
    pub flutes: u32,
    /// mm/min
    pub feed: f64,
    pub plunge: f64,
    /// rpm
    pub spindle: f64,
    /// Depth per pass, mm.
    pub stepdown: f64,
    /// Fraction of the diameter between two passes.
    pub stepover: f64,
    pub notes: String,
}

impl Default for Tool {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            kind: ToolKind::FlatEndMill,
            diameter: 3.175,
            flutes: 2,
            feed: 800.0,
            plunge: 200.0,
            spindle: 12000.0,
            stepdown: 0.5,
            stepover: 0.4,
            notes: String::new(),
        }
    }
}

impl Tool {
    pub fn label(&self) -> String {
        match self.kind {
            ToolKind::VBit { angle } => {
                format!("{} (V {angle:.0}°, Ø{:.2})", self.name, self.diameter)
            }
            _ => format!("{} (Ø{:.3})", self.name, self.diameter),
        }
    }
}

fn number(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(serde_json::Value::as_f64)
}

/// Reads a Fusion 360 tool library export (`.json`, `data` array). Units
/// are converted to millimetres; the first preset gives the cutting data.
pub fn import_fusion(json: &str) -> Result<Vec<Tool>, serde_json::Error> {
    let root: serde_json::Value = serde_json::from_str(json)?;
    let entries = root
        .get("data")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut tools = Vec::new();
    for entry in entries {
        let kind_text = entry
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        if kind_text.contains("holder") || kind_text.contains("probe") {
            continue;
        }
        let scale = match entry.get("unit").and_then(serde_json::Value::as_str) {
            Some(unit) if unit.starts_with("inch") => 25.4,
            _ => 1.0,
        };
        let geometry = entry.get("geometry").cloned().unwrap_or_default();
        let diameter = number(&geometry, "DC").unwrap_or(3.175) * scale;
        let angle = number(&geometry, "SIG")
            .or_else(|| number(&geometry, "TA").map(|taper| taper * 2.0))
            .unwrap_or(90.0);
        let kind = if kind_text.contains("ball") {
            ToolKind::BallEndMill
        } else if kind_text.contains("chamfer")
            || kind_text.contains("engrave")
            || kind_text.contains("spot")
            || kind_text.contains("v-bit")
            || kind_text.contains("tapered")
        {
            ToolKind::VBit { angle }
        } else if kind_text.contains("drill") {
            ToolKind::Drill
        } else {
            ToolKind::FlatEndMill
        };
        let preset = entry
            .get("start-values")
            .and_then(|values| values.get("presets"))
            .and_then(serde_json::Value::as_array)
            .and_then(|presets| presets.first())
            .cloned()
            .unwrap_or_default();
        let defaults = Tool::default();
        let name = entry
            .get("description")
            .and_then(serde_json::Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{kind_text} {diameter:.3}"));
        tools.push(Tool {
            name,
            kind,
            diameter,
            flutes: number(&geometry, "NOF").map_or(defaults.flutes, |flutes| flutes as u32),
            feed: number(&preset, "v_f").map_or(defaults.feed, |feed| feed * scale),
            plunge: number(&preset, "v_f_plunge").map_or(defaults.plunge, |feed| feed * scale),
            spindle: number(&preset, "n").unwrap_or(defaults.spindle),
            stepdown: number(&preset, "stepdown").map_or(defaults.stepdown, |depth| depth * scale),
            stepover: number(&preset, "stepover")
                .map(|stepover| (stepover * scale / diameter.max(0.01)).clamp(0.05, 0.95))
                .unwrap_or(defaults.stepover),
            notes: entry
                .get("vendor")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned(),
            ..defaults
        });
    }
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_library_is_imported_in_millimetres() {
        let json = r#"{"data":[
            {"type":"flat end mill","unit":"millimeters","description":"2F 3.175",
             "geometry":{"DC":3.175,"NOF":2},
             "start-values":{"presets":[{"n":18000,"v_f":900,"v_f_plunge":300,"stepdown":1.0,"stepover":1.27}]}},
            {"type":"chamfer mill","unit":"inches","description":"V 60",
             "geometry":{"DC":0.25,"SIG":60}},
            {"type":"holder","description":"ER11"}
        ]}"#;
        let tools = import_fusion(json).unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].kind, ToolKind::FlatEndMill);
        assert_eq!((tools[0].feed, tools[0].spindle), (900.0, 18000.0));
        assert!((tools[0].stepover - 0.4).abs() < 1e-3);
        assert_eq!(tools[1].kind, ToolKind::VBit { angle: 60.0 });
        assert!((tools[1].diameter - 6.35).abs() < 1e-9);
    }
}
