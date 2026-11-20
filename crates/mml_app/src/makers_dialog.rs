//! Generators of ready-to-cut projects: a box sized for an object, a
//! maze, a jigsaw puzzle. The result lands in the design as vectors.

use crate::app::MyApp;
use crate::i18n::{tr, tr_key};
use eframe::egui::{self, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::Path;
use mml_core::makers::{boxes, jigsaw, maze};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Box,
    Maze,
    Jigsaw,
}

pub struct MakersDialog {
    kind: Kind,
    boxes: boxes::BoxParams,
    maze: maze::MazeParams,
    solution: bool,
    jigsaw: jigsaw::JigsawParams,
}

impl Default for MakersDialog {
    fn default() -> Self {
        Self {
            kind: Kind::Box,
            boxes: boxes::BoxParams::default(),
            maze: maze::MazeParams::default(),
            solution: false,
            jigsaw: jigsaw::JigsawParams::default(),
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
