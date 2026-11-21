//! Tool and material libraries of other programs, found where those
//! programs keep them, so they can be imported (and kept up to date)
//! without hunting for files: Autodesk Fusion tool libraries, LightBurn
//! material libraries, MaxmakeLAB's material library.

use crate::{Preset, Tool};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// Fusion tool library (`.json`, `data` array).
    FusionTools,
    /// LightBurn material library (`.clb`).
    LightBurnMaterials,
    /// MaxmakeLAB material library (LightBurn format).
    MaxmakeLabMaterials,
}

impl SourceKind {
    pub fn software(self) -> &'static str {
        match self {
            Self::FusionTools => "Autodesk Fusion",
            Self::LightBurnMaterials => "LightBurn",
            Self::MaxmakeLabMaterials => "MaxmakeLAB",
        }
    }

    /// True for tools, false for material settings.
    pub fn is_tools(self) -> bool {
        matches!(self, Self::FusionTools)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub kind: SourceKind,
    pub path: PathBuf,
}

impl Source {
    /// File name without extension, as the program names the library.
    pub fn name(&self) -> String {
        self.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

pub enum Imported {
    Tools(Vec<Tool>),
    Presets(Vec<Preset>),
}

impl Imported {
    pub fn len(&self) -> usize {
        match self {
            Self::Tools(tools) => tools.len(),
            Self::Presets(presets) => presets.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Folders searched, from the environment of the computer.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub home: Option<PathBuf>,
    /// Windows `%APPDATA%` (Roaming).
    pub appdata: Option<PathBuf>,
    /// Where MaxmakeLAB is installed on macOS.
    pub applications: Option<PathBuf>,
}

impl Roots {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        Self {
            home,
            appdata: std::env::var_os("APPDATA").map(PathBuf::from),
            applications: Some(PathBuf::from("/Applications")),
        }
    }
}

fn files_with(folder: &Path, extension: &str, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            if depth > 0 {
                found.extend(files_with(&path, extension, depth - 1));
            }
        } else if path
            .extension()
            .and_then(|found| found.to_str())
            .is_some_and(|found| found.eq_ignore_ascii_case(extension))
        {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Every library found, Fusion first, then LightBurn, then MaxmakeLAB.
pub fn discover(roots: &Roots) -> Vec<Source> {
    let mut sources = Vec::new();
    let mut add = |kind, paths: Vec<PathBuf>| {
        for path in paths {
            if !sources.iter().any(|source: &Source| source.path == path) {
                sources.push(Source { kind, path });
            }
        }
    };
    // Fusion keeps local tool libraries under CAM360/libraries.
    let mut fusion = Vec::new();
    if let Some(home) = &roots.home {
        fusion.push(home.join("Library/Application Support/Autodesk/CAM360/libraries"));
    }
    if let Some(appdata) = &roots.appdata {
        fusion.push(appdata.join("Autodesk/CAM360/libraries"));
    }
    for folder in fusion {
        add(SourceKind::FusionTools, files_with(&folder, "json", 2));
    }
    // LightBurn libraries are saved where the user chooses: the usual
    // places, without going deep into them.
    if let Some(home) = &roots.home {
        for (folder, depth) in [
            ("Documents/LightBurn", 3),
            ("Documents", 1),
            ("Desktop", 1),
            ("Downloads", 1),
        ] {
            add(
                SourceKind::LightBurnMaterials,
                files_with(&home.join(folder), "clb", depth),
            );
        }
    }
    // MaxmakeLAB ships one library per language: the English one.
    let mut maxmake = Vec::new();
    if let Some(home) = &roots.home {
        maxmake.push(home.join("Library/Application Support/Marxmake/MaxmakeLAB/data/files"));
    }
    if let Some(appdata) = &roots.appdata {
        maxmake.push(appdata.join("Marxmake/MaxmakeLAB/data/files"));
    }
    if let Some(applications) = &roots.applications {
        maxmake.push(applications.join("MaxmakeLAB.app/Contents/Resources/data/files"));
    }
    let english = maxmake
        .iter()
        .flat_map(|folder| files_with(folder, "clb", 0))
        .find(|path| path.to_string_lossy().contains("en_US"));
    add(
        SourceKind::MaxmakeLabMaterials,
        english.into_iter().collect(),
    );
    sources
}

/// Reads a library. Fusion files that are not tool libraries (other
/// JSON in the folder) come back empty.
pub fn read(source: &Source) -> Result<Imported, String> {
    let text = std::fs::read_to_string(&source.path).map_err(|error| error.to_string())?;
    match source.kind {
        SourceKind::FusionTools => crate::import_fusion(&text)
            .map(Imported::Tools)
            .map_err(|error| error.to_string()),
        SourceKind::LightBurnMaterials | SourceKind::MaxmakeLabMaterials => {
            crate::import_lightburn(&text)
                .map(Imported::Presets)
                .map_err(|error| error.to_string())
        }
    }
}

/// Modification time, seconds since the epoch, to notice a library the
/// other program changed.
pub fn modified(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FUSION: &str = r#"{"data": [
        {"type": "flat end mill", "unit": "millimeters", "description": "Flat 3",
         "geometry": {"DC": 3.0, "NOF": 2}},
        {"type": "holder", "description": "ER11"}
    ]}"#;

    const LIGHTBURN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<LightBurnLibrary>
    <Material name="Plywood">
        <Entry Thickness="3" Desc="Cut" NoThickTitle="">
            <CutSetting type="Cut">
                <index Value="0"/>
                <name Value="C00"/>
                <maxPower Value="80"/>
                <speed Value="10"/>
            </CutSetting>
        </Entry>
    </Material>
</LightBurnLibrary>"#;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn libraries_are_found_in_the_usual_folders_and_read() {
        let home = std::env::temp_dir().join(format!("mml-sources-{}", uuid::Uuid::new_v4()));
        let fusion = home.join("Library/Application Support/Autodesk/CAM360/libraries/Local");
        write(&fusion.join("Shop.json"), FUSION);
        write(
            &home.join("Documents/LightBurn/Libraries/Wood.clb"),
            LIGHTBURN,
        );
        write(&home.join("Desktop/Acrylic.clb"), LIGHTBURN);
        // Too deep in Documents: not searched.
        write(&home.join("Documents/a/b/Hidden.clb"), LIGHTBURN);
        let roots = Roots {
            home: Some(home.clone()),
            appdata: None,
            applications: None,
        };
        let sources = discover(&roots);
        let names: Vec<(SourceKind, String)> = sources
            .iter()
            .map(|source| (source.kind, source.name()))
            .collect();
        assert_eq!(
            names,
            [
                (SourceKind::FusionTools, "Shop".to_owned()),
                (SourceKind::LightBurnMaterials, "Wood".to_owned()),
                (SourceKind::LightBurnMaterials, "Acrylic".to_owned()),
            ]
        );
        let Ok(Imported::Tools(tools)) = read(&sources[0]) else {
            panic!("tools");
        };
        assert_eq!(tools.len(), 1, "the holder is skipped");
        let Ok(Imported::Presets(presets)) = read(&sources[1]) else {
            panic!("presets");
        };
        assert!(!presets.is_empty());
        assert!(modified(&sources[1].path).is_some());
        std::fs::remove_dir_all(home).ok();
    }
}
