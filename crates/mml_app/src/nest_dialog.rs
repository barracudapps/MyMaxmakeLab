//! "Nest on the sheet": places the selected parts, and as many copies
//! as asked, close together by their real outline to save material.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Path, Rect, Transform};
use mml_core::nest::{NestParams, group_parts, nest};

pub struct NestDialog {
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    pub spacing: f64,
    pub rotate: bool,
    pub copies: u32,
}

impl NestDialog {
    pub fn new(app: &MyApp) -> Self {
        Self {
            width: app.machine.width,
            height: app.machine.height,
            margin: 5.0,
            spacing: 3.0,
            rotate: true,
            copies: 1,
        }
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.nest else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    let bed = (app.machine.width, app.machine.height);
    let selected = app.selection.len();
    egui::Window::new(tr("nest.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_max_width(340.0);
            ui.label(RichText::new(tr("nest.intro")).weak());
            egui::Grid::new("nest-grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("nest.sheet"));
                    ui.horizontal(|ui| {
                        ui.add(
                            DragValue::new(&mut dialog.width)
                                .range(10.0..=5000.0)
                                .suffix(" mm"),
                        );
                        ui.label("×");
                        ui.add(
                            DragValue::new(&mut dialog.height)
                                .range(10.0..=5000.0)
                                .suffix(" mm"),
                        );
                        if ui
                            .small_button(tr("nest.bed"))
                            .on_hover_text(tr("nest.bed_hint"))
                            .clicked()
                        {
                            (dialog.width, dialog.height) = bed;
                        }
                    });
                    ui.end_row();
                    ui.label(tr("nest.margin"));
                    ui.add(
                        DragValue::new(&mut dialog.margin)
                            .range(0.0..=100.0)
                            .suffix(" mm")
                            .speed(0.1),
                    );
                    ui.end_row();
                    ui.label(tr("nest.spacing"))
                        .on_hover_text(tr("nest.spacing_hint"));
                    ui.add(
                        DragValue::new(&mut dialog.spacing)
                            .range(0.0..=50.0)
                            .suffix(" mm")
                            .speed(0.1),
                    );
                    ui.end_row();
                    ui.label(tr("nest.copies"))
                        .on_hover_text(tr("nest.copies_hint"));
                    ui.add(DragValue::new(&mut dialog.copies).range(1..=500));
                    ui.end_row();
                    ui.label(tr("nest.rotate"));
                    ui.checkbox(&mut dialog.rotate, "");
                    ui.end_row();
                });
            ui.label(RichText::new(tr("nest.sheet_hint")).weak().small());
            ui.add_enabled_ui(selected > 0, |ui| {
                apply = ui.button(tr("nest.apply")).clicked();
            });
        });
    if apply {
        let params = NestParams {
            sheet: (dialog.width, dialog.height),
            margin: dialog.margin,
            spacing: dialog.spacing,
            rotate: dialog.rotate,
        };
        let copies = dialog.copies.max(1) as usize;
        run(app, params, copies);
    }
    if !open {
        app.nest = None;
    }
}

fn outline(shape: &Shape) -> Vec<Path> {
    if shape.is_image() {
        let bounds = shape.bounds();
        return vec![Path::new(bounds.corners().to_vec(), true)];
    }
    shape.world_paths()
}

/// An imported drawing is one shape holding many outlines: for nesting,
/// each separate outline (with what lies inside it) becomes a part.
fn islands(shape: &Shape) -> Option<Vec<Shape>> {
    if !matches!(shape.kind, ShapeKind::Paths { .. }) {
        return None;
    }
    let paths = shape.world_paths();
    let bounds: Vec<Rect> = paths.iter().map(Path::bounds).collect();
    let membership = group_parts(&bounds);
    let count = membership.iter().max().map_or(0, |last| last + 1);
    if count < 2 {
        return None;
    }
    let pieces = (0..count)
        .map(|island| {
            let members: Vec<&Path> = paths
                .iter()
                .zip(&membership)
                .filter(|(_, member)| **member == island)
                .map(|(path, _)| path)
                .collect();
            let mut area = Rect::EMPTY;
            members
                .iter()
                .flat_map(|path| &path.points)
                .for_each(|point| area.include(*point));
            let center = area.center();
            let mut piece = shape.clone();
            piece.id = mml_core::ShapeId::new();
            piece.kind = ShapeKind::Paths {
                paths: members
                    .iter()
                    .map(|path| {
                        Path::new(
                            path.points.iter().map(|point| *point - center).collect(),
                            path.closed,
                        )
                    })
                    .collect(),
            };
            piece.transform = Transform::translate(center.x, center.y);
            piece
        })
        .collect();
    Some(pieces)
}

fn run(app: &mut MyApp, params: NestParams, copies: usize) {
    let mut split = Vec::new();
    let originals: Vec<Shape> = app
        .document
        .shapes
        .iter()
        .filter(|shape| app.selection.contains(&shape.id) && !shape.locked)
        .flat_map(|shape| match islands(shape) {
            Some(pieces) => {
                split.push(shape.id);
                pieces
            }
            None => vec![shape.clone()],
        })
        .collect();
    if originals.is_empty() {
        return;
    }
    let bounds: Vec<Rect> = originals.iter().map(Shape::bounds).collect();
    let membership = group_parts(&bounds);
    let part_count = membership.iter().max().map_or(0, |last| last + 1);
    // Every copy of every part, the originals first.
    let mut groups: Vec<Vec<Shape>> = Vec::with_capacity(part_count * copies);
    for copy in 0..copies {
        for part in 0..part_count {
            let shapes = originals
                .iter()
                .zip(&membership)
                .filter(|(_, member)| **member == part)
                .map(|(shape, _)| {
                    let mut shape = shape.clone();
                    if copy > 0 {
                        shape.id = mml_core::ShapeId::new();
                    }
                    shape
                })
                .collect();
            groups.push(shapes);
        }
    }
    let outlines: Vec<Vec<Path>> = groups
        .iter()
        .map(|shapes| shapes.iter().flat_map(outline).collect())
        .collect();
    let nesting = nest(&outlines, &params);
    let placed = nesting.placements.iter().flatten().count();
    if placed == 0 {
        app.notify(tr("nest.nothing"), Level::Warning);
        return;
    }
    app.begin_edit("history.nest");
    app.document
        .shapes
        .retain(|shape| !split.contains(&shape.id));
    let mut selection = Vec::new();
    let mut left_out = 0;
    for (index, (shapes, placement)) in groups.into_iter().zip(nesting.placements).enumerate() {
        let original = index < part_count;
        if placement.is_none() {
            left_out += 1;
            if !original {
                continue;
            }
        }
        for mut shape in shapes {
            if let Some(transform) = placement {
                shape.transform = shape.transform.then(transform);
            }
            selection.push(shape.id);
            match app
                .document
                .shapes
                .iter_mut()
                .find(|existing| existing.id == shape.id)
            {
                Some(existing) => *existing = shape,
                None => app.document.shapes.push(shape),
            }
        }
    }
    app.selection = selection;
    app.end_edit();
    let used = (nesting.used_height / params.sheet.1 * 100.0).clamp(0.0, 100.0);
    if left_out > 0 {
        app.notify(trf("nest.left_out", &[&left_out, &placed]), Level::Warning);
    } else {
        app.notify(
            trf(
                "nest.done",
                &[
                    &placed,
                    &format!("{:.0}", nesting.used_height),
                    &format!("{used:.0}"),
                ],
            ),
            Level::Info,
        );
    }
}
