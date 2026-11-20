use crate::svg_import::collect;
use mml_core::geometry::{Path, Point};
use std::sync::{Arc, OnceLock};

/// System fonts are scanned once; it takes a noticeable time.
pub(crate) fn font_database() -> Arc<usvg::fontdb::Database> {
    static DATABASE: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DATABASE
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
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
        assert!(bounds.min.y < 0.0, "descender of g goes below the baseline");
    }
}
