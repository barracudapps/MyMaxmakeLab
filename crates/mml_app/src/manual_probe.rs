//! Manual measurement card: the operator jogs until the tool touches,
//! records the position, and the stock origin and size follow.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_core::geometry::{Point, Rect};
use mml_device::MachineState;
use mml_probe::manual::{self, Origin, SolveError, StockShape, Touch};

pub struct ManualState {
    pub shape: StockShape,
    pub origin: Origin,
    /// Recorded machine positions of the tool tip.
    pub touches: Vec<(Touch, [f64; 3])>,
}

impl Default for ManualState {
    fn default() -> Self {
        Self {
            shape: StockShape::Block,
            origin: Origin::FrontLeft,
            touches: Vec::new(),
        }
    }
}

fn touch_label(touch: Touch) -> String {
    match touch.index() {
        Some(index) => format!("{} {index}", tr(touch.key())),
        None => tr(touch.key()).to_owned(),
    }
}

pub fn section(app: &mut MyApp, ui: &mut egui::Ui) {
    let open = !app.settings.has_probe;
    egui::CollapsingHeader::new(RichText::new(tr("manual.title")).strong())
        .id_salt("manual-probe")
        .default_open(open)
        .show(ui, |ui| body(app, ui));
}

fn body(app: &mut MyApp, ui: &mut egui::Ui) {
    ui.label(RichText::new(tr("manual.intro")).weak());
    ui.horizontal_wrapped(|ui| {
        for shape in StockShape::ALL {
            if ui
                .selectable_label(app.manual.shape == shape, tr(shape.key()))
                .clicked()
                && app.manual.shape != shape
            {
                app.manual.shape = shape;
                app.manual.origin = shape.default_origin();
                app.manual.touches.clear();
            }
        }
    });
    egui::Grid::new("manual-contact")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr("manual.diameter"))
                .on_hover_text(tr("manual.diameter_hint"));
            ui.add(
                DragValue::new(&mut app.settings.manual_contact.diameter)
                    .range(0.0..=30.0)
                    .suffix(" mm")
                    .speed(0.01),
            );
            ui.end_row();
            ui.label(tr("manual.shim"))
                .on_hover_text(tr("manual.shim_hint"));
            ui.add(
                DragValue::new(&mut app.settings.manual_contact.shim)
                    .range(0.0..=5.0)
                    .suffix(" mm")
                    .speed(0.01),
            );
            ui.end_row();
        });
    ui.label(RichText::new(tr("manual.tip")).weak().small());
    ui.add_space(4.0);

    let position = app
        .device
        .status
        .as_ref()
        .filter(|_| app.device.is_connected())
        .map(|status| status.machine_position);
    let steps = app.manual.shape.steps();
    let next = steps
        .iter()
        .find(|step| !app.manual.touches.iter().any(|(t, _)| *t == step.touch))
        .map(|step| step.touch);
    egui::Grid::new("manual-steps")
        .num_columns(3)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for (number, step) in steps.iter().enumerate() {
                let mut text = format!("{}. {}", number + 1, touch_label(step.touch));
                if step.optional {
                    text.push_str(&format!(" ({})", tr("manual.optional")));
                }
                let mut label = RichText::new(text);
                if next == Some(step.touch) {
                    label = label.strong().color(crate::theme::ACCENT);
                }
                ui.label(label);
                let recorded = app
                    .manual
                    .touches
                    .iter()
                    .position(|(t, _)| *t == step.touch);
                match recorded {
                    Some(index) => {
                        ui.horizontal(|ui| {
                            for (axis, value) in
                                ["X", "Y", "Z"].iter().zip(&mut app.manual.touches[index].1)
                            {
                                ui.add(
                                    DragValue::new(value)
                                        .prefix(format!("{axis} "))
                                        .speed(0.01)
                                        .max_decimals(3),
                                );
                            }
                        });
                        if ui.small_button(tr("manual.clear_one")).clicked() {
                            app.manual.touches.remove(index);
                        }
                    }
                    None => {
                        ui.horizontal(|ui| {
                            let record = egui::Button::new(tr("manual.record"));
                            if ui
                                .add_enabled(position.is_some(), record)
                                .on_hover_text(tr("manual.record_hint"))
                                .clicked()
                                && let Some(position) = position
                            {
                                app.manual.touches.push((step.touch, position));
                            }
                            if ui
                                .small_button(tr("manual.type"))
                                .on_hover_text(tr("manual.type_hint"))
                                .clicked()
                            {
                                app.manual
                                    .touches
                                    .push((step.touch, position.unwrap_or([0.0; 3])));
                            }
                        });
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });

    let result = manual::solve(
        app.manual.shape,
        &app.manual.touches,
        app.settings.manual_contact,
    );
    let measure = match result {
        Ok(measure) => measure,
        Err(SolveError::Missing(touch)) => {
            ui.label(RichText::new(trf("manual.missing", &[&touch_label(touch)])).weak());
            return;
        }
        Err(SolveError::Inconsistent) => {
            ui.label(RichText::new(tr("manual.inconsistent")).color(crate::theme::WARNING));
            return;
        }
    };
    summary(ui, &measure);
    if measure.locates_xy() {
        ui.horizontal(|ui| {
            ui.label(tr("manual.origin"));
            ui.radio_value(
                &mut app.manual.origin,
                Origin::FrontLeft,
                tr("manual.origin_corner"),
            );
            let center_known = measure.origin(Origin::Center).is_some();
            ui.add_enabled_ui(center_known, |ui| {
                ui.radio_value(
                    &mut app.manual.origin,
                    Origin::Center,
                    tr("manual.origin_center"),
                );
            });
        });
    }
    let idle = app.device.is_connected()
        && app.device.state() == MachineState::Idle
        && !app.device.job_running();
    let program = measure.origin_program(app.manual.origin);
    ui.horizontal_wrapped(|ui| {
        let apply = egui::Button::new(RichText::new(tr("manual.apply")).strong());
        if ui
            .add_enabled(idle && program.is_some(), apply)
            .on_hover_text(tr("manual.apply_hint"))
            .clicked()
            && let Some(lines) = program
        {
            apply_origin(app, &measure, lines);
        }
        if ui.button(tr("manual.reset")).clicked() {
            app.manual.touches.clear();
        }
    });
}

fn summary(ui: &mut egui::Ui, measure: &manual::Measure) {
    let mm = |value: f64| format!("{value:.2}");
    if let Some(diameter) = measure.diameter {
        ui.label(trf("manual.result_diameter", &[&mm(diameter)]));
    }
    match (measure.width(), measure.depth()) {
        (Some(width), Some(depth)) if measure.diameter.is_none() => {
            ui.label(trf("manual.result_size", &[&mm(width), &mm(depth)]));
        }
        _ => {}
    }
    if let Some(height) = measure.height() {
        ui.label(trf("manual.result_height", &[&mm(height)]));
    }
    if let (Some(width), Some(angle)) = (measure.top_width, measure.side_angle) {
        ui.label(trf(
            "manual.result_trapezoid",
            &[&mm(width), &format!("{angle:.1}")],
        ));
    }
    if let Some((plane, residual)) = measure.plane {
        ui.label(trf(
            "probe.plane_summary",
            &[&format!("{:.2}", plane.tilt_degrees()), &mm(residual)],
        ));
    }
    if let Some(skew) = measure.skew {
        let text = trf("manual.result_skew", &[&format!("{skew:.2}")]);
        if skew.abs() > 0.2 {
            ui.label(RichText::new(text).color(crate::theme::WARNING));
            ui.label(RichText::new(tr("manual.skew_warning")).color(crate::theme::WARNING));
        } else {
            ui.label(text);
        }
    }
}

fn apply_origin(app: &mut MyApp, measure: &manual::Measure, lines: Vec<String>) {
    // Work offset after the change, to express a tilted top in work
    // coordinates: only Z moves when the sides were not touched.
    let previous = app
        .device
        .status
        .as_ref()
        .map_or([0.0; 3], |status| status.work_offset);
    let offset = match measure.origin(app.manual.origin) {
        Some(origin) if measure.locates_xy() => origin,
        _ => [previous[0], previous[1], measure.top],
    };
    for line in lines {
        app.device.line(line);
    }
    app.device.z_probed = true;
    if let Some(height) = measure.height() {
        app.collision.stock_thickness = height;
    }
    if let Some((plane, residual)) = measure.plane {
        let work = mml_probe::Plane {
            a: plane.a,
            b: plane.b,
            c: plane.a * offset[0] + plane.b * offset[1] + plane.c - offset[2],
        };
        let points: Vec<Point> = app
            .manual
            .touches
            .iter()
            .map(|(_, p)| Point::new(p[0] - offset[0], p[1] - offset[1]))
            .collect();
        let area = app
            .job
            .built
            .as_ref()
            .map(|built| built.stats.cut_bounds)
            .filter(|area| !area.is_empty())
            .unwrap_or_else(|| {
                let mut area = Rect::EMPTY;
                points.iter().for_each(|point| area.include(*point));
                area
            });
        app.probe.map = Some(work.to_map(area));
        app.probe.plane = Some((work, residual));
    }
    app.notify(tr("manual.applied"), Level::Info);
}
