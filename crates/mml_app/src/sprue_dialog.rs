//! "Sprue" (grappe): arranges the selected parts in a frame, like a
//! model kit or a wooden 3D puzzle board, and keeps each part attached
//! by bridges (laser) or tabs (CNC) placed on calm stretches of its
//! outline.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::Transform;
use mml_core::layer::{CncProfileParams, Layer, LineParams, Operation, ProfileSide};
use mml_core::machine::MachineKind;

pub struct SprueDialog {
    pub spacing: f64,
    pub margin: f64,
    pub frame: bool,
    pub corner_radius: f64,
    pub bridge_width: f64,
    /// 0: automatic.
    pub bridges: u32,
}

impl Default for SprueDialog {
    fn default() -> Self {
        Self {
            spacing: 3.0,
            margin: 6.0,
            frame: true,
            corner_radius: 3.0,
            bridge_width: 0.6,
            bridges: 0,
        }
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.sprue else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    let count = app.selection.len();
    egui::Window::new(tr("sprue.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("sprue.intro")).weak());
            egui::Grid::new("sprue-grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("sprue.spacing"));
                    ui.add(
                        DragValue::new(&mut dialog.spacing)
                            .range(0.5..=50.0)
                            .suffix(" mm")
                            .speed(0.1),
                    );
                    ui.end_row();
                    ui.label(tr("sprue.frame"));
                    ui.checkbox(&mut dialog.frame, "");
                    ui.end_row();
                    if dialog.frame {
                        ui.label(tr("sprue.margin"));
                        ui.add(
                            DragValue::new(&mut dialog.margin)
                                .range(1.0..=100.0)
                                .suffix(" mm")
                                .speed(0.1),
                        );
                        ui.end_row();
                        ui.label(tr("sprue.corner"));
                        ui.add(
                            DragValue::new(&mut dialog.corner_radius)
                                .range(0.0..=50.0)
                                .suffix(" mm")
                                .speed(0.1),
                        );
                        ui.end_row();
                    }
                    ui.label(tr("sprue.bridge_width"))
                        .on_hover_text(tr("sprue.bridge_width_hint"));
                    ui.add(
                        DragValue::new(&mut dialog.bridge_width)
                            .range(0.0..=10.0)
                            .suffix(" mm")
                            .speed(0.05),
                    );
                    ui.end_row();
                    ui.label(tr("sprue.bridges"));
                    ui.horizontal(|ui| {
                        let mut automatic = dialog.bridges == 0;
                        if ui
                            .checkbox(&mut automatic, tr("sprue.automatic"))
                            .on_hover_text(tr("sprue.automatic_hint"))
                            .changed()
                        {
                            dialog.bridges = if automatic { 0 } else { 2 };
                        }
                        if !automatic {
                            ui.add(DragValue::new(&mut dialog.bridges).range(1..=12));
                        }
                    });
                    ui.end_row();
                });
            apply = ui
                .add_enabled(
                    count > 0,
                    egui::Button::new(RichText::new(trf("sprue.apply", &[&count])).strong()),
                )
                .clicked();
        });
    if apply {
        let dialog = app.sprue.take().unwrap();
        arrange(app, &dialog);
    } else if !open {
        app.sprue = None;
    }
}

fn arrange(app: &mut MyApp, dialog: &SprueDialog) {
    let selected: Vec<usize> = (0..app.document.shapes.len())
        .filter(|&index| app.selection.contains(&app.document.shapes[index].id))
        .collect();
    if selected.is_empty() {
        return;
    }
    app.begin_edit("history.sprue");
    let bounds: Vec<_> = selected
        .iter()
        .map(|&index| app.document.shapes[index].bounds())
        .collect();
    let sizes: Vec<(f64, f64)> = bounds
        .iter()
        .map(|rect| (rect.width(), rect.height()))
        .collect();
    let margin = if dialog.frame { dialog.margin } else { 0.0 };
    let usable = (app.machine.width - 2.0 * margin).max(10.0);
    let (corners, (width, height)) = mml_core::attach::shelf_pack(&sizes, usable, dialog.spacing);
    for ((&index, rect), corner) in selected.iter().zip(&bounds).zip(&corners) {
        let shape = &mut app.document.shapes[index];
        let (dx, dy) = (
            margin + corner.x - rect.min.x,
            margin + corner.y - rect.min.y,
        );
        shape.transform = shape.transform.then(Transform::translate(dx, dy));
    }
    // Bridges on the layers of the parts.
    let layers: Vec<_> = selected
        .iter()
        .map(|&index| app.document.shapes[index].layer)
        .collect();
    for layer in app
        .document
        .layers
        .iter_mut()
        .filter(|layer| layers.contains(&layer.id))
    {
        match &mut layer.operation {
            Operation::LaserLine(params) => {
                params.bridge_width = dialog.bridge_width;
                params.bridge_count = dialog.bridges;
            }
            Operation::CncProfile(params) => {
                params.tabs = if dialog.bridges == 0 {
                    2
                } else {
                    dialog.bridges
                };
                params.tab_width = params.tab_width.max(dialog.bridge_width);
            }
            _ => {}
        }
    }
    if dialog.frame {
        let cnc = app.machine.kind == MachineKind::Cnc && !app.machine.laser_mode;
        let operation = if cnc {
            Operation::CncProfile(CncProfileParams {
                side: ProfileSide::Outside,
                ..CncProfileParams::default()
            })
        } else {
            Operation::LaserLine(LineParams::default())
        };
        // Last layer: the parts are cut while the board is still held.
        let layer = Layer::new(
            tr("sprue.frame_layer"),
            app.document.next_layer_color(),
            operation,
        );
        let layer_id = layer.id;
        app.document.layers.push(layer);
        let mut frame = Shape::new(
            tr("sprue.frame_layer"),
            layer_id,
            ShapeKind::Rectangle {
                width: width + 2.0 * margin,
                height: height + 2.0 * margin,
                corner_radius: dialog.corner_radius,
            },
        );
        frame.transform =
            Transform::translate((width + 2.0 * margin) / 2.0, (height + 2.0 * margin) / 2.0);
        app.document.shapes.push(frame);
    }
    app.end_edit();
    app.canvas.request_fit();
    app.notify(trf("sprue.done", &[&selected.len()]), Level::Info);
}
