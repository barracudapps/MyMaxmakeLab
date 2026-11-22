//! Shortcuts over the canvas that shorten the usual path from an idea
//! to a finished part: a start panel on an empty design, a bar of layer
//! colours to sort shapes in one click, and a button to the next step.

use crate::app::{Mode, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, UiBuilder};
use mml_core::geometry::Point;
use mml_core::layer::Layer;

pub fn overlays(app: &mut MyApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    if app.mode == Mode::Design {
        // Closed for this session once dismissed, or for good when asked.
        // Picking a drawing tool means drawing: the panel steps aside.
        let drawing = app.canvas.tool != crate::canvas::Tool::Select;
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            app.start_panel = false;
        }
        if app.document.shapes.is_empty()
            && app.start_panel
            && app.settings.show_start_panel
            && !drawing
        {
            start_panel(app, ui, canvas);
        }
        layer_bar(app, ui, canvas);
    }
    next_step(app, ui, canvas);
}

/// A large button: the action in bold, what it does underneath.
fn start_action(ui: &mut egui::Ui, title: &str, detail: &str) -> bool {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.append(
        title,
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(15.0),
            color: ui.visuals().strong_text_color(),
            ..Default::default()
        },
    );
    job.append(
        &format!("\n{detail}"),
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(11.5),
            color: ui.visuals().weak_text_color(),
            ..Default::default()
        },
    );
    job.wrap.max_width = ui.available_width() - 20.0;
    // A left-aligned, full-width layout puts the text on the left.
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
        ui.add(egui::Button::new(job).wrap()).clicked()
    })
    .inner
}

/// What to do on an empty design: every way to start, and the last
/// projects.
fn start_panel(app: &mut MyApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    let width = 380.0f32.min(canvas.width() - 40.0);
    if width < 200.0 {
        return;
    }
    let area = egui::Rect::from_center_size(canvas.center(), egui::vec2(width, 10.0));
    let frame = egui::Frame::popup(ui.style())
        .inner_margin(14.0)
        .corner_radius(12.0);
    ui.scope_builder(
        UiBuilder::new().max_rect(egui::Rect::from_min_size(
            egui::pos2(
                area.left(),
                canvas.top() + (canvas.height() * 0.12).max(20.0),
            ),
            egui::vec2(width, canvas.height()),
        )),
        |ui| {
            frame.show(ui, |ui| {
                ui.set_width(width - 28.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("start.title")).size(18.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("×").size(18.0)).frame(false))
                            .on_hover_text(tr("start.close"))
                            .clicked()
                        {
                            app.start_panel = false;
                        }
                    });
                });
                ui.add_space(6.0);
                let laser = app.machine.kind == mml_core::machine::MachineKind::Laser;
                let actions: Vec<(&str, &str, u8)> = [
                    ("start.import", "start.import_hint", 0),
                    ("start.library", "start.library_hint", 1),
                    ("start.text", "start.text_hint", 2),
                    ("start.makers", "start.makers_hint", 3),
                    ("start.samples", "start.samples_hint", 7),
                    ("start.tablet", "start.tablet_hint", 4),
                    ("start.paper", "start.paper_hint", 5),
                    ("start.open", "start.open_hint", 6),
                ]
                .into_iter()
                .filter(|(_, _, id)| *id != 5 || laser)
                .collect();
                for (title, detail, id) in actions {
                    if start_action(ui, tr(title), tr(detail)) {
                        match id {
                            0 => app.import_dialog(),
                            1 => app.shapes_library.open = true,
                            2 => {
                                let center =
                                    Point::new(app.machine.width / 2.0, app.machine.height / 2.0);
                                app.text_dialog = Some(crate::text_tools::TextDialog::new(center));
                            }
                            3 => app.makers = Some(Default::default()),
                            4 => app.pairing.open = true,
                            5 => app.paper = Some(crate::paper_dialog::PaperDialog::new(app)),
                            7 => app.samples_open = true,
                            _ => app.open_dialog(),
                        }
                    }
                }
                let recent: Vec<_> = app.settings.recent.iter().take(4).cloned().collect();
                if !recent.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr("start.recent")).weak());
                    for path in recent {
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if ui
                            .link(name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            app.open_path(&path);
                        }
                    }
                }
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(tr("start.drop")).weak().small()).wrap());
                let mut hide = !app.settings.show_start_panel;
                if ui
                    .checkbox(&mut hide, RichText::new(tr("start.never")).small())
                    .on_hover_text(tr("start.never_hint"))
                    .changed()
                {
                    app.settings.show_start_panel = !hide;
                }
            });
        },
    );
}

/// One swatch per layer under the canvas: with shapes selected a click
/// moves them to that layer, otherwise it makes it the current layer.
fn layer_bar(app: &mut MyApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    let size = 22.0;
    let count = app.document.layers.len() as f32 + 1.0;
    let bar = egui::Rect::from_min_size(
        egui::pos2(canvas.left() + 12.0, canvas.bottom() - size - 12.0),
        egui::vec2(count * (size + 6.0) + 12.0, size + 8.0),
    );
    ui.painter()
        .rect_filled(bar, 8.0, ui.visuals().panel_fill.gamma_multiply(0.92));
    let selecting = !app.selection.is_empty();
    let mut move_to = None;
    let mut make_current = None;
    let mut add = false;
    for (index, layer) in app.document.layers.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            bar.min + egui::vec2(6.0 + index as f32 * (size + 6.0), 4.0),
            egui::vec2(size, size),
        );
        let response = ui.interact(
            rect,
            egui::Id::unique(("layer-swatch", layer.id)),
            Sense::click(),
        );
        let [r, g, b] = layer.color;
        let current = layer.id == app.current_layer;
        ui.painter()
            .rect_filled(rect, 5.0, Color32::from_rgb(r, g, b));
        if current || response.hovered() {
            let color = if current {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            ui.painter().rect_stroke(
                rect.expand(2.0),
                6.0,
                Stroke::new(2.0, color),
                egui::StrokeKind::Outside,
            );
        }
        let hint = if selecting {
            trf("layers.bar_move", &[&layer.name])
        } else {
            trf(
                "layers.bar_current",
                &[&layer.name, &tr(layer.operation.kind_key())],
            )
        };
        if response.on_hover_text(hint).clicked() {
            if selecting {
                move_to = Some(layer.id);
            } else {
                make_current = Some(layer.id);
            }
        }
    }
    let plus = egui::Rect::from_min_size(
        bar.min + egui::vec2(6.0 + (count - 1.0) * (size + 6.0), 4.0),
        egui::vec2(size, size),
    );
    let response = ui.interact(plus, egui::Id::unique("layer-add"), Sense::click());
    ui.painter().rect_stroke(
        plus,
        5.0,
        Stroke::new(1.5, ui.visuals().weak_text_color()),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        plus.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(16.0),
        ui.visuals().text_color(),
    );
    if response.on_hover_text(tr("layers.bar_add")).clicked() {
        add = true;
    }
    if let Some(layer) = make_current {
        app.current_layer = layer;
    }
    if add {
        app.begin_edit("history.add_layer");
        let layer = Layer::new(
            trf("layers.default_name", &[&(app.document.layers.len() + 1)]),
            app.document.next_layer_color(),
            app.default_vector_operation(),
        );
        let id = layer.id;
        app.document.layers.push(layer);
        if selecting {
            move_selection(app, id);
        }
        app.current_layer = id;
        app.end_edit();
    } else if let Some(layer) = move_to {
        app.begin_edit("history.layer");
        move_selection(app, layer);
        app.current_layer = layer;
        app.end_edit();
    }
}

fn move_selection(app: &mut MyApp, layer: mml_core::LayerId) {
    let selection = app.selection.clone();
    for shape in app
        .document
        .shapes
        .iter_mut()
        .filter(|shape| selection.contains(&shape.id))
    {
        shape.layer = layer;
    }
}

/// The step after this one, as a button in the corner of the canvas.
fn next_step(app: &mut MyApp, ui: &mut egui::Ui, canvas: egui::Rect) {
    let (label, target) = match app.mode {
        Mode::Design if !app.document.shapes.is_empty() => (tr("flow.to_prepare"), Mode::Prepare),
        Mode::Prepare if app.job.built.is_some() => (tr("flow.to_machine"), Mode::Run),
        Mode::Run
            if app
                .job
                .built
                .as_ref()
                .is_some_and(|built| built.stats.cut_length > 0.0)
                && app.device.is_connected()
                && !app.device.job_running() =>
        {
            (tr("flow.to_start"), Mode::Run)
        }
        _ => return,
    };
    let size = egui::vec2(220.0, 40.0);
    let rect =
        egui::Rect::from_min_size(canvas.right_bottom() - size - egui::vec2(16.0, 14.0), size);
    let button = egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
        .fill(crate::theme::ACCENT)
        .corner_radius(20.0);
    if ui.put(rect, button).clicked() {
        app.job.scroll_to_job = app.mode == Mode::Run;
        app.mode = target;
    }
}
