//! Texts of the sample projects in the five languages of the
//! application, `key = value` per line like its own translations. A
//! value `@name` stands for the shared sentence `common.name`.

use std::collections::HashMap;
use std::sync::OnceLock;

const FILES: [(&str, &str); 5] = [
    ("fr", include_str!("../text/fr.txt")),
    ("en", include_str!("../text/en.txt")),
    ("de", include_str!("../text/de.txt")),
    ("it", include_str!("../text/it.txt")),
    ("es", include_str!("../text/es.txt")),
];

type Table = HashMap<String, String>;

pub(crate) fn parse(source: &str) -> Table {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once(" = "))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
}

fn tables() -> &'static HashMap<&'static str, Table> {
    static TABLES: OnceLock<HashMap<&'static str, Table>> = OnceLock::new();
    TABLES.get_or_init(|| {
        FILES
            .iter()
            .map(|(language, source)| (*language, parse(source)))
            .collect()
    })
}

/// The language code among ours that `language` (like "fr-BE") starts
/// with, English otherwise.
pub fn supported(language: &str) -> &'static str {
    FILES
        .iter()
        .map(|(code, _)| *code)
        .find(|code| language.to_lowercase().starts_with(code))
        .unwrap_or("en")
}

fn raw(language: &str, key: &str) -> Option<&'static str> {
    let tables = tables();
    [supported(language), "en", "fr"]
        .iter()
        .find_map(|code| tables.get(code).and_then(|table| table.get(key)))
        .map(String::as_str)
}

/// The text of `key`, shared sentences expanded; the key itself when
/// missing everywhere.
pub fn get(language: &str, key: &str) -> String {
    match raw(language, key) {
        Some(value) => match value.strip_prefix('@') {
            Some(common) => get(language, &format!("common.{common}")),
            None => value.to_owned(),
        },
        None => key.to_owned(),
    }
}

pub fn exists(key: &str) -> bool {
    tables()
        .get("fr")
        .is_some_and(|table| table.contains_key(key))
}

/// `key` with `{0}`, `{1}`... replaced by `values`.
pub fn format(language: &str, key: &str, values: &[&str]) -> String {
    let mut text = get(language, key);
    for (index, value) in values.iter().enumerate() {
        text = text.replace(&format!("{{{index}}}"), value);
    }
    text
}

/// Lines `prefix.1`, `prefix.2`... as long as they exist.
pub fn lines(language: &str, prefix: &str) -> Vec<String> {
    (1..)
        .map(|index| format!("{prefix}.{index}"))
        .take_while(|key| exists(key))
        .map(|key| get(language, &key))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_key_and_placeholder() {
        let reference = parse(FILES[0].1);
        let placeholders = |value: &str| {
            (0..6)
                .filter(|index| value.contains(&format!("{{{index}}}")))
                .collect::<Vec<_>>()
        };
        for (language, source) in &FILES[1..] {
            let table = parse(source);
            for (key, value) in &reference {
                let Some(translated) = table.get(key) else {
                    panic!("{language}: missing {key}");
                };
                assert_eq!(
                    placeholders(value),
                    placeholders(translated),
                    "{language}: {key}"
                );
                // Shared sentences are references, never translated.
                assert_eq!(
                    value.starts_with('@'),
                    translated.starts_with('@'),
                    "{language}: {key}"
                );
                if value.starts_with('@') {
                    assert_eq!(value, translated, "{language}: {key}");
                }
            }
            for key in table.keys() {
                assert!(reference.contains_key(key), "{language}: unknown {key}");
            }
        }
        for (key, value) in &reference {
            if let Some(common) = value.strip_prefix('@') {
                assert!(reference.contains_key(&format!("common.{common}")), "{key}");
            }
        }
    }
}
