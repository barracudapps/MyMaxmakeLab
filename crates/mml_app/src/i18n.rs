//! Interface translations. English is the reference; any missing key in
//! another language falls back to English, then to the key itself.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    French,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::French];

    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::French => "Français",
        }
    }

    pub fn system() -> Self {
        match sys_locale::get_locale() {
            Some(locale) if locale.to_lowercase().starts_with("fr") => Self::French,
            _ => Self::English,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

type Table = HashMap<&'static str, &'static str>;

fn parse(source: &'static str) -> Table {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect()
}

fn tables() -> &'static [Table; 2] {
    static TABLES: OnceLock<[Table; 2]> = OnceLock::new();
    TABLES.get_or_init(|| {
        [
            parse(include_str!("../assets/i18n/en.txt")),
            parse(include_str!("../assets/i18n/fr.txt")),
        ]
    })
}

pub fn set_language(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}

pub fn tr(key: &'static str) -> &'static str {
    let tables = tables();
    let index = CURRENT.load(Ordering::Relaxed) as usize;
    tables[index]
        .get(key)
        .or_else(|| tables[0].get(key))
        .copied()
        .unwrap_or(key)
}

/// `tr` with `{0}`, `{1}`... placeholders.
pub fn trf(key: &'static str, arguments: &[&dyn std::fmt::Display]) -> String {
    let mut text = tr(key).to_owned();
    for (index, argument) in arguments.iter().enumerate() {
        text = text.replace(&format!("{{{index}}}"), &argument.to_string());
    }
    text
}

/// Translated GRBL message with the English text of the crate as
/// fallback.
pub fn grbl_error(code: u32) -> String {
    translated_code("grbl.error.", code, mml_device::error_message(code))
}

pub fn grbl_alarm(code: u32) -> String {
    translated_code("grbl.alarm.", code, mml_device::alarm_message(code))
}

fn translated_code(prefix: &str, code: u32, fallback: &str) -> String {
    let key = format!("{prefix}{code}");
    let tables = tables();
    let index = CURRENT.load(Ordering::Relaxed) as usize;
    tables[index]
        .get(key.as_str())
        .copied()
        .unwrap_or(fallback)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_french_key_exists_in_english() {
        let [english, french] = tables();
        let missing: Vec<_> = french
            .keys()
            .filter(|key| !english.contains_key(*key) && !key.starts_with("grbl."))
            .collect();
        assert!(missing.is_empty(), "keys missing in en.txt: {missing:?}");
        let untranslated: Vec<_> = english
            .keys()
            .filter(|key| !french.contains_key(*key))
            .collect();
        assert!(
            untranslated.is_empty(),
            "keys missing in fr.txt: {untranslated:?}"
        );
    }

    #[test]
    fn every_key_used_in_the_code_is_translated() {
        let sources = [
            include_str!("app.rs"),
            include_str!("canvas.rs"),
            include_str!("machine_panel.rs"),
            include_str!("panels.rs"),
            include_str!("workflow.rs"),
            include_str!("relief_dialog.rs"),
            include_str!("assist.rs"),
            include_str!("theme.rs"),
            include_str!("../../mml_core/src/layer.rs"),
        ];
        let english = &tables()[0];
        let mut missing = Vec::new();
        for source in sources {
            for (index, _) in source.match_indices('"') {
                let rest = &source[index + 1..];
                let Some(end) = rest.find('"') else { continue };
                let candidate = &rest[..end];
                let prefix = candidate.split('.').next().unwrap_or("");
                let known_prefix = [
                    "mode",
                    "menu",
                    "theme",
                    "tool",
                    "view",
                    "shape",
                    "layer",
                    "file",
                    "status",
                    "props",
                    "align",
                    "layers",
                    "op",
                    "materials",
                    "job",
                    "warn",
                    "device",
                    "state",
                    "jog",
                    "console",
                    "macros",
                    "machine",
                    "origin",
                    "text",
                    "array",
                    "prefs",
                    "dialog",
                    "close",
                    "about",
                ]
                .contains(&prefix);
                let looks_like_key = candidate.contains('.')
                    && candidate.chars().all(|c| {
                        c.is_ascii_lowercase() || c == '_' || c == '.' || c.is_ascii_digit()
                    });
                if known_prefix
                    && looks_like_key
                    && !candidate.ends_with(".json")
                    && !english.contains_key(candidate)
                {
                    missing.push(candidate.to_owned());
                }
            }
        }
        assert!(missing.is_empty(), "untranslated keys: {missing:?}");
    }
}
