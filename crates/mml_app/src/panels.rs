use crate::app::{ArrayDialog, Level, Mode, MyApp, PresetDialog};
use crate::i18n::{tr, trf};
use eframe::egui::{self, Color32, DragValue, RichText};
use mml_core::document::{Shape, ShapeKind};
use mml_core::geometry::{Point, Transform};
use mml_core::layer::{
    CncPocketParams, CncProfileParams, DitherMode, FillParams, ImageParams, Layer, LineParams,
    Operation, ProfileSide,
};
use mml_core::machine::{MachineKind, Origin};
use mml_job::JobWarning;

pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(title).strong().size(16.0));
    ui.add_space(4.0);
}

fn number(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    speed: f64,
) -> bool {
    ui.add(
        DragValue::new(value)
            .range(range)
            .suffix(suffix)
            .speed(speed)
            .max_decimals(3),
    )
    .changed()
}

/// Speeds are stored in mm/min and shown in mm/s for lasers if wanted.
fn speed(ui: &mut egui::Ui, value: &mut f64, per_second: bool) -> bool {
    if per_second {
        let mut shown = *value / 60.0;
        let changed = number(ui, &mut shown, 0.1..=2000.0, " mm/s", 1.0);
        if changed {
            *value = shown * 60.0;
        }
        changed
    } else {
        number(ui, value, 1.0..=100_000.0, " mm/min", 10.0)
    }
}

fn percent(ui: &mut egui::Ui, value: &mut f64) -> bool {
    ui.add(
        egui::Slider::new(value, 0.0..=100.0)
            .suffix(" %")
            .max_decimals(1),
    )
    .changed()
}

fn passes(ui: &mut egui::Ui, value: &mut u32) -> bool {
    ui.add(DragValue::new(value).range(1..=100)).changed()
}

pub fn design_panel(app: &mut MyApp, ui: &mut egui::Ui) {
    use crate::theme::card;
    card(ui, |ui| properties(app, ui));
    card(ui, |ui| layers(app, ui));
    card(ui, |ui| operation_editor(app, ui));
}

pub fn prepare_panel(app: &mut MyApp, ui: &mut egui::Ui) {
    use crate::theme::card;
    card(ui, |ui| job_summary(app, ui));
    let external = app
        .job
        .built
        .as_ref()
        .is_some_and(|built| built.external.is_some());
    if external {
        return;
    }
    card(ui, |ui| layers(app, ui));
    card(ui, |ui| operation_editor(app, ui));
}

fn properties(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("props.title"));
    if app.selection.is_empty() {
        ui.label(RichText::new(tr("props.empty")).weak());
        if app.document.shapes.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(tr("props.getting_started")).weak());
        }
        return;
    }
    let bounds = app.document.bounds_of(&app.selection);
    let mut x = bounds.min.x;
    let mut y = bounds.min.y;
    let mut width = bounds.width();
    let mut height = bounds.height();
    let single = (app.selection.len() == 1)
        .then(|| app.document.shape(app.selection[0]).cloned())
        .flatten();
    let mut rotation = single
        .as_ref()
        .map_or(0.0, |shape| shape.transform.rotation_degrees());
    let mut transform = None;
    egui::Grid::new("props-grid")
        .num_columns(4)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            ui.label("X");
            if number(ui, &mut x, -10_000.0..=10_000.0, " mm", 0.1) {
                transform = Some(Transform::translate(x - bounds.min.x, 0.0));
            }
            ui.label("Y");
            if number(ui, &mut y, -10_000.0..=10_000.0, " mm", 0.1) {
                transform = Some(Transform::translate(0.0, y - bounds.min.y));
            }
            ui.end_row();
            let ratio = height / width.max(1e-9);
            ui.label(tr("props.width"));
            if number(ui, &mut width, 0.01..=10_000.0, " mm", 0.1) && bounds.width() > 0.0 {
                let factor = width / bounds.width();
                let (sx, sy) = if app.lock_ratio() {
                    (factor, factor)
                } else {
                    (factor, 1.0)
                };
                transform = Some(Transform::scale(sx, sy).around(bounds.min));
            }
            ui.label(tr("props.height"));
            if number(ui, &mut height, 0.01..=10_000.0, " mm", 0.1) && bounds.height() > 0.0 {
                let factor = height / bounds.height();
                let (sx, sy) = if app.lock_ratio() {
                    (factor, factor)
                } else {
                    (1.0, factor)
                };
                transform = Some(Transform::scale(sx, sy).around(bounds.min));
            }
            let _ = ratio;
            ui.end_row();
            ui.label(tr("props.rotation"));
            if number(ui, &mut rotation, -360.0..=360.0, " °", 0.5) {
                let current = single
                    .as_ref()
                    .map_or(0.0, |shape| shape.transform.rotation_degrees());
                transform =
                    Some(Transform::rotate_degrees(rotation - current).around(bounds.center()));
            }
            let mut lock = app.lock_ratio();
            if ui.checkbox(&mut lock, tr("props.lock_ratio")).changed() {
                app.set_lock_ratio(lock);
            }
            ui.end_row();
        });
    if let Some(transform) = transform {
        for shape in app
            .document
            .shapes
            .iter_mut()
            .filter(|shape| app.selection.contains(&shape.id))
        {
            shape.transform = shape.transform.then(transform);
        }
        app.touch();
    }

    ui.horizontal_wrapped(|ui| {
        let bed = (app.machine.width, app.machine.height);
        let align = [
            ("⬅", "align.left", 0),
            ("↔", "align.hcenter", 1),
            ("➡", "align.right", 2),
            ("⬆", "align.top", 3),
            ("↕", "align.vcenter", 4),
            ("⬇", "align.bottom", 5),
            ("+", "align.bed_center", 6),
        ];
        for (icon, key, kind) in align {
            if ui.button(icon).on_hover_text(tr(key)).clicked() {
                align_selection(app, kind, bed);
            }
        }
    });
    ui.horizontal(|ui| {
        if ui.button(tr("menu.duplicate")).clicked() {
            app.duplicate_selection();
        }
        if ui.button(tr("menu.array")).clicked() {
            app.array_dialog = Some(ArrayDialog {
                columns: 3,
                rows: 2,
                gap_x: 5.0,
                gap_y: 5.0,
            });
        }
        if ui
            .button(RichText::new(tr("menu.delete")).color(crate::theme::DANGER))
            .clicked()
        {
            app.delete_selection();
        }
    });

    let layer_names: Vec<(mml_core::LayerId, String)> = app
        .document
        .layers
        .iter()
        .map(|layer| (layer.id, layer.name.clone()))
        .collect();
    let current = single.as_ref().map(|shape| shape.layer);
    let selected_name = current
        .and_then(|id| {
            layer_names
                .iter()
                .find(|(layer, _)| *layer == id)
                .map(|(_, name)| name.clone())
        })
        .unwrap_or_else(|| tr("props.mixed").to_owned());
    ui.horizontal(|ui| {
        ui.label(tr("props.layer"));
        egui::ComboBox::from_id_salt("shape-layer")
            .selected_text(selected_name)
            .show_ui(ui, |ui| {
                for (id, name) in &layer_names {
                    if ui.selectable_label(current == Some(*id), name).clicked() {
                        app.begin_edit();
                        for shape in app
                            .document
                            .shapes
                            .iter_mut()
                            .filter(|shape| app.selection.contains(&shape.id))
                        {
                            shape.layer = *id;
                        }
                        app.end_edit();
                    }
                }
            });
    });

    if let Some(shape) = single {
        if let Some(link) = app.link_of(shape.id) {
            let name = link
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(trf("link.active", &[&name]))
                        .color(Color32::from_rgb(60, 140, 210)),
                )
                .on_hover_text(tr("link.hint"));
                if ui.small_button(tr("link.unlink")).clicked() {
                    app.unlink(shape.id);
                }
            });
        }
        primitive_editor(app, ui, &shape);
    }
}

fn primitive_editor(app: &mut MyApp, ui: &mut egui::Ui, shape: &Shape) {
    let mut kind = shape.kind.clone();
    let mut changed = false;
    match &mut kind {
        ShapeKind::Rectangle { corner_radius, .. } => {
            ui.horizontal(|ui| {
                ui.label(tr("props.corner_radius"));
                changed |= number(ui, corner_radius, 0.0..=1000.0, " mm", 0.1);
            });
        }
        ShapeKind::Polygon { sides, .. } => {
            ui.horizontal(|ui| {
                ui.label(tr("props.sides"));
                changed |= ui.add(DragValue::new(sides).range(3..=64)).changed();
            });
        }
        ShapeKind::Image {
            image,
            width,
            height,
        } => {
            ui.label(RichText::new(format!("{} × {} px", image.width, image.height)).weak());
            let dpi = 25.4 * image.width as f64
                / (*width * shape.transform.scale_factors().0.abs()).max(1e-9);
            ui.label(RichText::new(trf("props.image_dpi", &[&format!("{dpi:.0}")])).weak());
            let _ = height;
            ui.horizontal(|ui| {
                if ui
                    .button(tr("props.trace"))
                    .on_hover_text(tr("props.trace_hint"))
                    .clicked()
                {
                    let paths = mml_io::trace_image(image, 128, 1.0, *width, *height);
                    if paths.is_empty() {
                        app.notify(tr("props.trace_empty"), Level::Warning);
                    } else {
                        let layer = app.layer_for(false);
                        let mut traced = Shape::new(
                            format!("{} (trace)", shape.name),
                            layer,
                            ShapeKind::Paths { paths },
                        );
                        traced.transform = shape.transform;
                        app.add_shape(traced);
                    }
                }
            });
        }
        ShapeKind::Ellipse { .. } | ShapeKind::Paths { .. } => {}
    }
    if changed && let Some(target) = app.document.shape_mut(shape.id) {
        target.kind = kind;
        app.touch();
    }
}

fn align_selection(app: &mut MyApp, kind: u8, bed: (f64, f64)) {
    let ids = app.selection.clone();
    let all = app.document.bounds_of(&ids);
    let reference = if ids.len() == 1 || kind == 6 {
        mml_core::geometry::Rect::from_points(Point::default(), Point::new(bed.0, bed.1))
    } else {
        all
    };
    app.begin_edit();
    for shape in app
        .document
        .shapes
        .iter_mut()
        .filter(|shape| ids.contains(&shape.id))
    {
        let bounds = if kind == 6 { all } else { shape.bounds() };
        let (dx, dy) = match kind {
            0 => (reference.min.x - bounds.min.x, 0.0),
            1 => (reference.center().x - bounds.center().x, 0.0),
            2 => (reference.max.x - bounds.max.x, 0.0),
            3 => (0.0, reference.max.y - bounds.max.y),
            4 => (0.0, reference.center().y - bounds.center().y),
            5 => (0.0, reference.min.y - bounds.min.y),
            _ => (
                reference.center().x - bounds.center().x,
                reference.center().y - bounds.center().y,
            ),
        };
        shape.transform = shape.transform.then(Transform::translate(dx, dy));
    }
    app.end_edit();
}

fn operation_label(operation: &Operation) -> &'static str {
    tr(operation.kind_key())
}

fn layers(app: &mut MyApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        section(ui, tr("layers.title"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("+", |ui| {
                for operation in all_operations() {
                    if ui.button(operation_label(&operation)).clicked() {
                        app.begin_edit();
                        let color = app.document.next_layer_color();
                        let layer = Layer::new(operation_label(&operation), color, operation);
                        app.current_layer = layer.id;
                        app.document.layers.push(layer);
                        app.end_edit();
                    }
                }
            })
            .response
            .on_hover_text(tr("layers.add"));
        });
    });
    let mut action: Option<(usize, i32)> = None;
    let count = app.document.layers.len();
    for index in 0..count {
        let layer_id = app.document.layers[index].id;
        let used = app.document.shapes_on(layer_id).count();
        let selected = app.current_layer == layer_id;
        let frame = egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(6, 4))
            .corner_radius(6.0)
            .fill(if selected {
                ui.visuals().selection.bg_fill.gamma_multiply(0.35)
            } else {
                Color32::TRANSPARENT
            });
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let layer = &mut app.document.layers[index];
                let mut changed = ui.color_edit_button_srgb(&mut layer.color).changed();
                let eye = if layer.visible { "👁" } else { "🚫" };
                if ui
                    .selectable_label(layer.visible, eye)
                    .on_hover_text(tr("layers.visible"))
                    .clicked()
                {
                    layer.visible = !layer.visible;
                    changed = true;
                }
                let output = if layer.output { "🔥" } else { "⏵" };
                if ui
                    .selectable_label(layer.output, output)
                    .on_hover_text(tr("layers.output"))
                    .clicked()
                {
                    layer.output = !layer.output;
                    changed = true;
                }
                if selected {
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut layer.name).desired_width(90.0))
                        .changed();
                } else if ui
                    .selectable_label(false, RichText::new(&layer.name).strong())
                    .clicked()
                {
                    app.current_layer = layer_id;
                }
                let layer = &app.document.layers[index];
                let label = format!("{} · {}", operation_label(&layer.operation), used);
                if !selected
                    && ui
                        .selectable_label(false, RichText::new(label).weak().small())
                        .clicked()
                {
                    app.current_layer = layer_id;
                }
                if changed {
                    app.touch();
                }
                if selected {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if count > 1
                            && ui
                                .small_button("×")
                                .on_hover_text(tr("layers.remove"))
                                .clicked()
                        {
                            action = Some((index, 0));
                        }
                        if index + 1 < count && ui.small_button("⬇").clicked() {
                            action = Some((index, 1));
                        }
                        if index > 0 && ui.small_button("⬆").clicked() {
                            action = Some((index, -1));
                        }
                        if !app.selection.is_empty()
                            && ui
                                .small_button("⬅")
                                .on_hover_text(tr("layers.assign"))
                                .clicked()
                        {
                            action = Some((index, 2));
                        }
                    });
                }
            });
        });
    }
    if let Some((index, kind)) = action {
        app.begin_edit();
        match kind {
            0 => {
                let id = app.document.layers[index].id;
                app.document.remove_layer(id);
                app.current_layer = app.document.layers[0].id;
            }
            2 => {
                let id = app.document.layers[index].id;
                for shape in app
                    .document
                    .shapes
                    .iter_mut()
                    .filter(|shape| app.selection.contains(&shape.id))
                {
                    shape.layer = id;
                }
            }
            direction => {
                let target = (index as i32 + direction) as usize;
                app.document.layers.swap(index, target);
            }
        }
        app.end_edit();
    }
    ui.label(RichText::new(tr("layers.order_hint")).weak().small());
}

fn all_operations() -> [Operation; 5] {
    [
        Operation::LaserLine(LineParams::default()),
        Operation::LaserFill(FillParams::default()),
        Operation::LaserImage(ImageParams::default()),
        Operation::CncProfile(CncProfileParams::default()),
        Operation::CncPocket(CncPocketParams::default()),
    ]
}

fn operation_editor(app: &mut MyApp, ui: &mut egui::Ui) {
    let Some(index) = app
        .document
        .layers
        .iter()
        .position(|layer| layer.id == app.current_layer)
    else {
        return;
    };
    section(ui, &trf("op.title", &[&app.document.layers[index].name]));
    let per_second = app.settings.laser_speed_per_second;
    let mut operation = app.document.layers[index].operation.clone();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr("op.type"));
        egui::ComboBox::from_id_salt("op-type")
            .selected_text(operation_label(&operation))
            .show_ui(ui, |ui| {
                for candidate in all_operations() {
                    let same =
                        std::mem::discriminant(&candidate) == std::mem::discriminant(&operation);
                    if ui
                        .selectable_label(same, operation_label(&candidate))
                        .clicked()
                        && !same
                    {
                        operation = candidate;
                        changed = true;
                    }
                }
            });
    });
    if ui
        .button(tr("assist.open"))
        .on_hover_text(tr("assist.open_hint"))
        .clicked()
    {
        let layer = app.document.layers[index].id;
        app.assist = Some(crate::assist::AssistDialog {
            layer,
            content: crate::assist::analyze(&app.document, layer),
            material: None,
            thickness: 3.0,
        });
    }
    material_picker(app, ui, &mut operation, &mut changed);
    let mut pause_before = app.document.layers[index].pause_before;
    if ui
        .checkbox(&mut pause_before, tr("op.pause_before"))
        .on_hover_text(tr("op.pause_before_hint"))
        .changed()
    {
        app.document.layers[index].pause_before = pause_before;
        app.touch();
    }
    egui::Grid::new("op-grid")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| match &mut operation {
            Operation::LaserLine(params) => {
                ui.label(tr("op.speed"));
                changed |= speed(ui, &mut params.speed, per_second);
                ui.end_row();
                ui.label(tr("op.power"));
                changed |= percent(ui, &mut params.power);
                ui.end_row();
                ui.label(tr("op.passes"));
                changed |= passes(ui, &mut params.passes);
                ui.end_row();
                ui.label(tr("op.z_step"))
                    .on_hover_text(tr("op.z_step_hint"));
                changed |= number(ui, &mut params.z_step, 0.0..=10.0, " mm", 0.05);
                ui.end_row();
                ui.label(tr("op.air"));
                changed |= ui.checkbox(&mut params.air_assist, "").changed();
                ui.end_row();
            }
            Operation::LaserFill(params) => {
                ui.label(tr("op.speed"));
                changed |= speed(ui, &mut params.speed, per_second);
                ui.end_row();
                ui.label(tr("op.power"));
                changed |= percent(ui, &mut params.power);
                ui.end_row();
                ui.label(tr("op.interval"));
                changed |= number(ui, &mut params.interval, 0.01..=5.0, " mm", 0.005);
                ui.end_row();
                ui.label(tr("op.angle"));
                changed |= number(ui, &mut params.angle, -180.0..=180.0, " °", 1.0);
                ui.end_row();
                ui.label(tr("op.passes"));
                changed |= passes(ui, &mut params.passes);
                ui.end_row();
                ui.label(tr("op.overscan"))
                    .on_hover_text(tr("op.overscan_hint"));
                changed |= number(ui, &mut params.overscan, 0.0..=20.0, " mm", 0.1);
                ui.end_row();
                ui.label(tr("op.bidirectional"));
                changed |= ui.checkbox(&mut params.bidirectional, "").changed();
                ui.end_row();
                ui.label(tr("op.crosshatch"));
                changed |= ui.checkbox(&mut params.crosshatch, "").changed();
                ui.end_row();
                ui.label(tr("op.air"));
                changed |= ui.checkbox(&mut params.air_assist, "").changed();
                ui.end_row();
            }
            Operation::LaserImage(params) => {
                ui.label(tr("op.speed"));
                changed |= speed(ui, &mut params.speed, per_second);
                ui.end_row();
                ui.label(tr("op.power_max"));
                changed |= percent(ui, &mut params.power_max);
                ui.end_row();
                if params.dither == DitherMode::Grayscale {
                    ui.label(tr("op.power_min"));
                    changed |= percent(ui, &mut params.power_min);
                    ui.end_row();
                }
                ui.label(tr("op.dither"));
                egui::ComboBox::from_id_salt("dither")
                    .selected_text(params.dither.label())
                    .show_ui(ui, |ui| {
                        for mode in DitherMode::ALL {
                            changed |= ui
                                .selectable_value(&mut params.dither, mode, mode.label())
                                .changed();
                        }
                    });
                ui.end_row();
                if params.dither == DitherMode::Threshold {
                    ui.label(tr("op.threshold"));
                    changed |= ui
                        .add(egui::Slider::new(&mut params.threshold, 1..=254))
                        .changed();
                    ui.end_row();
                }
                ui.label(tr("op.dpi")).on_hover_text(tr("op.dpi_hint"));
                changed |= number(ui, &mut params.dpi, 25.0..=1270.0, " dpi", 1.0);
                ui.end_row();
                ui.label(tr("op.brightness"));
                changed |= ui
                    .add(egui::Slider::new(&mut params.brightness, -100.0..=100.0))
                    .changed();
                ui.end_row();
                ui.label(tr("op.contrast"));
                changed |= ui
                    .add(egui::Slider::new(&mut params.contrast, -100.0..=100.0))
                    .changed();
                ui.end_row();
                ui.label(tr("op.negative"));
                changed |= ui.checkbox(&mut params.negative, "").changed();
                ui.end_row();
                ui.label(tr("op.passes"));
                changed |= passes(ui, &mut params.passes);
                ui.end_row();
                ui.label(tr("op.overscan"))
                    .on_hover_text(tr("op.overscan_hint"));
                changed |= number(ui, &mut params.overscan, 0.0..=20.0, " mm", 0.1);
                ui.end_row();
                ui.label(tr("op.bidirectional"));
                changed |= ui.checkbox(&mut params.bidirectional, "").changed();
                ui.end_row();
            }
            Operation::CncProfile(params) => {
                ui.label(tr("op.side"));
                ui.horizontal(|ui| {
                    for (side, key) in [
                        (ProfileSide::Outside, "op.outside"),
                        (ProfileSide::Inside, "op.inside"),
                        (ProfileSide::On, "op.on_line"),
                    ] {
                        changed |= ui
                            .selectable_value(&mut params.side, side, tr(key))
                            .changed();
                    }
                });
                ui.end_row();
                changed |= cnc_common(
                    ui,
                    &mut params.tool_diameter,
                    &mut params.depth,
                    &mut params.pass_depth,
                    &mut params.feed,
                    &mut params.plunge,
                    &mut params.spindle,
                    &mut params.climb,
                );
                ui.label(tr("op.tabs"));
                changed |= ui
                    .add(DragValue::new(&mut params.tabs).range(0..=32))
                    .changed();
                ui.end_row();
                if params.tabs > 0 {
                    ui.label(tr("op.tab_width"));
                    changed |= number(ui, &mut params.tab_width, 0.5..=50.0, " mm", 0.1);
                    ui.end_row();
                    ui.label(tr("op.tab_height"));
                    changed |= number(ui, &mut params.tab_height, 0.1..=20.0, " mm", 0.05);
                    ui.end_row();
                }
            }
            Operation::CncPocket(params) => {
                changed |= cnc_common(
                    ui,
                    &mut params.tool_diameter,
                    &mut params.depth,
                    &mut params.pass_depth,
                    &mut params.feed,
                    &mut params.plunge,
                    &mut params.spindle,
                    &mut params.climb,
                );
                ui.label(tr("op.stepover"));
                let mut stepover = params.stepover * 100.0;
                if ui
                    .add(egui::Slider::new(&mut stepover, 5.0..=95.0).suffix(" %"))
                    .changed()
                {
                    params.stepover = stepover / 100.0;
                    changed = true;
                }
                ui.end_row();
            }
        });
    if changed {
        app.document.layers[index].operation = operation;
        app.touch();
    }
}

#[allow(clippy::too_many_arguments)]
fn cnc_common(
    ui: &mut egui::Ui,
    tool: &mut f64,
    depth: &mut f64,
    pass_depth: &mut f64,
    feed: &mut f64,
    plunge: &mut f64,
    spindle: &mut f64,
    climb: &mut bool,
) -> bool {
    let mut changed = false;
    ui.label(tr("op.tool_diameter"));
    changed |= number(ui, tool, 0.1..=50.0, " mm", 0.01);
    ui.end_row();
    ui.label(tr("op.depth"));
    changed |= number(ui, depth, 0.0..=200.0, " mm", 0.05);
    ui.end_row();
    ui.label(tr("op.pass_depth"));
    changed |= number(ui, pass_depth, 0.01..=50.0, " mm", 0.05);
    ui.end_row();
    ui.label(tr("op.feed"));
    changed |= number(ui, feed, 1.0..=20_000.0, " mm/min", 10.0);
    ui.end_row();
    ui.label(tr("op.plunge"));
    changed |= number(ui, plunge, 1.0..=5_000.0, " mm/min", 5.0);
    ui.end_row();
    ui.label(tr("op.spindle"));
    changed |= number(ui, spindle, 0.0..=60_000.0, " rpm", 100.0);
    ui.end_row();
    ui.label(tr("op.climb"));
    changed |= ui.checkbox(climb, "").changed();
    ui.end_row();
    changed
}

fn material_picker(
    app: &mut MyApp,
    ui: &mut egui::Ui,
    operation: &mut Operation,
    changed: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(tr("materials.preset"));
        let compatible: Vec<(usize, String)> = app
            .library
            .presets
            .iter()
            .enumerate()
            .filter(|(_, preset)| {
                std::mem::discriminant(&preset.operation) == std::mem::discriminant(operation)
            })
            .map(|(index, preset)| (index, preset.label()))
            .collect();
        let hint = if compatible.is_empty() {
            tr("materials.none")
        } else {
            tr("materials.choose")
        };
        egui::ComboBox::from_id_salt("preset")
            .selected_text(hint)
            .width(170.0)
            .show_ui(ui, |ui| {
                for (index, label) in &compatible {
                    if ui.selectable_label(false, label).clicked() {
                        *operation = app.library.presets[*index].operation.clone();
                        *changed = true;
                    }
                }
            });
        if ui
            .small_button("+")
            .on_hover_text(tr("materials.save_preset"))
            .clicked()
        {
            app.preset_dialog = Some(PresetDialog {
                material: String::new(),
                thickness: 3.0,
                title: operation_label(operation).to_owned(),
            });
        }
    });
}

fn format_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!("{} h {:02} min", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{} min {:02} s", seconds / 60, seconds % 60)
    }
}

pub fn warning_text(warning: &JobWarning) -> String {
    match warning {
        JobWarning::NothingToOutput => tr("warn.nothing").to_owned(),
        JobWarning::OutOfBounds => tr("warn.out_of_bounds").to_owned(),
        JobWarning::CncOnLaser(layer) => trf("warn.cnc_on_laser", &[layer]),
        JobWarning::LaserOnCnc(layer) => trf("warn.laser_on_cnc", &[layer]),
        JobWarning::ImageOnVectorLayer(layer) => trf("warn.image_on_vector", &[layer]),
        JobWarning::VectorOnImageLayer(layer) => trf("warn.vector_on_image", &[layer]),
        JobWarning::OpenPathsNotFilled(layer) => trf("warn.open_paths", &[layer]),
    }
}

fn job_summary(app: &mut MyApp, ui: &mut egui::Ui) {
    section(ui, tr("job.title"));
    if app.job.building.is_some() && app.job.built.is_none() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr("job.building"));
        });
        return;
    }
    let Some(built) = &app.job.built else {
        ui.label(RichText::new(tr("job.nothing")).weak());
        return;
    };
    if let Some(name) = &built.external {
        ui.label(RichText::new(trf("job.external", &[name])).strong());
        if ui.small_button(tr("job.back_to_design")).clicked() {
            app.job.built = None;
            app.job.built_revision = (0, 0);
            return;
        }
    }
    let stats = built.stats.clone();
    let warnings = built.warnings.clone();
    let lines = stats.line_count;
    let size = built.gcode.len();
    egui::Grid::new("stats")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label(tr("job.duration"));
            ui.label(
                RichText::new(format_duration(stats.duration))
                    .strong()
                    .size(16.0),
            );
            ui.end_row();
            if !stats.cut_bounds.is_empty() {
                ui.label(tr("job.size"));
                ui.label(format!(
                    "{:.1} × {:.1} mm",
                    stats.cut_bounds.width(),
                    stats.cut_bounds.height()
                ));
                ui.end_row();
                ui.label(tr("job.origin"));
                ui.label(format!(
                    "X {:.1}  Y {:.1}",
                    stats.cut_bounds.min.x, stats.cut_bounds.min.y
                ));
                ui.end_row();
            }
            ui.label(tr("job.cut_length"));
            ui.label(format!("{:.0} mm", stats.cut_length));
            ui.end_row();
            ui.label(tr("job.travel_length"));
            ui.label(format!("{:.0} mm", stats.travel_length));
            ui.end_row();
            ui.label(tr("job.lines"));
            ui.label(format!("{lines} · {:.0} kB", size as f64 / 1024.0));
            ui.end_row();
        });
    for warning in &warnings {
        ui.label(
            RichText::new(format!("⚠ {}", warning_text(warning))).color(crate::theme::WARNING),
        );
    }
    if !app.job_is_current() {
        ui.label(RichText::new(tr("job.outdated")).weak());
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let label = if app.job.simulating { "⏸" } else { "▶" };
        if ui.button(label).on_hover_text(tr("job.simulate")).clicked() {
            app.job.simulating = !app.job.simulating;
            if app.job.simulate.is_none_or(|value| value >= 1.0) {
                app.job.simulate = Some(0.0);
            }
        }
        let mut progress = app.job.simulate.unwrap_or(1.0);
        if ui
            .add(egui::Slider::new(&mut progress, 0.0..=1.0).show_value(false))
            .changed()
        {
            app.job.simulate = Some(progress);
            app.job.simulating = false;
            refresh_simulation(app);
        }
    });
    if app.job.simulating {
        // Time based: a whole job plays in about eight seconds whatever
        // the frame rate.
        let delta = ui.input(|input| input.stable_dt).min(0.1) / 8.0;
        let value = app.job.simulate.unwrap_or(0.0) + delta;
        app.job.simulate = Some(value.min(1.0));
        if value >= 1.0 {
            app.job.simulating = false;
        }
        refresh_simulation(app);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button(tr("menu.export_gcode")).clicked() {
            app.export_gcode();
        }
        let run =
            egui::Button::new(RichText::new(tr("job.go_run")).strong()).fill(crate::theme::SUCCESS);
        if ui.add(run).clicked() {
            app.mode = Mode::Run;
        }
    });
}

fn refresh_simulation(app: &mut MyApp) {
    if let (Some(progress), Some(built)) = (app.job.simulate, &app.job.built) {
        let total = built.program.line_count.max(1);
        app.job.preview_line = Some(Some((progress as f64 * total as f64) as usize));
    }
}

pub fn dialogs(app: &mut MyApp, ctx: &egui::Context) {
    crate::workflow::checklist_modal(app, ctx);
    crate::workflow::tutorial(app, ctx);
    crate::relief_dialog::show(app, ctx);
    crate::assist::show(app, ctx);
    text_dialog(app, ctx);
    array_dialog(app, ctx);
    preset_dialog(app, ctx);
    machine_dialog(app, ctx);
    preferences(app, ctx);
    about(app, ctx);
    close_confirmation(app, ctx);
    library_offer(app, ctx);
}

fn text_dialog(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.text_dialog else {
        return;
    };
    let mut open = true;
    let mut create = false;
    egui::Window::new(tr("text.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut dialog.text)
                    .desired_rows(3)
                    .hint_text(tr("text.hint")),
            );
            egui::Grid::new("text-grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("text.font"));
                egui::ComboBox::from_id_salt("font")
                    .selected_text(&dialog.style.family)
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        for family in [
                            "sans-serif".to_owned(),
                            "serif".to_owned(),
                            "monospace".to_owned(),
                        ]
                        .iter()
                        .chain(&dialog.families)
                        {
                            ui.selectable_value(&mut dialog.style.family, family.clone(), family);
                        }
                    });
                ui.end_row();
                ui.label(tr("text.size"));
                ui.add(
                    DragValue::new(&mut dialog.style.size)
                        .range(1.0..=500.0)
                        .suffix(" mm")
                        .speed(0.2),
                );
                ui.end_row();
                ui.label("");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut dialog.style.bold, tr("text.bold"));
                    ui.checkbox(&mut dialog.style.italic, tr("text.italic"));
                });
                ui.end_row();
            });
            if ui
                .add_enabled(
                    !dialog.text.trim().is_empty(),
                    egui::Button::new(tr("text.create")),
                )
                .clicked()
            {
                create = true;
            }
        });
    if create {
        let dialog = app.text_dialog.take().unwrap();
        let paths = mml_io::text_to_paths(&dialog.text, &dialog.style);
        if paths.is_empty() {
            app.notify(tr("text.no_font"), Level::Error);
            return;
        }
        let bounds = mml_core::geometry::bounds_of(&paths);
        let center = bounds.center();
        let paths = paths
            .into_iter()
            .map(|path| {
                mml_core::geometry::Path::new(
                    path.points
                        .into_iter()
                        .map(|point| point - center)
                        .collect(),
                    path.closed,
                )
            })
            .collect();
        let layer = app.layer_for(false);
        let name = dialog
            .text
            .lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(24)
            .collect::<String>();
        let mut shape = Shape::new(name, layer, ShapeKind::Paths { paths });
        shape.transform = Transform::translate(
            dialog.position.x + bounds.width() / 2.0,
            dialog.position.y + bounds.height() / 2.0,
        );
        app.add_shape(shape);
    } else if !open {
        app.text_dialog = None;
    }
}

fn array_dialog(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.array_dialog else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    egui::Window::new(tr("array.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("array-grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("array.columns"));
                ui.add(DragValue::new(&mut dialog.columns).range(1..=100));
                ui.end_row();
                ui.label(tr("array.rows"));
                ui.add(DragValue::new(&mut dialog.rows).range(1..=100));
                ui.end_row();
                ui.label(tr("array.gap_x"));
                ui.add(
                    DragValue::new(&mut dialog.gap_x)
                        .range(-1000.0..=1000.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
                ui.end_row();
                ui.label(tr("array.gap_y"));
                ui.add(
                    DragValue::new(&mut dialog.gap_y)
                        .range(-1000.0..=1000.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
                ui.end_row();
            });
            apply = ui.button(tr("array.apply")).clicked();
        });
    if apply {
        let dialog = app.array_dialog.take().unwrap();
        let bounds = app.document.bounds_of(&app.selection);
        if bounds.is_empty() {
            return;
        }
        app.begin_edit();
        let originals: Vec<Shape> = app
            .document
            .shapes
            .iter()
            .filter(|shape| app.selection.contains(&shape.id))
            .cloned()
            .collect();
        let mut ids: Vec<_> = app.selection.clone();
        for row in 0..dialog.rows {
            for column in 0..dialog.columns {
                if row == 0 && column == 0 {
                    continue;
                }
                let dx = column as f64 * (bounds.width() + dialog.gap_x);
                let dy = -(row as f64) * (bounds.height() + dialog.gap_y);
                for original in &originals {
                    let mut copy = original.clone();
                    copy.id = mml_core::ShapeId::new();
                    copy.transform = copy.transform.then(Transform::translate(dx, dy));
                    ids.push(copy.id);
                    app.document.shapes.push(copy);
                }
            }
        }
        app.selection = ids;
        app.end_edit();
    } else if !open {
        app.array_dialog = None;
    }
}

fn preset_dialog(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.preset_dialog else {
        return;
    };
    let mut open = true;
    let mut save = false;
    egui::Window::new(tr("materials.save_preset"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("preset-grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(tr("materials.material"));
                    ui.text_edit_singleline(&mut dialog.material);
                    ui.end_row();
                    ui.label(tr("materials.thickness"));
                    ui.add(
                        DragValue::new(&mut dialog.thickness)
                            .range(0.0..=200.0)
                            .suffix(" mm")
                            .speed(0.1),
                    );
                    ui.end_row();
                    ui.label(tr("materials.name"));
                    ui.text_edit_singleline(&mut dialog.title);
                    ui.end_row();
                });
            save = ui
                .add_enabled(
                    !dialog.material.trim().is_empty(),
                    egui::Button::new(tr("materials.save")),
                )
                .clicked();
        });
    if save {
        let dialog = app.preset_dialog.take().unwrap();
        if let Some(layer) = app.document.layer(app.current_layer) {
            let thickness = (dialog.thickness > 0.0).then_some(dialog.thickness);
            let preset = mml_materials::Preset::new(
                dialog.material.trim(),
                thickness,
                dialog.title.trim(),
                layer.operation.clone(),
            );
            app.library.presets.push(preset);
            app.save_library();
            app.notify(tr("materials.saved"), Level::Info);
        }
    } else if !open {
        app.preset_dialog = None;
    }
}

fn machine_dialog(app: &mut MyApp, ctx: &egui::Context) {
    let Some(profile) = &mut app.machine_dialog else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    let mut read = false;
    egui::Window::new(tr("machine.title"))
        .open(&mut open)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::Grid::new("machine-grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("machine.name"));
                    ui.text_edit_singleline(&mut profile.name);
                    ui.end_row();
                    ui.label(tr("machine.kind"));
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut profile.kind,
                            MachineKind::Laser,
                            tr("machine.laser"),
                        );
                        ui.selectable_value(&mut profile.kind, MachineKind::Cnc, tr("machine.cnc"));
                    });
                    ui.end_row();
                    ui.label(tr("machine.width"));
                    ui.add(
                        DragValue::new(&mut profile.width)
                            .range(10.0..=5000.0)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                    ui.label(tr("machine.height"));
                    ui.add(
                        DragValue::new(&mut profile.height)
                            .range(10.0..=5000.0)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                    ui.label(tr("machine.z_travel"));
                    ui.add(
                        DragValue::new(&mut profile.z_travel)
                            .range(0.0..=500.0)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                    ui.label(tr("machine.origin"));
                    egui::ComboBox::from_id_salt("origin")
                        .selected_text(origin_label(profile.origin))
                        .show_ui(ui, |ui| {
                            for origin in [
                                Origin::FrontLeft,
                                Origin::FrontRight,
                                Origin::RearLeft,
                                Origin::RearRight,
                            ] {
                                ui.selectable_value(
                                    &mut profile.origin,
                                    origin,
                                    origin_label(origin),
                                );
                            }
                        });
                    ui.end_row();
                    ui.label(tr("machine.max_s"))
                        .on_hover_text(tr("machine.max_s_hint"));
                    ui.add(DragValue::new(&mut profile.max_s).range(1.0..=100_000.0));
                    ui.end_row();
                    ui.label(tr("machine.laser_mode"))
                        .on_hover_text(tr("machine.laser_mode_hint"));
                    ui.checkbox(&mut profile.laser_mode, "");
                    ui.end_row();
                    ui.label(tr("machine.safe_z"));
                    ui.add(
                        DragValue::new(&mut profile.safe_z)
                            .range(0.5..=100.0)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                    ui.label(tr("machine.relative"))
                        .on_hover_text(tr("machine.relative_hint"));
                    ui.checkbox(&mut profile.relative_to_current, "");
                    ui.end_row();
                });
            ui.label(tr("machine.start_gcode"));
            ui.add(
                egui::TextEdit::multiline(&mut profile.start_gcode)
                    .code_editor()
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.label(tr("machine.end_gcode"));
            ui.add(
                egui::TextEdit::multiline(&mut profile.end_gcode)
                    .code_editor()
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                apply = ui.button(tr("dialog.apply")).clicked();
                read = ui
                    .add_enabled(
                        app.device.is_connected(),
                        egui::Button::new(tr("machine.read_controller")),
                    )
                    .on_hover_text(tr("machine.read_controller_hint"))
                    .clicked();
            });
        });
    if read {
        let device = &app.device;
        if let Some(profile) = &mut app.machine_dialog {
            if let Some(width) = device.setting_f64(130) {
                profile.width = width;
            }
            if let Some(height) = device.setting_f64(131) {
                profile.height = height;
            }
            if let Some(z) = device.setting_f64(132) {
                profile.z_travel = z;
            }
            if let Some(max_s) = device.setting_f64(30) {
                profile.max_s = max_s;
            }
            if let Some(laser) = device.setting_f64(32) {
                profile.laser_mode = laser > 0.5;
            }
        }
    }
    if apply {
        app.machine = app.machine_dialog.take().unwrap();
        app.revision += 1;
    } else if !open {
        app.machine_dialog = None;
    }
}

fn origin_label(origin: Origin) -> &'static str {
    match origin {
        Origin::FrontLeft => tr("origin.front_left"),
        Origin::FrontRight => tr("origin.front_right"),
        Origin::RearLeft => tr("origin.rear_left"),
        Origin::RearRight => tr("origin.rear_right"),
    }
}

fn preferences(app: &mut MyApp, ctx: &egui::Context) {
    let mut open = app.settings_open;
    egui::Window::new(tr("menu.preferences"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.checkbox(
                &mut app.settings.laser_speed_per_second,
                tr("prefs.speed_per_second"),
            );
            ui.horizontal(|ui| {
                ui.label(tr("prefs.grid"));
                ui.add(
                    DragValue::new(&mut app.settings.grid)
                        .range(0.1..=50.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
            });
            ui.checkbox(&mut app.settings.snap, tr("view.snap"));
            crate::workflow::advanced_settings(app, ui);
        });
    app.settings_open = open;
}

fn about(app: &mut MyApp, ctx: &egui::Context) {
    let mut open = app.about_open;
    egui::Window::new(tr("menu.about"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("MyMaxmakeLab");
            ui.label(format!(
                "{} {}",
                tr("about.version"),
                env!("CARGO_PKG_VERSION")
            ));
            ui.label(tr("about.text"));
        });
    app.about_open = open;
}

fn close_confirmation(app: &mut MyApp, ctx: &egui::Context) {
    if !app.confirm_close {
        return;
    }
    egui::Modal::new(egui::Id::unique("confirm-close")).show(ctx, |ui| {
        ui.label(RichText::new(tr("close.question")).strong());
        ui.horizontal(|ui| {
            if ui.button(tr("menu.save")).clicked() && app.save(false) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if ui.button(tr("close.discard")).clicked() {
                app.revision = 0;
                app.project_path = None;
                app.document.shapes.clear();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if ui.button(tr("dialog.cancel")).clicked() {
                app.confirm_close = false;
            }
        });
    });
}

fn library_offer(app: &mut MyApp, ctx: &egui::Context) {
    let Some(path) = app.library_offer.first().cloned() else {
        return;
    };
    egui::Modal::new(egui::Id::unique("library-offer")).show(ctx, |ui| {
        ui.label(RichText::new(tr("materials.offer_title")).strong());
        ui.label(tr("materials.offer_text"));
        ui.horizontal(|ui| {
            if ui.button(tr("materials.offer_import")).clicked() {
                app.import_library(&path);
                app.library_offer.clear();
                app.settings.library_import_offered = true;
            }
            if ui.button(tr("dialog.later")).clicked() {
                app.library_offer.clear();
                app.settings.library_import_offered = true;
            }
        });
    });
}
