//! "3D model → CNC" assistant: place an STL / OBJ in the stock and get
//! the relief program, previewed like any other job.

use crate::app::{BuiltJob, Level, Mode, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_relief::{ReliefParams, ToolShape, Triangle};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

pub struct ReliefDialog {
    pub name: String,
    pub triangles: Arc<Vec<Triangle>>,
    pub params: ReliefParams,
    pub computing: Option<Receiver<String>>,
    /// Wraps the relief around a cylinder on an A axis (diameter, mm).
    pub rotary_diameter: Option<f64>,
}

impl ReliefDialog {
    pub fn open(name: String, triangles: Vec<Triangle>, bed: (f64, f64)) -> Self {
        let (low, high) = mml_relief::bounds(&triangles);
        let width = ((high[0] - low[0]) as f64).min(bed.0 * 0.8).max(10.0);
        let params = ReliefParams {
            width,
            ..ReliefParams::default()
        };
        Self {
            name,
            triangles: Arc::new(triangles),
            params,
            computing: None,
            rotary_diameter: None,
        }
    }

    fn model_size(&self) -> (f64, f64) {
        let (low, high) = mml_relief::bounds(&self.triangles);
        let ratio = ((high[1] - low[1]) / (high[0] - low[0]).max(1e-6)) as f64;
        (self.params.width, self.params.width * ratio)
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.relief else {
        return;
    };
    if let Some(receiver) = &dialog.computing
        && let Ok(gcode) = receiver.try_recv()
    {
        let name = dialog.name.clone();
        app.relief = None;
        let program = mml_gcode::parse(&gcode);
        let stats = mml_gcode::analyze(&program, app.motion_settings());
        app.job.built = Some(BuiltJob {
            gcode: Arc::new(gcode),
            program,
            stats,
            warnings: Vec::new(),
            external: Some(name),
            pauses: Vec::new(),
            anchor: None,
        });
        app.job.preview_line = Some(None);
        app.mode = Mode::Prepare;
        app.canvas.request_fit();
        app.notify(tr("relief.ready"), Level::Info);
        return;
    }
    let mut open = true;
    let mut generate = false;
    let (width, height) = dialog.model_size();
    egui::Window::new(trf("relief.title", &[&dialog.name]))
        .open(&mut open)
        .collapsible(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            let params = &mut dialog.params;
            ui.label(
                RichText::new(trf(
                    "relief.size",
                    &[
                        &format!("{width:.1}"),
                        &format!("{height:.1}"),
                        &format!("{:.1}", params.depth),
                    ],
                ))
                .weak(),
            );
            ui.horizontal(|ui| {
                ui.label(tr("relief.orientation"));
                for (axis, label) in [(0, "X 90°"), (1, "Y 90°"), (2, "Z 90°")] {
                    if ui
                        .button(label)
                        .on_hover_text(tr("relief.orientation_hint"))
                        .clicked()
                    {
                        let mut triangles = (*dialog.triangles).clone();
                        mml_relief::rotate_quarter(&mut triangles, axis);
                        dialog.triangles = Arc::new(triangles);
                    }
                }
            });
            egui::Grid::new("relief-grid")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    ui.label(tr("relief.width"));
                    ui.add(
                        DragValue::new(&mut params.width)
                            .range(1.0..=2000.0)
                            .suffix(" mm")
                            .speed(0.5),
                    );
                    ui.end_row();
                    ui.label(tr("relief.depth"));
                    ui.add(
                        DragValue::new(&mut params.depth)
                            .range(0.1..=100.0)
                            .suffix(" mm")
                            .speed(0.1),
                    );
                    ui.end_row();
                    ui.label(tr("relief.origin"));
                    ui.horizontal(|ui| {
                        ui.add(
                            DragValue::new(&mut params.origin[0])
                                .prefix("X ")
                                .suffix(" mm")
                                .speed(0.5),
                        );
                        ui.add(
                            DragValue::new(&mut params.origin[1])
                                .prefix("Y ")
                                .suffix(" mm")
                                .speed(0.5),
                        );
                    });
                    ui.end_row();
                    ui.label(tr("relief.background"));
                    ui.checkbox(&mut params.background_floor, "")
                        .on_hover_text(tr("relief.background_hint"));
                    ui.end_row();
                    ui.label(tr("relief.tool"));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut params.tool, ToolShape::Ball, tr("relief.ball"));
                        ui.selectable_value(&mut params.tool, ToolShape::Flat, tr("relief.flat"));
                        ui.add(
                            DragValue::new(&mut params.tool_diameter)
                                .range(0.1..=25.0)
                                .suffix(" mm")
                                .speed(0.01),
                        );
                    });
                    ui.end_row();
                    ui.label(tr("op.stepover"));
                    let mut stepover = params.stepover * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut stepover, 3.0..=60.0).suffix(" %"))
                        .changed()
                    {
                        params.stepover = stepover / 100.0;
                    }
                    ui.end_row();
                    ui.label(tr("op.pass_depth"));
                    ui.add(
                        DragValue::new(&mut params.pass_depth)
                            .range(0.05..=20.0)
                            .suffix(" mm")
                            .speed(0.05),
                    );
                    ui.end_row();
                    ui.label(tr("relief.allowance"));
                    ui.add(
                        DragValue::new(&mut params.allowance)
                            .range(0.0..=3.0)
                            .suffix(" mm")
                            .speed(0.01),
                    );
                    ui.end_row();
                    ui.label(tr("relief.passes"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut params.roughing, tr("relief.roughing"));
                        ui.checkbox(&mut params.finishing, tr("relief.finishing"));
                    });
                    ui.end_row();
                    ui.label(tr("op.feed"));
                    ui.add(
                        DragValue::new(&mut params.feed)
                            .range(10.0..=20_000.0)
                            .suffix(" mm/min")
                            .speed(10.0),
                    );
                    ui.end_row();
                    ui.label(tr("op.plunge"));
                    ui.add(
                        DragValue::new(&mut params.plunge)
                            .range(10.0..=5_000.0)
                            .suffix(" mm/min")
                            .speed(5.0),
                    );
                    ui.end_row();
                    ui.label(tr("op.spindle"));
                    ui.add(
                        DragValue::new(&mut params.spindle)
                            .range(0.0..=60_000.0)
                            .suffix(" rpm")
                            .speed(100.0),
                    );
                    ui.end_row();
                });
            let mut wrap = dialog.rotary_diameter.is_some();
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut wrap, tr("relief.rotary"))
                    .on_hover_text(tr("relief.rotary_hint"))
                    .changed()
                {
                    dialog.rotary_diameter = wrap.then_some(50.0);
                }
                if let Some(diameter) = &mut dialog.rotary_diameter {
                    ui.add(
                        DragValue::new(diameter)
                            .range(5.0..=400.0)
                            .prefix("Ø ")
                            .suffix(" mm")
                            .speed(0.5),
                    );
                    if ui
                        .small_button(tr("relief.one_turn"))
                        .on_hover_text(tr("relief.one_turn_hint"))
                        .clicked()
                    {
                        let ratio = height / width.max(1e-6);
                        dialog.params.width = std::f64::consts::PI * *diameter / ratio.max(1e-6);
                        dialog.params.origin[1] = 0.0;
                    }
                }
            });
            ui.label(RichText::new(tr("relief.hint")).weak().small());
            if dialog.computing.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("relief.computing"));
                });
            } else {
                generate = ui
                    .button(RichText::new(tr("relief.generate")).strong())
                    .clicked();
            }
        });
    if generate && let Some(dialog) = &mut app.relief {
        let (sender, receiver) = channel();
        let triangles = dialog.triangles.clone();
        let params = dialog.params;
        let rotary = dialog.rotary_diameter;
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let gcode = mml_relief::relief_gcode(&triangles, &params);
            let gcode = match rotary {
                // Y of the unrolled relief becomes the A angle; Z0 is the
                // top of the cylinder, which must sit on the A axis line.
                Some(diameter) => mml_job::wrap_rotary(&gcode, mml_job::Rotary::AxisA { diameter }),
                None => gcode,
            };
            let _ = sender.send(gcode);
            repaint.request_repaint();
        });
        dialog.computing = Some(receiver);
    }
    if !open {
        app.relief = None;
    }
}
