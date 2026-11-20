use crate::i18n::Language;
use mml_core::machine::MachineProfile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedConnection {
    pub use_tcp: bool,
    pub serial_port: String,
    pub baud: u32,
    pub host: String,
    pub tcp_port: u16,
}

impl Default for SavedConnection {
    fn default() -> Self {
        Self {
            use_tcp: false,
            serial_port: String::new(),
            baud: 115_200,
            host: "192.168.4.1".into(),
            tcp_port: 23,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub gcode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub theme: ThemeChoice,
    /// Laser speeds are shown in mm/s (as on most laser software) when
    /// true, otherwise in mm/min like CNC feeds.
    pub laser_speed_per_second: bool,
    pub machine: MachineProfile,
    pub connection: SavedConnection,
    pub recent: Vec<PathBuf>,
    pub macros: Vec<Macro>,
    pub snap: bool,
    pub grid: f64,
    pub show_travel: bool,
    pub jog_step: f64,
    pub jog_feed: f64,
    pub frame_power: f64,
    pub library_import_offered: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::system(),
            theme: ThemeChoice::System,
            laser_speed_per_second: true,
            machine: MachineProfile::generic_laser(),
            connection: SavedConnection::default(),
            recent: Vec::new(),
            macros: vec![
                Macro {
                    name: "Home".into(),
                    gcode: "$H".into(),
                },
                Macro {
                    name: "Laser off".into(),
                    gcode: "M5".into(),
                },
            ],
            snap: true,
            grid: 1.0,
            show_travel: false,
            jog_step: 10.0,
            jog_feed: 3000.0,
            frame_power: 0.0,
            library_import_offered: false,
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "MyMaxmakeLab")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn library_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("materials.json"))
}

impl Settings {
    pub fn load() -> Self {
        config_dir()
            .and_then(|dir| std::fs::read(dir.join("settings.json")).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(dir) = config_dir() else { return };
        if std::fs::create_dir_all(&dir).is_ok()
            && let Ok(data) = serde_json::to_vec_pretty(self)
        {
            let _ = std::fs::write(dir.join("settings.json"), data);
        }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|existing| existing != &path);
        self.recent.insert(0, path);
        self.recent.truncate(8);
    }
}
