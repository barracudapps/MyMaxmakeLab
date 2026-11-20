//! Generators of ready-to-cut projects: a box sized for an object, a
//! maze, a jigsaw puzzle. The result lands in the design as vectors.

use crate::app::MyApp;
use crate::i18n::{tr, tr_key};
use eframe::egui::{self, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::Path;
use mml_core::makers::{boxes, jigsaw, maze, test_grid};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Box,
    Maze,
    Jigsaw,
    TestGrid,
}

pub struct MakersDialog {
    kind: Kind,
    boxes: boxes::BoxParams,
    maze: maze::MazeParams,
    solution: bool,
    jigsaw: jigsaw::JigsawParams,
    grid: test_grid::TestGridParams,
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
            ui.horizontal(|ui| {
                ui.selectable_value(&mut dialog.kind, Kind::Box, tr("makers.box"));
                ui.selectable_value(&mut dialog.kind, Kind::Maze, tr("makers.maze"));
                ui.selectable_value(&mut dialog.kind, Kind::Jigsaw, tr("makers.jigsaw"));
                ui.selectable_value(&mut dialog.kind, Kind::TestGrid, tr("makers.test_grid"))
                    .on_hover_text(tr("makers.test_grid_hint"));
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
