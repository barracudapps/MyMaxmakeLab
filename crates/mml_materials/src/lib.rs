//! Material presets: a named set of layer parameters for a material and
//! thickness, applied to a layer in one click.

pub mod paper;
pub mod tools;

use mml_core::layer::{DitherMode, FillParams, ImageParams, LineParams, Operation};
use serde::{Deserialize, Serialize};
use std::path::Path;
pub use tools::{Tool, ToolKind, import_fusion};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("cannot access the library: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid library: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid LightBurn library: {0}")]
    Xml(#[from] roxmltree::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub id: Uuid,
    pub material: String,
    /// mm; `None` when the setting does not depend on thickness.
    pub thickness: Option<f64>,
    pub title: String,
    pub operation: Operation,
    #[serde(default)]
    pub notes: String,
}

impl Preset {
    pub fn new(
        material: impl Into<String>,
        thickness: Option<f64>,
        title: impl Into<String>,
        operation: Operation,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            material: material.into(),
            thickness,
            title: title.into(),
            operation,
            notes: String::new(),
        }
    }

    pub fn label(&self) -> String {
        match self.thickness {
            Some(thickness) => format!("{} {thickness} mm - {}", self.material, self.title),
            None => format!("{} - {}", self.material, self.title),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub tools: Vec<Tool>,
}

impl Library {
    pub fn load(path: &Path) -> Result<Self, LibraryError> {
        match std::fs::read(path) {
            Ok(data) => Ok(serde_json::from_slice(&data)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), LibraryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn materials(&self) -> Vec<&str> {
        let mut materials: Vec<&str> = self
            .presets
            .iter()
            .map(|preset| preset.material.as_str())
            .collect();
        materials.sort_unstable_by_key(|name| name.to_lowercase());
        materials.dedup();
        materials
    }

    /// Adds presets, skipping exact duplicates; returns how many were new.
    pub fn merge(&mut self, presets: Vec<Preset>) -> usize {
        let mut added = 0;
        for preset in presets {
            let duplicate = self.presets.iter().any(|existing| {
                existing.material == preset.material
                    && existing.thickness == preset.thickness
                    && existing.title == preset.title
                    && existing.operation == preset.operation
            });
            if !duplicate {
                self.presets.push(preset);
                added += 1;
            }
        }
        added
    }
}

/// Where MaxmakeLAB keeps its material library, to offer a migration.
pub fn maxmakelab_library_candidates() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(
            Path::new(&home).join("Library/Application Support/Marxmake/MaxmakeLAB/data/files"),
        );
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(Path::new(&appdata).join("Marxmake/MaxmakeLAB/data/files"));
    }
    roots.push("/Applications/MaxmakeLAB.app/Contents/Resources/data/files".into());
    roots
        .into_iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "clb")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.is_ascii())
        })
        .collect()
}

/// "BlackAcrylic" -> "Black Acrylic".
fn humanize(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut previous_lower = false;
    for character in name.chars() {
        if character.is_uppercase() && previous_lower {
            out.push(' ');
        }
        previous_lower = character.is_lowercase() || character.is_ascii_digit();
        out.push(character);
    }
    out
}

/// MaxmakeLAB ships its English library with Chinese descriptions.
fn translate_description(description: &str) -> Option<&'static str> {
    Some(match description {
        "切割" => "Cut",
        "矢量雕刻" | "线条雕刻" => "Line engrave",
        "矢量区域清除" | "填充雕刻" => "Fill engrave",
        "图片雕刻" => "Image engrave",
        "画笔" => "Pen",
        _ => return None,
    })
}

fn dither_from(name: &str) -> DitherMode {
    match name.to_ascii_lowercase().as_str() {
        "threshold" => DitherMode::Threshold,
        "ordered" => DitherMode::Ordered,
        "newsprint" | "halftone" => DitherMode::Halftone,
        "atkinson" => DitherMode::Atkinson,
        "jarvis" => DitherMode::Jarvis,
        "stucki" => DitherMode::Stucki,
        "grayscale" | "greyscale" | "sketch" => DitherMode::Grayscale,
        _ => DitherMode::FloydSteinberg,
    }
}

/// Reads a LightBurn `.clb` library. Speeds are stored in mm/s there.
pub fn import_lightburn(xml: &str) -> Result<Vec<Preset>, LibraryError> {
    let document = roxmltree::Document::parse(xml)?;
    let mut presets = Vec::new();
    for material in document
        .descendants()
        .filter(|node| node.has_tag_name("Material"))
    {
        let material_name = humanize(material.attribute("name").unwrap_or("Material"));
        for entry in material
            .children()
            .filter(|node| node.has_tag_name("Entry"))
        {
            let thickness = entry
                .attribute("Thickness")
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| *value > 0.0);
            for setting in entry
                .children()
                .filter(|node| node.has_tag_name("CutSetting"))
            {
                let value = |tag: &str| {
                    setting
                        .children()
                        .find(|node| node.has_tag_name(tag))
                        .and_then(|node| node.attribute("Value"))
                };
                let number = |tag: &str, default: f64| {
                    value(tag).and_then(|v| v.parse().ok()).unwrap_or(default)
                };
                let speed = number("speed", 10.0) * 60.0;
                let power = number("maxPower", 20.0);
                let passes = number("numPasses", 1.0).max(1.0) as u32;
                let kind = setting.attribute("type").unwrap_or("Cut");
                let operation = match kind {
                    "Scan" => Operation::LaserFill(FillParams {
                        speed,
                        power,
                        passes,
                        interval: number("interval", 0.1),
                        overscan: number("overscan", FillParams::default().overscan),
                        ..FillParams::default()
                    }),
                    "Image" => Operation::LaserImage(ImageParams {
                        speed,
                        power_min: number("minPower", 0.0),
                        power_max: power,
                        passes,
                        dither: dither_from(value("ditherMode").unwrap_or("")),
                        ..ImageParams::default()
                    }),
                    _ => Operation::LaserLine(LineParams {
                        speed,
                        power,
                        passes,
                        z_step: number("zPerPass", 0.0),
                        ..LineParams::default()
                    }),
                };
                let description = value("LinkPath")
                    .and_then(|path| path.rsplit('/').next())
                    .or(entry.attribute("Desc"))
                    .unwrap_or_default();
                let title = translate_description(description)
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        if description.is_empty() {
                            kind.to_owned()
                        } else {
                            description.to_owned()
                        }
                    });
                presets.push(Preset::new(
                    material_name.clone(),
                    thickness,
                    title,
                    operation,
                ));
            }
        }
    }
    Ok(presets)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<LightBurnLibrary>
    <Material name="BlackAcrylic">
        <Entry Thickness="3.0000" Desc="切割">
            <CutSetting type="Cut">
                <LinkPath Value="亚克力/3.0000/切割"/>
                <maxPower Value="100"/>
                <speed Value="12"/>
            </CutSetting>
        </Entry>
        <Entry Thickness="-1.0000" Desc="图片雕刻">
            <CutSetting type="Image">
                <LinkPath Value="亚克力/-1.0000/图片雕刻"/>
                <minPower Value="0"/>
                <maxPower Value="10"/>
                <speed Value="300"/>
                <ditherMode Value="jarvis"/>
            </CutSetting>
        </Entry>
    </Material>
</LightBurnLibrary>"#;

    #[test]
    fn maxmakelab_library_is_imported_in_english_with_mm_per_minute() {
        let presets = import_lightburn(SAMPLE).unwrap();
        assert_eq!(presets[0].label(), "Black Acrylic 3 mm - Cut");
        assert_eq!(
            presets[0].operation,
            Operation::LaserLine(LineParams {
                speed: 720.0,
                power: 100.0,
                ..LineParams::default()
            })
        );
        assert_eq!(presets[1].thickness, None);
        match &presets[1].operation {
            Operation::LaserImage(params) => assert_eq!(params.dither, DitherMode::Jarvis),
            other => panic!("unexpected {other:?}"),
        }
        let mut library = Library::default();
        assert_eq!(library.merge(presets.clone()), 2);
        assert_eq!(library.merge(presets), 0);
    }
}
