//! Projects that check and tune the machine.

use crate::shapes::*;
use crate::{Builder, settings};
use mml_core::geometry::Path;
use mml_core::layer::{FillParams, LineParams, Operation, ProfileSide};
use mml_core::makers::test_grid::{TestGridParams, cells};
use mml_core::{Document, MachineKind};

/// Router calibration part, 100 x 100 mm in 6 mm stock: pockets, holes
/// and an outline of known sizes to measure, engraved first with a
/// V-bit so the job also rehearses a tool change.
pub fn cnc_calibration(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "cnc_calibration");
    b.stock(120.0, 120.0);
    let stock = 6.0;
    let through = stock + 0.3;
    let engrave = b.layer(b.t("layer.engrave"), settings::v_line(0.5));
    let pocket_square = b.layer(b.t("layer.pocket2"), settings::pocket(3.175, 2.0));
    let pocket_round = b.layer(b.t("layer.pocket3"), settings::pocket(3.175, 3.0));
    let holes = b.layer(
        b.t("layer.holes"),
        settings::profile(3.175, through, ProfileSide::Inside, 0),
    );
    let outline = b.layer(
        b.t("layer.outline"),
        settings::profile(3.175, through, ProfileSide::Outside, 4),
    );
    // Engraving: the name, the hole sizes and a 50 mm ruler.
    let mut lines = stroke_text("MyMaxmakeLab", 7.0, point(70.0, 108.0));
    for (diameter, x) in [(6.0, 34.0), (8.0, 52.0), (10.0, 73.0), (12.0, 97.0)] {
        lines.extend(stroke_text(&format!("{diameter:.0}"), 3.5, point(x, 40.0)));
    }
    lines.push(line(point(45.0, 29.0), point(95.0, 29.0)));
    for tick in 0..=10 {
        let x = 45.0 + tick as f64 * 5.0;
        let length = if tick % 2 == 0 { 5.0 } else { 3.0 };
        lines.push(line(point(x, 29.0), point(x, 29.0 + length)));
    }
    lines.extend(stroke_text("0", 3.0, point(45.0, 25.0)));
    lines.extend(stroke_text("50", 3.0, point(95.0, 25.0)));
    b.add(engrave, b.t("layer.engrave"), lines);
    b.add(pocket_square, "30 x 30", vec![rect(30.0, 68.0, 30.0, 30.0)]);
    b.add(pocket_round, "Ø 24", vec![circle(point(92.0, 83.0), 12.0)]);
    let hole_paths: Vec<Path> = [(6.0, 34.0), (8.0, 52.0), (10.0, 73.0), (12.0, 97.0)]
        .iter()
        .map(|(diameter, x)| circle(point(*x, 50.0), diameter / 2.0))
        .collect();
    b.add(holes, b.t("layer.holes"), hole_paths);
    b.add(
        outline,
        "100 x 100",
        vec![rounded_rect(20.0, 20.0, 100.0, 100.0, 5.0)],
    );
    b.step("s1", Vec::new());
    b.step("s2", vec![engrave]);
    b.step("s3", vec![pocket_square, pocket_round, holes, outline]);
    b.step("s4", Vec::new());
    b.finish(MachineKind::Cnc)
}

/// Laser settings: power and speed grid, a comb of slots to find the
/// press fit and a strip of ten squares to measure the kerf.
pub fn laser_tests(language: &str) -> Document {
    let mut b = Builder::new(text_language(language), "laser_tests");
    b.stock(200.0, 150.0);
    let labels = b.layer(b.t("layer.labels"), settings::laser_line());
    let params = TestGridParams {
        power: (10.0, 90.0),
        speed: (1000.0, 5000.0),
        rows: 5,
        columns: 5,
        cell: 10.0,
        gap: 3.0,
    };
    let (gx, gy) = (40.0, 75.0);
    let mut grid_layers = Vec::new();
    let mut label_lines = Vec::new();
    let pitch = params.cell + params.gap;
    let grid = cells(&params);
    for (index, cell) in grid.iter().enumerate() {
        let layer = b.layer(
            format!("{:.0}% · {:.0} mm/min", cell.power, cell.speed),
            Operation::LaserFill(FillParams {
                power: cell.power,
                speed: cell.speed,
                ..FillParams::default()
            }),
        );
        grid_layers.push(layer);
        b.add(
            layer,
            format!("{:.0}% {:.0}", cell.power, cell.speed),
            translate(std::slice::from_ref(&cell.square), gx, gy),
        );
        let (row, column) = (index / params.columns, index % params.columns);
        if column == 0 {
            label_lines.extend(stroke_text(
                &format!("{:.0}%", cell.power),
                3.0,
                point(gx - 7.0, gy + row as f64 * pitch + params.cell / 2.0),
            ));
        }
        if row == params.rows - 1 {
            label_lines.extend(stroke_text(
                &format!("{:.0}", cell.speed),
                2.5,
                point(
                    gx + column as f64 * pitch + params.cell / 2.0,
                    gy + 5.0 * pitch + 2.0,
                ),
            ));
        }
    }
    label_lines.extend(stroke_text(
        "mm/min",
        3.0,
        point(gx + 31.0, gy + 5.0 * pitch + 8.0),
    ));
    // Comb: nine slots from 2.6 to 3.4 mm, 12 mm deep, labelled.
    let (cx, cy, cw, ch) = (112.0, 80.0, 84.0, 34.0);
    let mut comb = vec![(cx, cy), (cx + cw, cy), (cx + cw, cy + ch)];
    for index in (0..9).rev() {
        let width = 2.6 + index as f64 * 0.1;
        let centre = cx + 6.0 + index as f64 * 9.0;
        comb.push((centre + width / 2.0, cy + ch));
        comb.push((centre + width / 2.0, cy + ch - 12.0));
        comb.push((centre - width / 2.0, cy + ch - 12.0));
        comb.push((centre - width / 2.0, cy + ch));
        label_lines.extend(stroke_text(
            &format!("{width:.1}"),
            2.5,
            point(centre, cy + ch - 17.0),
        ));
    }
    comb.push((cx, cy + ch));
    // Kerf strip: ten 10 mm squares sharing their cuts.
    let mut strip = vec![rect(20.0, 25.0, 100.0, 10.0)];
    for index in 1..10 {
        let x = 20.0 + index as f64 * 10.0;
        strip.push(line(point(x, 25.0), point(x, 35.0)));
    }
    label_lines.extend(stroke_text("10 x 10 mm", 3.0, point(70.0, 41.0)));
    b.add(labels, b.t("layer.labels"), label_lines);
    // Raw cuts: the test measures the beam itself, so no kerf
    // compensation here.
    let cut = b.layer(
        b.t("layer.cut"),
        Operation::LaserLine(LineParams {
            speed: 300.0,
            power: 100.0,
            passes: 2,
            air_assist: true,
            kerf: 0.0,
            ..LineParams::default()
        }),
    );
    b.add(cut, b.t("layer.comb"), vec![polygon(&comb)]);
    b.add(cut, b.t("layer.strip"), strip);
    b.step("s1", Vec::new());
    let mut engraving = grid_layers;
    engraving.push(labels);
    b.step("s2", engraving);
    b.step("s3", vec![cut]);
    b.step("s4", Vec::new());
    b.finish(MachineKind::Laser)
}

/// The language code as a static string for the builder.
pub(crate) fn text_language(language: &str) -> &'static str {
    crate::text::supported(language)
}
