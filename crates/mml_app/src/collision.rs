//! Collision safety for milling: the real stick-out of the installed
//! tool measured on the tool setter, and the depth allowed at each point
//! before the holder, the shank or the tool hits material, a fixture or
//! the table. The whole program is checked before Start and the depth
//! allowed under the head is shown live.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use crate::machine_panel::JobKind;
use eframe::egui::{self, DragValue, RichText};
use mml_core::geometry::Point;
use mml_gcode::clearance::{Limit, Obstacle, Scene, ToolAssembly, Violation, check};

pub struct CollisionState {
    /// Real stick-out of the installed tool, measured on the setter.
    /// Cleared when the tool may have changed.
    pub stick_out: Option<f64>,
    pub flute_length: f64,
    pub stock_thickness: f64,
    /// Allowed cut below the stock (through cuts into a spoilboard).
    pub through: f64,
    pub margin: f64,
    /// Program checked last and its violations.
    checked: Option<(usize, Vec<Violation>)>,
}

impl Default for CollisionState {
    fn default() -> Self {
        Self {
            stick_out: None,
            flute_length: 12.0,
            stock_thickness: 10.0,
            through: 0.3,
            margin: 1.0,
            checked: None,
        }
    }
}

/// Two-speed touch on the tool setter below the current position.
pub fn setter_program() -> Vec<String> {
    [
        "G91",
        "G38.2 Z-60 F200",
        "G0 Z2",
        "G38.2 Z-4 F40",
        "G0 Z10",
        "G90",
    ]
    .iter()
    .map(|line| (*line).to_owned())
    .collect()
}

fn label(limit: Limit) -> &'static str {
    match limit {
        Limit::Table => tr("collision.table"),
        Limit::Flutes => tr("collision.flutes"),
        Limit::Holder => tr("collision.holder"),
        Limit::Obstacle => tr("collision.obstacle"),
    }
}

/// Largest tool diameter of the milling layers (the stoutest tool is
/// the one most likely to touch walls).
fn tool_diameter(app: &MyApp) -> f64 {
    use mml_core::layer::Operation;
    app.document
        .layers
        .iter()
        .filter(|layer| layer.output && layer.obstacle.is_none())
        .filter_map(|layer| match &layer.operation {
            Operation::CncProfile(params) => Some(params.tool_diameter),
            Operation::CncPocket(params) => Some(params.tool_diameter),
            Operation::CncVCarve(params) => Some(params.tool_diameter),
            _ => None,
        })
        .fold(0.0, f64::max)
        .max(1.0)
}

fn obstacles(app: &MyApp) -> Vec<Obstacle> {
    app.document
        .layers
        .iter()
        .filter_map(|layer| layer.obstacle.map(|top| (layer.id, top)))
        .flat_map(|(layer, top)| {
            app.document
                .shapes
                .iter()
                .filter(move |shape| shape.layer == layer)
                .map(move |shape| Obstacle {
                    outline: shape
                        .world_paths()
                        .into_iter()
                        .filter(|path| path.closed)
                        .map(|path| path.points.iter().map(|point| [point.x, point.y]).collect())
                        .collect(),
                    top,
                })
        })
        .collect()
}

fn assembly(app: &MyApp) -> Option<ToolAssembly> {
    Some(ToolAssembly {
        tool_diameter: tool_diameter(app),
        flute_length: app.collision.flute_length,
        stick_out: app.collision.stick_out?,
        holder_diameter: app.machine.holder_diameter,
    })
}

/// Runs `action` with the scene of the current setup.
fn with_scene<R>(app: &MyApp, action: impl FnOnce(&Scene) -> R) -> R {
    let map = app.probe.map.as_ref();
    let surface = |x: f64, y: f64| {
        map.and_then(|map| map.height(Point::new(x, y)))
            .unwrap_or(0.0)
    };
    let obstacles = obstacles(app);
    let scene = Scene {
        surface: &surface,
        stock_bottom: -app.collision.stock_thickness,
        through: app.collision.through,
        obstacles: &obstacles,
        margin: app.collision.margin,
    };
    action(&scene)
}

/// Violations of the built program, recomputed when it or the setup
/// changes. `None` while the stick-out is not measured.
pub fn violations(app: &mut MyApp) -> Option<Vec<Violation>> {
    let tool = assembly(app)?;
    let built = app.job.built.as_ref()?;
    let key = std::sync::Arc::as_ptr(&built.gcode) as usize
        ^ (tool.stick_out.to_bits() as usize)
        ^ (tool.flute_length.to_bits() as usize).rotate_left(7)
        ^ (app.collision.stock_thickness.to_bits() as usize).rotate_left(13)
        ^ (tool.tool_diameter.to_bits() as usize).rotate_left(19)
        ^ obstacles(app)
            .iter()
            .flat_map(|obstacle| {
                obstacle
                    .outline
                    .iter()
                    .flatten()
                    .map(move |point| point[0] + point[1] * 3.0 + obstacle.top * 7.0)
            })
            .sum::<f64>()
            .to_bits() as usize;
    if app
        .collision
        .checked
        .as_ref()
        .is_none_or(|(cached, _)| *cached != key)
    {
        let program = built.program.clone();
        let found = with_scene(app, |scene| check(&program, &tool, scene, 20));
        app.collision.checked = Some((key, found));
    }
    app.collision
        .checked
        .as_ref()
        .map(|(_, found)| found.clone())
}

/// Keeps the check of the built program up to date (called every
/// frame; recomputes only when the program or the setup changed).
pub fn refresh(app: &mut MyApp) {
    if app.machine.kind == mml_core::machine::MachineKind::Cnc {
        let _ = violations(app);
    }
}

/// Start conditions from collisions, from the last check.
pub fn gates(app: &MyApp) -> Vec<String> {
    if app.collision.stick_out.is_none() {
        return vec![tr("gate.measure_tool").to_owned()];
    }
    match app.collision.checked.as_ref() {
        Some((_, found)) if !found.is_empty() => {
            let first = found[0];
            vec![trf(
                "gate.collision",
                &[
                    &found.len(),
                    &(first.line + 1),
                    &format!("{:.1}", first.excess),
                    &label(first.limit),
                ],
            )]
        }
        _ => Vec::new(),
    }
}

pub fn card(app: &mut MyApp, ui: &mut egui::Ui) {
    crate::panels::section(ui, tr("collision.title"));
    ui.label(RichText::new(tr("collision.intro")).weak());
    let idle = app.device.is_connected() && app.device.state() == mml_device::MachineState::Idle;
    let calibrated = app.machine.tool_setter.reference_z.is_some();
    egui::Grid::new("collision-grid")
        .num_columns(2)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            ui.label(tr("collision.stick_out"));
            match app.collision.stick_out {
                Some(length) => ui.label(
                    RichText::new(trf("collision.measured", &[&format!("{length:.2}")]))
                        .color(crate::theme::SUCCESS),
                ),
                None => ui.label(
                    RichText::new(tr("collision.not_measured")).color(crate::theme::WARNING),
                ),
            };
            ui.end_row();
            ui.label(tr("collision.flute_length"))
                .on_hover_text(tr("collision.flute_hint"));
            ui.add(
                DragValue::new(&mut app.collision.flute_length)
                    .range(0.5..=150.0)
                    .suffix(" mm")
                    .speed(0.1),
            );
            ui.end_row();
            ui.label(tr("collision.holder_diameter"))
                .on_hover_text(tr("collision.holder_hint"));
            ui.add(
                DragValue::new(&mut app.machine.holder_diameter)
                    .range(3.0..=120.0)
                    .suffix(" mm")
                    .speed(0.1),
            );
            ui.end_row();
            ui.label(tr("collision.stock"));
            ui.add(
                DragValue::new(&mut app.collision.stock_thickness)
                    .range(0.1..=300.0)
                    .suffix(" mm")
                    .speed(0.1),
            );
            ui.end_row();
            ui.label(tr("collision.through"))
                .on_hover_text(tr("collision.through_hint"));
            ui.add(
                DragValue::new(&mut app.collision.through)
                    .range(0.0..=5.0)
                    .suffix(" mm")
                    .speed(0.05),
            );
            ui.end_row();
        });
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                idle && calibrated,
                egui::Button::new(RichText::new(tr("collision.measure")).strong()),
            )
            .on_hover_text(tr("collision.measure_hint"))
            .clicked()
        {
            app.device.probe_reports.clear();
            crate::workflow::run_auxiliary(app, setter_program(), JobKind::ToolSetter);
        }
        ui.menu_button(tr("collision.calibrate"), |ui| {
            ui.label(RichText::new(tr("collision.calibrate_hint")).weak());
            ui.horizontal(|ui| {
                ui.label(tr("collision.gauge"));
                ui.add(
                    DragValue::new(&mut app.machine.tool_setter.gauge_length)
                        .range(5.0..=150.0)
                        .suffix(" mm")
                        .speed(0.01),
                );
            });
            if ui
                .add_enabled(idle, egui::Button::new(tr("collision.probe_gauge")))
                .clicked()
            {
                app.device.probe_reports.clear();
                crate::workflow::run_auxiliary(app, setter_program(), JobKind::SetterReference);
                ui.close();
            }
        });
    });
    if !calibrated {
        ui.label(RichText::new(tr("collision.calibrate_first")).color(crate::theme::WARNING));
    }
    // Depth allowed under the head, live.
    if let (Some(tool), Some(status)) = (assembly(app), &app.device.status) {
        let [x, y, _] = status.work_position;
        let (depth, limit) = with_scene(app, |scene| scene.allowed_depth(&tool, x, y));
        ui.label(trf(
            "collision.here",
            &[&format!("{depth:.1}"), &label(limit)],
        ));
    }
    if let Some(found) = violations(app) {
        if found.is_empty() {
            ui.label(RichText::new(tr("collision.clear")).color(crate::theme::SUCCESS));
        } else {
            for violation in found.iter().take(4) {
                ui.label(
                    RichText::new(trf(
                        "collision.violation",
                        &[
                            &(violation.line + 1),
                            &format!("{:.1}", violation.at[0]),
                            &format!("{:.1}", violation.at[1]),
                            &format!("{:.1}", violation.excess),
                            &label(violation.limit),
                        ],
                    ))
                    .color(crate::theme::DANGER),
                );
            }
        }
    }
}

/// End of a tool setter touch: calibration or measure.
pub fn setter_finished(app: &mut MyApp, kind: JobKind, completed: bool) {
    let touch = app
        .device
        .probe_reports
        .iter()
        .rev()
        .find(|(_, touched)| *touched)
        .map(|(position, _)| position[2]);
    app.device.probe_reports.clear();
    let Some(touch_z) = touch.filter(|_| completed) else {
        app.notify(tr("collision.probe_failed"), Level::Error);
        return;
    };
    if kind == JobKind::SetterReference {
        app.machine.tool_setter.reference_z = Some(touch_z);
        app.settings.machine = app.machine.clone();
        app.notify(tr("collision.calibrated"), Level::Info);
    } else if let Some(length) = app.machine.tool_setter.stick_out(touch_z) {
        app.collision.stick_out = Some(length);
        app.notify(
            trf("collision.measured", &[&format!("{length:.2}")]),
            Level::Info,
        );
    }
}
