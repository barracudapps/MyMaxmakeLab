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
    pub protocol: mml_device::Protocol,
}

impl Default for SavedConnection {
    fn default() -> Self {
        Self {
            use_tcp: false,
            serial_port: String::new(),
            baud: 115_200,
            host: "192.168.4.1".into(),
            tcp_port: 23,
            protocol: mml_device::Protocol::Grbl,
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
    /// Feed hold automatically on a critical anomaly.
    pub auto_pause: bool,
    /// Recent successful homing durations in seconds, newest last.
    pub homing_times: Vec<f64>,
    /// Advanced mode shows the console and the controller settings.
    pub advanced: bool,
    /// Mandatory checks before a job; can only be lifted in advanced
    /// mode. The live machine monitor stays active either way.
    pub barriers: bool,
    pub require_probe_cnc: bool,
    pub require_frame_laser: bool,
    pub probe: mml_probe::ProbeSettings,
    /// The machine has a 3D probe; without one, origins are recorded
    /// by hand.
    pub has_probe: bool,
    pub manual_contact: mml_probe::manual::Contact,
    /// Jog speed follows the step size (fine steps move slowly).
    pub jog_proportional: bool,
    pub idle_rpm: f64,
    pub tutorial_done: bool,
    /// Prices and lifetimes for job costs.
    pub costs: mml_core::stats::CostSettings,
    /// Install new production releases from GitHub by itself.
    pub auto_update: bool,
    /// Bed corners clicked on the camera picture (front-left, front-right,
    /// back-right, back-left), kept while the camera does not move.
    pub camera_corners: Option<[[f64; 2]; 4]>,
    /// Snapshot address of a phone or IP camera.
    pub camera_url: String,
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
            auto_pause: true,
            homing_times: Vec::new(),
            advanced: false,
            barriers: true,
            require_probe_cnc: true,
            require_frame_laser: true,
            probe: mml_probe::ProbeSettings::default(),
            has_probe: false,
            manual_contact: Default::default(),
            jog_proportional: true,
            idle_rpm: 8000.0,
            tutorial_done: false,
            costs: Default::default(),
            auto_update: true,
            camera_corners: None,
            camera_url: String::new(),
        }
    }
}

/// `MYMAXMAKELAB_CONFIG_DIR` overrides the location, for portable
/// installs on a USB stick and for isolated test sessions.
fn config_dir() -> Option<PathBuf> {
    if let Some(directory) = std::env::var_os("MYMAXMAKELAB_CONFIG_DIR") {
        return Some(PathBuf::from(directory));
    }
    directories::ProjectDirs::from("", "", "MyMaxmakeLab")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

/// Fonts added by the user, loaded next to the system and bundled ones.
pub fn fonts_dir() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("fonts"))
}

/// Every job that ran, one JSON record per line (appended, never
/// rewritten, so a crash cannot lose the history).
fn usage_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("usage.jsonl"))
}

pub fn load_usage() -> Vec<mml_core::stats::JobRecord> {
    usage_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn append_usage(record: &mml_core::stats::JobRecord) {
    use std::io::Write;
    let Some(path) = usage_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let (Ok(mut file), Ok(line)) = (
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path),
        serde_json::to_string(record),
    ) {
        let _ = writeln!(file, "{line}");
    }
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
