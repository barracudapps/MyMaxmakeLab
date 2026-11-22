//! "Puzzle on lines": a children's puzzle whose pieces follow the heavy
//! lines of the selected drawing (vectors or a picture). The drawing is
//! kept to be engraved; the cuts and the tray go on a cutting layer.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_core::document::{Shape, ShapeId, ShapeKind};
use mml_core::layer::{Layer, Operation};
use mml_core::makers::line_puzzle::{self, LinePuzzleParams, Mask};

pub struct LinePuzzleDialog {
    params: LinePuzzleParams,
    tray: bool,
    margin: f64,
    /// Put the vector drawing on an engraving layer.
    engrave: bool,
    /// Selection whose picture lines were last measured, so the heavy
    /// line follows a newly selected picture but not a typed value.
    measured: Vec<ShapeId>,
    heavy_edited: bool,
}

impl Default for LinePuzzleDialog {
    fn default() -> Self {
        let params = LinePuzzleParams::default();
        Self {
            params,
            tray: true,
            margin: params.frame.unwrap_or(12.0),
            engrave: true,
            measured: Vec::new(),
            heavy_edited: false,
        }
    }
}

/// The selected shapes, in document order.
fn selected_drawing(app: &MyApp) -> Vec<Shape> {
    app.document
        .shapes
        .iter()
        .filter(|shape| app.selection.contains(&shape.id))
        .cloned()
        .collect()
}

/// The drawing on the grid the puzzle is cut from.
fn drawing_mask(drawing: &[Shape], params: &LinePuzzleParams) -> Option<Mask> {
    let mut bounds = mml_core::geometry::Rect::EMPTY;
    for shape in drawing {
        bounds = bounds.union(shape.bounds());
    }
    if drawing.is_empty() || bounds.is_empty() {
        return None;
    }
    let margin = params.frame.unwrap_or(0.0) + params.heavy * 2.0 + 2.0;
    let mut mask = Mask::covering(bounds, margin);
    for shape in drawing {
        mask.add_shape(shape, params.heavy * 1.25);
    }
    Some(mask)
}

/// Heavy line fitting the outlines of the selected pictures: a bit
/// wider than their measured lines, so every outline is a cut and the
/// detail is left to the smallest-piece merge. Vectors have no width of
/// their own to measure (it comes from this very setting).
fn measured_heavy(drawing: &[Shape], params: &LinePuzzleParams) -> Option<f64> {
    if !drawing.iter().any(Shape::is_image) {
        return None;
    }
    let pictures: Vec<Shape> = drawing
        .iter()
        .filter(|shape| shape.is_image())
        .cloned()
        .collect();
    let width = drawing_mask(&pictures, params)?.line_width()?;
    Some(((width * 1.3) / 0.05).round().max(1.0) * 0.05)
}

/// A layer that cuts along lines: the current one if it does, else the
/// first such layer, else a new one.
fn cut_layer(app: &mut MyApp) -> mml_core::LayerId {
    let cuts = |operation: &Operation| {
        matches!(
            operation,
            Operation::LaserLine(_) | Operation::CncProfile(_)
        )
    };
    if let Some(layer) = app
        .document
        .layer(app.current_layer)
        .filter(|layer| cuts(&layer.operation))
    {
        return layer.id;
    }
    if let Some(layer) = app
        .document
        .layers
        .iter()
        .find(|layer| cuts(&layer.operation))
    {
        return layer.id;
    }
    let layer = Layer::new(
        tr("layer.cut"),
        app.document.next_layer_color(),
        app.default_vector_operation(),
    );
    let id = layer.id;
    app.document.layers.push(layer);
    id
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.line_puzzle else {
        return;
    };
    let selected = app.selection.len();
    if !dialog.heavy_edited && dialog.measured != app.selection {
        dialog.measured = app.selection.clone();
        let drawing = selected_drawing(app);
        let dialog = app.line_puzzle.as_mut().expect("dialog checked above");
        if let Some(heavy) = measured_heavy(&drawing, &dialog.params) {
            dialog.params.heavy = heavy.clamp(0.1, 20.0);
        }
    }
    let Some(dialog) = &mut app.line_puzzle else {
        return;
    };
    let mut open = true;
    let mut measure = false;
    let mut create = false;
    egui::Window::new(tr("line_puzzle.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.label(RichText::new(tr("line_puzzle.intro")).weak());
            egui::Grid::new("line-puzzle")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("line_puzzle.heavy"))
                        .on_hover_text(tr("line_puzzle.heavy_hint"));
                    ui.horizontal(|ui| {
                        let heavy = ui.add(
                            DragValue::new(&mut dialog.params.heavy)
                                .range(0.1..=20.0)
                                .suffix(" mm")
                                .speed(0.05),
                        );
                        if heavy.changed() {
                            dialog.heavy_edited = true;
                        }
                        measure = ui
                            .button(tr("line_puzzle.measure"))
                            .on_hover_text(tr("line_puzzle.measure_hint"))
                            .clicked();
                    });
                    ui.end_row();
                    ui.label(tr("line_puzzle.min_piece"))
                        .on_hover_text(tr("line_puzzle.min_piece_hint"));
                    ui.add(
                        DragValue::new(&mut dialog.params.min_piece)
                            .range(2.0..=200.0)
                            .suffix(" mm")
                            .speed(0.2),
                    );
                    ui.end_row();
                    ui.checkbox(&mut dialog.tray, tr("line_puzzle.tray"))
                        .on_hover_text(tr("line_puzzle.tray_hint"));
                    ui.add_enabled(
                        dialog.tray,
                        DragValue::new(&mut dialog.margin)
                            .range(2.0..=100.0)
                            .suffix(" mm")
                            .speed(0.2),
                    );
                    ui.end_row();
                });
            ui.checkbox(&mut dialog.engrave, tr("line_puzzle.engrave"))
                .on_hover_text(tr("line_puzzle.engrave_hint"));
            if selected == 0 {
                ui.label(RichText::new(tr("line_puzzle.select")).color(crate::theme::WARNING));
            }
            ui.add_enabled_ui(selected > 0, |ui| {
                create = ui
                    .button(RichText::new(tr("line_puzzle.create")).strong())
                    .clicked();
            });
        });
    if measure {
        let drawing = selected_drawing(app);
        let dialog = app.line_puzzle.as_mut().expect("dialog shown");
        match measured_heavy(&drawing, &dialog.params) {
            Some(heavy) => {
                dialog.params.heavy = heavy.clamp(0.1, 20.0);
                dialog.heavy_edited = false;
                dialog.measured = app.selection.clone();
            }
            None => app.notify(tr("line_puzzle.measure_none"), Level::Warning),
        }
        return;
    }
    let Some(dialog) = &mut app.line_puzzle else {
        return;
    };
    if create {
        let params = LinePuzzleParams {
            frame: dialog.tray.then_some(dialog.margin),
            ..dialog.params
        };
        let engrave = dialog.engrave;
        run(app, params, engrave);
    }
    if !open {
        app.line_puzzle = None;
    }
}

fn run(app: &mut MyApp, params: LinePuzzleParams, engrave: bool) {
    let drawing = selected_drawing(app);
    let Some(mask) = drawing_mask(&drawing, &params) else {
        return;
    };
    let Some(puzzle) = line_puzzle::generate(&mask, &params) else {
        app.notify(tr("line_puzzle.nothing"), Level::Warning);
        return;
    };
    app.begin_edit("history.line_puzzle");
    let cut = cut_layer(app);
    let vectors = drawing.iter().any(|shape| !shape.is_image());
    if engrave && vectors {
        let name = tr("line_puzzle.engrave_layer");
        let fill = match app.document.layers.iter().find(|layer| layer.name == name) {
            Some(layer) => layer.id,
            None => {
                let layer = Layer::new(
                    name,
                    app.document.next_layer_color(),
                    app.default_vector_operation(),
                );
                let id = layer.id;
                // Engraving first, then the cuts that free the pieces.
                app.document.layers.insert(0, layer);
                id
            }
        };
        for shape in app
            .document
            .shapes
            .iter_mut()
            .filter(|shape| app.selection.contains(&shape.id) && !shape.is_image())
        {
            shape.layer = fill;
        }
    }
    let mut created = Vec::new();
    let pieces = Shape::new(
        tr("line_puzzle.pieces_name"),
        cut,
        ShapeKind::Paths { paths: puzzle.cuts },
    );
    created.push(pieces);
    if let Some(frame) = puzzle.frame {
        created.push(Shape::new(
            tr("line_puzzle.tray_name"),
            cut,
            ShapeKind::Paths { paths: vec![frame] },
        ));
    }
    app.selection = created.iter().map(|shape| shape.id).collect();
    app.document.shapes.extend(created);
    app.end_edit();
    app.notify(trf("line_puzzle.done", &[&puzzle.pieces]), Level::Info);
}
