//! QR codes and Code 128 barcodes as filled squares and bars, ready to
//! engrave (fill) or mark.

use crate::geometry::{Path, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    Qr,
    Code128,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeParams {
    pub kind: CodeKind,
    pub text: String,
    /// Width of the whole code, mm (QR: side; barcode: length).
    pub size: f64,
    /// Barcode height, mm.
    pub height: f64,
    /// Engrave the light parts instead (for dark materials that turn
    /// light when burnt, like anodised aluminium or slate).
    pub inverted: bool,
}

impl Default for CodeParams {
    fn default() -> Self {
        Self {
            kind: CodeKind::Qr,
            text: "https://".into(),
            size: 30.0,
            height: 15.0,
            inverted: false,
        }
    }
}

fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Path {
    Path::new(
        vec![
            Point::new(x, y),
            Point::new(x + width, y),
            Point::new(x + width, y + height),
            Point::new(x, y + height),
        ],
        true,
    )
}

/// Squares or bars to burn, lower-left at the origin; `None` when the
/// text cannot be encoded.
pub fn generate(params: &CodeParams) -> Option<Vec<Path>> {
    match params.kind {
        CodeKind::Qr => qr(params),
        CodeKind::Code128 => code128(params),
    }
}

fn qr(params: &CodeParams) -> Option<Vec<Path>> {
    let code = qrcode::QrCode::new(params.text.as_bytes()).ok()?;
    let width = code.width();
    let quiet = if params.inverted { 2 } else { 0 };
    let total = width + 2 * quiet;
    let cell = params.size / total as f64;
    let colors = code.to_colors();
    let mut paths = Vec::new();
    for row in 0..total {
        // Merge runs of dark modules on a row into one bar: fewer, longer
        // shapes engrave faster and cleaner.
        let mut run: Option<usize> = None;
        for column in 0..=total {
            let dark = column < total && {
                let inside = (quiet..quiet + width).contains(&row)
                    && (quiet..quiet + width).contains(&column);
                let module = inside
                    && colors[(row - quiet) * width + (column - quiet)] == qrcode::Color::Dark;
                module != params.inverted
            };
            match (dark, run) {
                (true, None) => run = Some(column),
                (false, Some(start)) => {
                    let y = (total - 1 - row) as f64 * cell;
                    paths.push(rectangle(
                        start as f64 * cell,
                        y,
                        (column - start) as f64 * cell,
                        cell,
                    ));
                    run = None;
                }
                _ => {}
            }
        }
    }
    Some(paths)
}

/// Bar patterns of Code 128 (widths of bar, space, bar, space, bar,
/// space), values 0 to 105 (103 to 105 are the start codes).
const PATTERNS: [&str; 106] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212",
    "221213", "221312", "231212", "112232", "122132", "122231", "113222", "123122", "123221",
    "223211", "221132", "221231", "213212", "223112", "312131", "311222", "321122", "321221",
    "312212", "322112", "322211", "212123", "212321", "232121", "111323", "131123", "131321",
    "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131",
    "113123", "113321", "133121", "313121", "211331", "231131", "213113", "213311", "213131",
    "311123", "311321", "331121", "312113", "312311", "332111", "314111", "221411", "431111",
    "111224", "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114",
    "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242",
    "121142", "121241", "114212", "124112", "124211", "411212", "421112", "421211", "212141",
    "214121", "412121", "111143", "111341", "131141", "114113", "114311", "411113", "411311",
    "113141", "114131", "311141", "411131", "211412", "211214", "211232",
];
const STOP: &str = "2331112";

fn code128(params: &CodeParams) -> Option<Vec<Path>> {
    // Code set B covers printable ASCII.
    if params.text.is_empty() || !params.text.bytes().all(|b| (32..127).contains(&b)) {
        return None;
    }
    let mut values = vec![104usize];
    values.extend(params.text.bytes().map(|b| (b - 32) as usize));
    let checksum = values
        .iter()
        .enumerate()
        .map(|(index, value)| value * index.max(1))
        .sum::<usize>()
        % 103;
    values.push(checksum);
    let mut widths: Vec<usize> = values
        .iter()
        .flat_map(|value| PATTERNS[*value].bytes().map(|b| (b - b'0') as usize))
        .collect();
    widths.extend(STOP.bytes().map(|b| (b - b'0') as usize));
    // Ten modules of quiet zone on each side are part of the length.
    let modules: usize = widths.iter().sum::<usize>() + 20;
    let module = params.size / modules as f64;
    let mut x = 10.0 * module;
    let mut paths = Vec::new();
    for (index, width) in widths.iter().enumerate() {
        let w = *width as f64 * module;
        if index % 2 == 0 {
            paths.push(rectangle(x, 0.0, w, params.height));
        }
        x += w;
    }
    Some(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_fits_the_requested_size() {
        let params = CodeParams {
            text: "https://example.com".into(),
            ..CodeParams::default()
        };
        let paths = generate(&params).unwrap();
        let max = paths
            .iter()
            .flat_map(|path| &path.points)
            .fold(0.0f64, |m, p| m.max(p.x).max(p.y));
        assert!((max - 30.0).abs() < 1e-9);
        assert!(paths.len() > 20);
    }

    #[test]
    fn every_code128_symbol_is_eleven_modules_wide() {
        for pattern in PATTERNS {
            assert_eq!(
                pattern.bytes().map(|b| (b - b'0') as usize).sum::<usize>(),
                11
            );
        }
        assert_eq!(STOP.bytes().map(|b| (b - b'0') as usize).sum::<usize>(), 13);
    }

    #[test]
    fn code128_checksum_and_length() {
        // "PJJ123C" is the classic example: checksum value 54.
        let params = CodeParams {
            kind: CodeKind::Code128,
            text: "PJJ123C".into(),
            size: 50.0,
            ..CodeParams::default()
        };
        let paths = generate(&params).unwrap();
        // Start + 7 characters + checksum: 9 symbols of 3 bars, stop 4.
        assert_eq!(paths.len(), 9 * 3 + 4);
        assert!(
            generate(&CodeParams {
                kind: CodeKind::Code128,
                text: "é".into(),
                ..CodeParams::default()
            })
            .is_none()
        );
    }
}
