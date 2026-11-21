//! Generators of ready-to-cut projects: a box sized for an object, a
//! maze, a jigsaw puzzle. The result lands in the design as vectors.

use crate::app::MyApp;
use crate::i18n::{tr, tr_key};
use eframe::egui::{self, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::Path;
use mml_core::makers::{boxes, codes, gear, jigsaw, maze, round_box, test_grid};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Box,
    Maze,
    Jigsaw,
    TestGrid,
    Gear,
    Code,
    RoundBox,
}

pub struct MakersDialog {
    kind: Kind,
    boxes: boxes::BoxParams,
    maze: maze::MazeParams,
    solution: bool,
    jigsaw: jigsaw::JigsawParams,
    grid: test_grid::TestGridParams,
    gear: gear::GearParams,
    code: codes::CodeParams,
    round: round_box::RoundBoxParams,
}

impl MakersDialog {
    /// Opens on the laser test grid with the given ranges.
    pub fn test_grid(grid: test_grid::TestGridParams) -> Self {
        Self {
            kind: Kind::TestGrid,
            grid,
            ..Self::default()
        }
    }
}

impl Default for MakersDialog {
    fn default() -> Self {
        Self {
            kind: Kind::Box,
            boxes: boxes::BoxParams::default(),
            maze: maze::MazeParams::default(),
            solution: false,
            jigsaw: jigsaw::JigsawParams::default(),
            grid: test_grid::TestGridParams::default(),
            gear: gear::GearParams::default(),
            code: codes::CodeParams::default(),
            round: round_box::RoundBoxParams::default(),
        }
    }
}

fn millimetres(ui: &mut egui::Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.add(DragValue::new(value).range(range).suffix(" mm").speed(0.1));
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |time| time.as_nanos() as u64)
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.makers else {
        return;
    };
    let mut open = true;
    let mut create = false;
    egui::Window::new(tr("makers.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut dialog.kind, Kind::Box, tr("makers.box"));
                ui.selectable_value(&mut dialog.kind, Kind::RoundBox, tr("makers.round_box"))
                    .on_hover_text(tr("makers.round_box_hint"));
                ui.selectable_value(&mut dialog.kind, Kind::Maze, tr("makers.maze"));
                ui.selectable_value(&mut dialog.kind, Kind::Jigsaw, tr("makers.jigsaw"));
                ui.selectable_value(&mut dialog.kind, Kind::TestGrid, tr("makers.test_grid"))
                    .on_hover_text(tr("makers.test_grid_hint"));
                ui.selectable_value(&mut dialog.kind, Kind::Gear, tr("makers.gear"));
                ui.selectable_value(&mut dialog.kind, Kind::Code, tr("makers.code"));
            });
            ui.separator();
            egui::Grid::new("makers-grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| match dialog.kind {
                    Kind::Box => {
                        let params = &mut dialog.boxes;
                        ui.label(tr("makers.inside"))
                            .on_hover_text(tr("makers.inside_hint"));
                        ui.horizontal(|ui| {
                            for value in &mut params.inner {
                                millimetres(ui, value, 5.0..=1000.0);
                            }
                        });
                        ui.end_row();
                        ui.label(tr("makers.thickness"));
                        millimetres(ui, &mut params.thickness, 0.5..=30.0);
                        ui.end_row();
                        ui.label(tr("makers.finger"));
                        millimetres(ui, &mut params.finger, 2.0..=100.0);
                        ui.end_row();
                        ui.label(tr("makers.kerf"))
                            .on_hover_text(tr("makers.kerf_hint"));
                        ui.add(
                            DragValue::new(&mut params.kerf)
                                .range(0.0..=1.0)
                                .suffix(" mm")
                                .speed(0.01),
                        );
                        ui.end_row();
                        ui.label(tr("makers.lid"));
                        ui.checkbox(&mut params.lid, "");
                        ui.end_row();
                    }
                    Kind::Maze => {
                        let params = &mut dialog.maze;
                        ui.label(tr("makers.cells"));
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut params.columns).range(2..=200));
                            ui.label("x");
                            ui.add(DragValue::new(&mut params.rows).range(2..=200));
                        });
                        ui.end_row();
                        ui.label(tr("makers.cell"));
                        millimetres(ui, &mut params.cell, 1.0..=100.0);
                        ui.end_row();
                        ui.label(tr("makers.output"));
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut params.output,
                                maze::MazeOutput::Walls,
                                tr("makers.walls"),
                            )
                            .on_hover_text(tr("makers.walls_hint"));
                            ui.selectable_value(
                                &mut params.output,
                                maze::MazeOutput::Corridors,
                                tr("makers.corridors"),
                            )
                            .on_hover_text(tr("makers.corridors_hint"));
                        });
                        ui.end_row();
                        ui.label(tr("makers.solution"));
                        ui.checkbox(&mut dialog.solution, "");
                        ui.end_row();
                        ui.label(tr("makers.variation"));
                        if ui.button(tr("ornament.roll")).clicked() {
                            params.seed = seed();
                        }
                        ui.end_row();
                    }
                    Kind::TestGrid => {
                        let params = &mut dialog.grid;
                        ui.label(tr("makers.grid_power"));
                        ui.horizontal(|ui| {
                            ui.add(
                                DragValue::new(&mut params.power.0)
                                    .range(1.0..=100.0)
                                    .suffix(" %"),
                            );
                            ui.label("-");
                            ui.add(
                                DragValue::new(&mut params.power.1)
                                    .range(1.0..=100.0)
                                    .suffix(" %"),
                            );
                            ui.add(DragValue::new(&mut params.rows).range(1..=20).prefix("x "));
                        });
                        ui.end_row();
                        ui.label(tr("makers.grid_speed"));
                        ui.horizontal(|ui| {
                            ui.add(
                                DragValue::new(&mut params.speed.0)
                                    .range(10.0..=60_000.0)
                                    .suffix(" mm/min")
                                    .speed(10.0),
                            );
                            ui.label("-");
                            ui.add(
                                DragValue::new(&mut params.speed.1)
                                    .range(10.0..=60_000.0)
                                    .suffix(" mm/min")
                                    .speed(10.0),
                            );
                            ui.add(
                                DragValue::new(&mut params.columns)
                                    .range(1..=20)
                                    .prefix("x "),
                            );
                        });
                        ui.end_row();
                        ui.label(tr("makers.cell"));
                        millimetres(ui, &mut params.cell, 2.0..=50.0);
                        ui.end_row();
                    }
                    Kind::RoundBox => {
                        let params = &mut dialog.round;
                        ui.label(tr("makers.inside"));
                        ui.horizontal(|ui| {
                            for value in &mut params.inner {
                                millimetres(ui, value, 10.0..=1000.0);
                            }
                        });
                        ui.end_row();
                        ui.label(tr("makers.radius"));
                        millimetres(ui, &mut params.radius, 2.0..=300.0);
                        ui.end_row();
                        ui.label(tr("makers.thickness"));
                        millimetres(ui, &mut params.thickness, 1.0..=10.0);
                        ui.end_row();
                        ui.label(tr("makers.hinge_pitch"))
                            .on_hover_text(tr("makers.hinge_pitch_hint"));
                        ui.add(
                            DragValue::new(&mut params.hinge_pitch)
                                .range(0.5..=5.0)
                                .suffix(" mm")
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label(tr("makers.closed"))
                            .on_hover_text(tr("makers.closed_hint"));
                        ui.checkbox(&mut params.lid, "");
                        ui.end_row();
                    }
                    Kind::Gear => {
                        let params = &mut dialog.gear;
                        ui.label(tr("makers.teeth"));
                        ui.add(DragValue::new(&mut params.teeth).range(6..=300));
                        ui.end_row();
                        ui.label(tr("makers.module"))
                            .on_hover_text(tr("makers.module_hint"));
                        ui.add(
                            DragValue::new(&mut params.module)
                                .range(0.3..=20.0)
                                .suffix(" mm")
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label(tr("makers.pressure_angle"));
                        ui.horizontal(|ui| {
                            for angle in [14.5, 20.0, 25.0] {
                                ui.selectable_value(
                                    &mut params.pressure_angle,
                                    angle,
                                    format!("{angle}°"),
                                );
                            }
                        });
                        ui.end_row();
                        ui.label(tr("makers.bore"));
                        millimetres(ui, &mut params.bore, 0.0..=500.0);
                        ui.end_row();
                        ui.label(tr("makers.backlash"))
                            .on_hover_text(tr("makers.backlash_hint"));
                        ui.add(
                            DragValue::new(&mut params.backlash)
                                .range(0.0..=1.0)
                                .suffix(" mm")
                                .speed(0.01),
                        );
                        ui.end_row();
                        ui.label("");
                        ui.label(
                            RichText::new(crate::i18n::trf(
                                "makers.gear_size",
                                &[
                                    &format!("{:.1}", params.pitch_diameter()),
                                    &format!("{:.1}", params.outside_diameter()),
                                ],
                            ))
                            .weak(),
                        );
                        ui.end_row();
                    }
                    Kind::Code => {
                        let params = &mut dialog.code;
                        ui.label(tr("makers.code_kind"));
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut params.kind, codes::CodeKind::Qr, "QR code");
                            ui.selectable_value(
                                &mut params.kind,
                                codes::CodeKind::Code128,
                                "Code 128",
                            );
                        });
                        ui.end_row();
                        ui.label(tr("makers.code_text"));
                        ui.text_edit_singleline(&mut params.text);
                        ui.end_row();
                        ui.label(tr("makers.code_size"));
                        millimetres(ui, &mut params.size, 5.0..=500.0);
                        ui.end_row();
                        if params.kind == codes::CodeKind::Code128 {
                            ui.label(tr("makers.code_height"));
                            millimetres(ui, &mut params.height, 2.0..=200.0);
                            ui.end_row();
                        }
                        ui.label(tr("makers.code_inverted"))
                            .on_hover_text(tr("makers.code_inverted_hint"));
                        ui.checkbox(&mut params.inverted, "");
                        ui.end_row();
                        if codes::generate(params).is_none() {
                            ui.label("");
                            ui.label(
                                RichText::new(tr("makers.code_invalid"))
                                    .color(crate::theme::WARNING),
                            );
                            ui.end_row();
                        }
                    }
                    Kind::Jigsaw => {
                        let params = &mut dialog.jigsaw;
                        ui.label(tr("makers.size"));
                        ui.horizontal(|ui| {
                            millimetres(ui, &mut params.width, 20.0..=2000.0);
                            millimetres(ui, &mut params.height, 20.0..=2000.0);
                        });
                        ui.end_row();
                        ui.label(tr("makers.pieces"));
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut params.columns).range(1..=100));
                            ui.label("x");
                            ui.add(DragValue::new(&mut params.rows).range(1..=100));
                        });
                        ui.end_row();
                        ui.label(tr("makers.knob"));
                        ui.add(egui::Slider::new(&mut params.knob, 0.1..=0.3));
                        ui.end_row();
                        ui.label(tr("makers.variation"));
                        if ui.button(tr("ornament.roll")).clicked() {
                            params.seed = seed();
                        }
                        ui.end_row();
                    }
                });
            create = ui
                .button(RichText::new(tr("makers.create")).strong())
                .clicked();
        });
    if create && let Some(dialog) = app.makers.take() {
        let layer = app.layer_for(false);
        let mut shapes: Vec<(String, Vec<Path>)> = Vec::new();
        match dialog.kind {
            Kind::Box => {
                for panel in boxes::layout(&dialog.boxes, app.machine.width.max(100.0)) {
                    shapes.push((
                        tr_key(&format!("makers.panel_{}", panel.name)),
                        vec![panel.outline],
                    ));
                }
            }
            Kind::Maze => {
                let generated = maze::generate(&dialog.maze);
                shapes.push((tr("makers.maze").to_owned(), generated.paths));
                if dialog.solution {
                    shapes.push((tr("makers.solution").to_owned(), vec![generated.solution]));
                }
            }
            Kind::Jigsaw => shapes.push((
                tr("makers.jigsaw").to_owned(),
                jigsaw::generate(&dialog.jigsaw),
            )),
            Kind::TestGrid => {
                create_test_grid(app, &dialog.grid);
                return;
            }
            Kind::Gear => shapes.push((tr("makers.gear").to_owned(), gear::generate(&dialog.gear))),
            Kind::RoundBox => {
                let params = &dialog.round;
                let made = round_box::generate(params);
                let [w, d, _] = params.inner;
                let t = params.thickness;
                let wall_height = made.wall[0].bounds().height();
                let place = |paths: Vec<Path>, dx: f64, dy: f64| -> Vec<Path> {
                    paths
                        .into_iter()
                        .map(|path| {
                            Path::new(
                                path.points
                                    .iter()
                                    .map(|p| mml_core::geometry::Point::new(p.x + dx, p.y + dy))
                                    .collect(),
                                path.closed,
                            )
                        })
                        .collect()
                };
                // Wall along the bottom, panels above it side by side.
                let row = wall_height + 8.0 + d / 2.0 + 2.0 * t;
                shapes.push((tr("makers.wall").to_owned(), made.wall));
                shapes.push((
                    tr("makers.panel_bottom").to_owned(),
                    place(made.bottom, w / 2.0 + 2.0 * t, row),
                ));
                if let Some(lid) = made.lid {
                    shapes.push((
                        tr("makers.panel_lid").to_owned(),
                        place(lid, 1.5 * w + 6.0 * t + 8.0, row),
                    ));
                }
            }
            Kind::Code => {
                let Some(paths) = codes::generate(&dialog.code) else {
                    app.makers = Some(dialog);
                    return;
                };
                // Squares and bars are areas: a fill layer engraves them.
                let fill = fill_layer(app);
                app.begin_edit("history.add");
                let shape = Shape::new(dialog.code.text.clone(), fill, ShapeKind::Paths { paths });
                app.selection = vec![shape.id];
                app.document.shapes.push(shape);
                app.end_edit();
                app.canvas.request_fit();
                return;
            }
        }
        app.begin_edit("history.add");
        app.selection.clear();
        for (name, paths) in shapes {
            let shape = Shape::new(name, layer, ShapeKind::Paths { paths });
            app.selection.push(shape.id);
            app.document.shapes.push(shape);
        }
        app.end_edit();
        app.canvas.request_fit();
    } else if !open {
        app.makers = None;
    }
}

/// A layer that fills areas: the current one if it does, else a new one.
fn fill_layer(app: &mut MyApp) -> mml_core::LayerId {
    use mml_core::layer::{Layer, Operation};
    let fills = |operation: &Operation| {
        matches!(
            operation,
            Operation::LaserFill(_) | Operation::CncPocket(_) | Operation::CncVCarve(_)
        )
    };
    if let Some(layer) = app
        .document
        .layer(app.current_layer)
        .filter(|layer| fills(&layer.operation))
    {
        return layer.id;
    }
    if let Some(layer) = app
        .document
        .layers
        .iter()
        .find(|layer| fills(&layer.operation))
    {
        return layer.id;
    }
    let operation = match app.machine.kind {
        mml_core::machine::MachineKind::Laser => Operation::LaserFill(Default::default()),
        mml_core::machine::MachineKind::Cnc => Operation::CncVCarve(Default::default()),
    };
    let layer = Layer::new(tr("layer.fill"), app.document.next_layer_color(), operation);
    let id = layer.id;
    app.document.layers.push(layer);
    id
}

/// Scan offset test: one small square per speed, filled back and forth
/// with fine lines on its own layer, the speed engraved beside it. A
/// wrong offset shows as ragged left and right edges; the offset to
/// enter is the width of that raggedness.
pub fn create_scan_test(app: &mut MyApp) {
    use mml_core::geometry::Point;
    use mml_core::layer::{FillParams, Layer, LineParams, Operation};
    let family = mml_io::hershey_families()
        .into_iter()
        .next()
        .unwrap_or_default();
    app.begin_edit("history.add");
    app.selection.clear();
    let labels = Layer::new(
        tr("makers.grid_labels"),
        app.document.next_layer_color(),
        Operation::LaserLine(LineParams {
            power: 20.0,
            speed: 1500.0,
            ..LineParams::default()
        }),
    );
    let label_layer = labels.id;
    app.document.layers.push(labels);
    for (index, speed) in [1000.0, 2000.0, 3000.0, 4500.0, 6000.0]
        .into_iter()
        .enumerate()
    {
        let layer = Layer::new(
            format!("{speed:.0} mm/min"),
            app.document.next_layer_color(),
            Operation::LaserFill(FillParams {
                speed,
                power: 30.0,
                interval: 0.25,
                bidirectional: true,
                overscan: 3.0,
                ..FillParams::default()
            }),
        );
        let id = layer.id;
        app.document.layers.push(layer);
        let y = index as f64 * 14.0;
        let square = Path::new(
            vec![
                Point::new(0.0, y),
                Point::new(20.0, y),
                Point::new(20.0, y + 10.0),
                Point::new(0.0, y + 10.0),
            ],
            true,
        );
        let shape = Shape::new(
            format!("{speed:.0}"),
            id,
            ShapeKind::Paths {
                paths: vec![square],
            },
        );
        app.selection.push(shape.id);
        app.document.shapes.push(shape);
        let mut text = mml_io::hershey_text(&format!("{speed:.0}"), &family, 4.0, 1.2);
        for point in text.iter_mut().flat_map(|path| path.points.iter_mut()) {
            point.x += 24.0;
            point.y += y + 3.0;
        }
        let label = Shape::new(
            format!("{speed:.0}"),
            label_layer,
            ShapeKind::Paths { paths: text },
        );
        app.selection.push(label.id);
        app.document.shapes.push(label);
    }
    app.end_edit();
    app.canvas.request_fit();
    app.notify(tr("scan.pattern_done"), crate::app::Level::Info);
}

/// One fill layer per square with its own power and speed, labels
/// engraved lightly next to the rows and above the columns.
fn create_test_grid(app: &mut MyApp, params: &test_grid::TestGridParams) {
    use mml_core::layer::{FillParams, Layer, LineParams, Operation};
    let cells = test_grid::cells(params);
    app.begin_edit("history.add");
    app.selection.clear();
    let family = mml_io::hershey_families()
        .into_iter()
        .next()
        .unwrap_or_default();
    let labels = Layer::new(
        tr("makers.grid_labels"),
        app.document.next_layer_color(),
        Operation::LaserLine(LineParams {
            power: 20.0,
            speed: 1500.0,
            ..LineParams::default()
        }),
    );
    let label_layer = labels.id;
    app.document.layers.push(labels);
    let size = (params.cell * 0.35).clamp(2.0, 5.0);
    let add_label = |app: &mut MyApp, text: &str, x: f64, y: f64, right: bool| {
        let mut paths = mml_io::hershey_text(text, &family, size, 1.2);
        let bounds = paths
            .iter()
            .fold(mml_core::geometry::Rect::EMPTY, |rect, path| {
                rect.union(path.bounds())
            });
        let dx = if right {
            x - bounds.max.x
        } else {
            x - (bounds.min.x + bounds.max.x) / 2.0
        };
        for point in paths.iter_mut().flat_map(|path| path.points.iter_mut()) {
            point.x += dx;
            point.y += y - bounds.min.y;
        }
        let shape = Shape::new(text.to_owned(), label_layer, ShapeKind::Paths { paths });
        app.selection.push(shape.id);
        app.document.shapes.push(shape);
    };
    let pitch = params.cell + params.gap;
    for row in 0..params.rows.max(1) {
        let power = cells[row * params.columns.max(1)].power;
        add_label(
            app,
            &format!("{power:.0}%"),
            -params.gap,
            row as f64 * pitch + params.cell / 3.0,
            true,
        );
    }
    for (column, cell) in cells.iter().take(params.columns.max(1)).enumerate() {
        let speed = cell.speed;
        let top = params.rows.max(1) as f64 * pitch;
        add_label(
            app,
            &format!("{speed:.0}"),
            column as f64 * pitch + params.cell / 2.0,
            top,
            false,
        );
    }
    for cell in cells {
        let layer = Layer::new(
            format!("{:.0}% · {:.0} mm/min", cell.power, cell.speed),
            app.document.next_layer_color(),
            Operation::LaserFill(FillParams {
                power: cell.power,
                speed: cell.speed,
                ..FillParams::default()
            }),
        );
        let layer_id = layer.id;
        app.document.layers.push(layer);
        let shape = Shape::new(
            layer_name_of(&cell),
            layer_id,
            ShapeKind::Paths {
                paths: vec![cell.square],
            },
        );
        app.selection.push(shape.id);
        app.document.shapes.push(shape);
    }
    app.end_edit();
    app.canvas.request_fit();
}

fn layer_name_of(cell: &test_grid::TestCell) -> String {
    format!("{:.0}% {:.0}", cell.power, cell.speed)
}
