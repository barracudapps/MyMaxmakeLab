use crate::svg_import::collect;
use mml_core::geometry::{Path, Point};
use std::sync::{Arc, RwLock};

/// Open-licensed fonts shipped with the application (see
/// fonts/LICENSES.md), so lettering is the same on every computer.
const BUNDLED: [&[u8]; 19] = [
    include_bytes!("../fonts/AlfaSlabOne-Regular.ttf"),
    include_bytes!("../fonts/AllertaStencil-Regular.ttf"),
    include_bytes!("../fonts/AmaticSC-Bold.ttf"),
    include_bytes!("../fonts/AmaticSC-Regular.ttf"),
    include_bytes!("../fonts/BebasNeue-Regular.ttf"),
    include_bytes!("../fonts/BlackOpsOne-Regular.ttf"),
    include_bytes!("../fonts/Caveat[wght].ttf"),
    include_bytes!("../fonts/CinzelDecorative-Regular.ttf"),
    include_bytes!("../fonts/DancingScript[wght].ttf"),
    include_bytes!("../fonts/GreatVibes-Regular.ttf"),
    include_bytes!("../fonts/Lobster-Regular.ttf"),
    include_bytes!("../fonts/Montserrat[wght].ttf"),
    include_bytes!("../fonts/Orbitron[wght].ttf"),
    include_bytes!("../fonts/Pacifico-Regular.ttf"),
    include_bytes!("../fonts/PermanentMarker-Regular.ttf"),
    include_bytes!("../fonts/Sacramento-Regular.ttf"),
    include_bytes!("../fonts/SpecialElite-Regular.ttf"),
    include_bytes!("../fonts/StardosStencil-Bold.ttf"),
    include_bytes!("../fonts/StardosStencil-Regular.ttf"),
];

static DATABASE: RwLock<Option<Arc<usvg::fontdb::Database>>> = RwLock::new(None);

/// System fonts plus bundled ones, scanned once (it takes a noticeable
/// time) and extended by `add_font_directory`.
pub(crate) fn font_database() -> Arc<usvg::fontdb::Database> {
    if let Some(database) = DATABASE.read().ok().and_then(|guard| guard.clone()) {
        return database;
    }
    let mut database = usvg::fontdb::Database::new();
    database.load_system_fonts();
    for data in BUNDLED {
        database.load_font_data(data.to_vec());
    }
    // Generic families point to fonts a minimal Linux does not have
    // (Arial, Courier New...): fall back to bundled ones so text never
    // disappears. Systems that have them keep their usual look.
    let present: Vec<bool> = ["Arial", "Courier New", "Comic Sans MS", "Impact"]
        .iter()
        .map(|family| {
            database
                .faces()
                .any(|face| face.families.iter().any(|(name, _)| name == family))
        })
        .collect();
    if !present[0] {
        database.set_sans_serif_family("Montserrat");
    }
    if !present[1] {
        database.set_monospace_family("Special Elite");
    }
    if !present[2] {
        database.set_cursive_family("Dancing Script");
    }
    if !present[3] {
        database.set_fantasy_family("Lobster");
    }
    let database = Arc::new(database);
    if let Ok(mut guard) = DATABASE.write() {
        *guard = Some(database.clone());
    }
    database
}

/// Loads the fonts of a directory (the user font folder) in addition to
/// the system and bundled fonts.
pub fn add_font_directory(directory: &std::path::Path) {
    let mut database = (*font_database()).clone();
    database.load_fonts_dir(directory);
    if let Ok(mut guard) = DATABASE.write() {
        *guard = Some(Arc::new(database));
    }
}

/// Families of the bundled fonts, to list them first.
pub fn bundled_families() -> Vec<String> {
    let mut database = usvg::fontdb::Database::new();
    for data in BUNDLED {
        database.load_font_data(data.to_vec());
    }
    let mut families: Vec<String> = database
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    families.sort_unstable();
    families.dedup();
    families
}

pub fn font_families() -> Vec<String> {
    let mut families: Vec<String> = font_database()
        .faces()
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    families.sort_unstable_by_key(|name| name.to_lowercase());
    families.dedup();
    families
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub family: String,
    /// Cap height is about 70 % of this size, in mm.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    /// Line height as a multiple of the size.
    pub line_spacing: f64,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "sans-serif".into(),
            size: 10.0,
            bold: false,
            italic: false,
            line_spacing: 1.2,
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Converts text to outlines through usvg so that kerning, ligatures and
/// fallback fonts behave like in any SVG renderer. The result is in mm,
/// first baseline on Y = 0.
pub fn text_to_paths(text: &str, style: &TextStyle) -> Vec<Path> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if crate::hershey::is_hershey(&style.family) {
        return crate::hershey::hershey_text(text, &style.family, style.size, style.line_spacing);
    }
    let lines: String = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let dy = if index == 0 {
                0.0
            } else {
                style.size * style.line_spacing
            };
            format!(
                r#"<tspan x="0" dy="{dy}" xml:space="preserve">{}</tspan>"#,
                escape(line)
            )
        })
        .collect();
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="100000">
        <text x="0" y="50000" font-family="{family}" font-size="{size}" font-weight="{weight}" font-style="{slant}">{lines}</text></svg>"#,
        family = escape(&style.family),
        size = style.size,
        weight = if style.bold { "bold" } else { "normal" },
        slant = if style.italic { "italic" } else { "normal" },
    );
    let options = usvg::Options {
        fontdb: font_database(),
        ..usvg::Options::default()
    };
    let Ok(tree) = usvg::Tree::from_data(svg.as_bytes(), &options) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    collect(tree.root(), &mut |_, group| paths.extend(group));
    paths
        .into_iter()
        .map(|path| {
            Path::new(
                path.points
                    .into_iter()
                    .map(|point| Point::new(point.x, 50000.0 - point.y))
                    .collect(),
                path.closed,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_converted_with_the_baseline_on_zero() {
        let paths = text_to_paths(
            "Ag",
            &TextStyle {
                size: 10.0,
                ..TextStyle::default()
            },
        );
        if font_database().is_empty() {
            return;
        }
        let bounds = mml_core::geometry::bounds_of(&paths);
        assert!(bounds.max.y > 5.0 && bounds.max.y < 10.0);
        assert!(bundled_families().iter().any(|family| family == "Pacifico"));
        let script = text_to_paths(
            "Ag",
            &TextStyle {
                family: "Pacifico".into(),
                ..TextStyle::default()
            },
        );
        assert!(!script.is_empty());
        assert!(bounds.min.y < 0.0, "descender of g goes below the baseline");
    }
}
