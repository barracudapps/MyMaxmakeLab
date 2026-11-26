//! "3D model → CNC" assistant: place an STL / OBJ in the stock and get
//! the relief program, previewed like any other job.

use crate::app::{BuiltJob, Level, Mode, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_relief::{
    IndexedParams, MoldParams, PositionalParams, ReliefParams, Side, ToolShape, Triangle,
};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

pub struct ReliefDialog {
    pub name: String,
    pub triangles: Arc<Vec<Triangle>>,
    pub params: ReliefParams,
    pub computing: Option<Receiver<Result<String, String>>>,
    /// Wraps the relief around a cylinder on an A axis (diameter, mm).
    pub rotary_diameter: Option<f64>,
    /// Machines the whole part from several angles on an A axis.
    pub indexed: Option<IndexedParams>,
    /// Positional 5-axis on the trunnion of the machine.
    pub positional: Option<PositionalParams>,
    /// Imprint of the model in the stock instead of a relief.
    pub mold: Option<MoldParams>,
    /// Sheet thickness for stacked slices.
    pub slice_thickness: f64,
    /// Poses ranked by the orientation guide, best first.
    pub poses: Vec<mml_relief::Pose>,
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
            indexed: None,
            positional: None,
            mold: None,
            slice_thickness: 3.0,
            poses: Vec::new(),
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
        && let Ok(result) = receiver.try_recv()
    {
        let gcode = match result {
            Ok(gcode) => gcode,
            Err(error) => {
                dialog.computing = None;
                app.notify(error, Level::Error);
                return;
            }
        };
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
            anchor_z: false,
        });
        app.job.preview_line = Some(None);
        app.mode = Mode::Prepare;
        app.canvas.request_fit();
        app.notify(tr("relief.ready"), Level::Info);
        return;
    }
    let mut open = true;
    let mut generate = false;
    let mut make_slices = false;
    let (width, height) = dialog.model_size();
    let trunnion = app.machine.trunnion;
    let has_trunnion = trunnion.is_some();
    if !has_trunnion {
        dialog.positional = None;
    }
    egui::Window::new(trf("relief.title", &[&dialog.name]))
        .open(&mut open)
        .collapsible(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            let molding = dialog.mold.is_some();
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
                        dialog.poses.clear();
                    }
                }
                if ui
                    .button(tr("relief.suggest"))
                    .on_hover_text(tr("relief.suggest_hint"))
                    .clicked()
                {
                    dialog.poses = mml_relief::rank_poses(&dialog.triangles);
                }
            });
            let mut chosen = None;
            for (rank, pose) in dialog.poses.iter().take(3).enumerate() {
                ui.horizontal(|ui| {
                    let text = trf(
                        "relief.pose",
                        &[
                            &(rank + 1),
                            &format!("{:.0}", pose.reachable * 100.0),
                            &format!("{:.0}", pose.height),
                            &format!("{:.0}", pose.base * 100.0),
                        ],
                    );
                    let label = if rank == 0 {
                        RichText::new(text).color(crate::theme::SUCCESS)
                    } else {
                        RichText::new(text).weak()
                    };
                    ui.label(label);
                    if ui.small_button(tr("relief.apply_pose")).clicked() {
                        chosen = Some(pose.turns);
                    }
                });
            }
            if let Some(turns) = chosen {
                let mut triangles = (*dialog.triangles).clone();
                mml_relief::apply_pose(&mut triangles, turns);
                dialog.triangles = Arc::new(triangles);
                dialog.poses.clear();
            }
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
                    // A mold keeps the real proportions of the model.
                    if !molding {
                        ui.label(tr("relief.depth"));
                        ui.add(
                            DragValue::new(&mut params.depth)
                                .range(0.1..=100.0)
                                .suffix(" mm")
                                .speed(0.1),
                        );
                        ui.end_row();
                    }
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
            ui.horizontal(|ui| {
                ui.label(tr("relief.axes"));
                let flat = dialog.rotary_diameter.is_none()
                    && dialog.indexed.is_none()
                    && dialog.positional.is_none();
                if ui.selectable_label(flat, tr("relief.flat_stock")).clicked() {
                    dialog.rotary_diameter = None;
                    dialog.indexed = None;
                    dialog.positional = None;
                }
                if ui
                    .selectable_label(dialog.rotary_diameter.is_some(), tr("relief.rotary"))
                    .on_hover_text(tr("relief.rotary_hint"))
                    .clicked()
                {
                    dialog.rotary_diameter = Some(50.0);
                    dialog.indexed = None;
                    dialog.positional = None;
                }
                if ui
                    .selectable_label(dialog.indexed.is_some(), tr("relief.indexed"))
                    .on_hover_text(tr("relief.indexed_hint"))
                    .clicked()
                {
                    dialog.indexed = Some(IndexedParams::default());
                    dialog.rotary_diameter = None;
                    dialog.positional = None;
                    dialog.params.origin[1] = 0.0;
                }
                if has_trunnion
                    && ui
                        .selectable_label(dialog.positional.is_some(), tr("relief.positional"))
                        .on_hover_text(tr("relief.positional_hint"))
                        .clicked()
                {
                    dialog.positional = Some(PositionalParams::default());
                    dialog.rotary_diameter = None;
                    dialog.indexed = None;
                }
            });
            let flat = dialog.rotary_diameter.is_none()
                && dialog.indexed.is_none()
                && dialog.positional.is_none();
            if !flat {
                dialog.mold = None;
            }
            if flat {
                let mut mold = dialog.mold.is_some();
                if ui
                    .checkbox(&mut mold, RichText::new(tr("relief.mold")).strong())
                    .on_hover_text(tr("relief.mold_hint"))
                    .changed()
                {
                    dialog.mold = mold.then(|| MoldParams {
                        sink: (height.min(width) / 2.0).max(1.0),
                        ..MoldParams::default()
                    });
                }
            }
            if let Some(mold) = &mut dialog.mold {
                egui::Grid::new("mold-grid")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label(tr("relief.mold_sink"))
                            .on_hover_text(tr("relief.mold_sink_hint"));
                        ui.add(
                            DragValue::new(&mut mold.sink)
                                .range(0.1..=500.0)
                                .suffix(" mm")
                                .speed(0.1),
                        );
                        ui.end_row();
                        ui.label(tr("relief.mold_offset"))
                            .on_hover_text(tr("relief.mold_offset_hint"));
                        ui.add(
                            DragValue::new(&mut mold.offset)
                                .range(0.0..=20.0)
                                .suffix(" mm")
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label(tr("relief.mold_islands"))
                            .on_hover_text(tr("relief.mold_islands_hint"));
                        ui.horizontal(|ui| {
                            let mut remove = mold.min_island > 0.0;
                            if ui.checkbox(&mut remove, "").changed() {
                                mold.min_island = if remove { 20.0 } else { 0.0 };
                            }
                            if remove {
                                ui.add(
                                    DragValue::new(&mut mold.min_island)
                                        .range(0.5..=10_000.0)
                                        .prefix("< ")
                                        .suffix(" mm²")
                                        .speed(0.5),
                                );
                            }
                        });
                        ui.end_row();
                        ui.label(tr("relief.mold_mirror"))
                            .on_hover_text(tr("relief.mold_mirror_hint"));
                        ui.checkbox(&mut mold.mirror, "");
                        ui.end_row();
                    });
            }
            if let Some(positional) = &mut dialog.positional {
                ui.horizontal(|ui| {
                    ui.label(tr("relief.stock"));
                    for (axis, prefix) in ["X ", "Y ", "Z "].iter().enumerate() {
                        ui.add(
                            DragValue::new(&mut positional.stock[axis])
                                .range(1.0..=1000.0)
                                .prefix(*prefix)
                                .suffix(" mm")
                                .speed(0.5),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("relief.sides"));
                    for side in Side::ALL {
                        let mut on = positional.sides.contains(&side);
                        if ui.checkbox(&mut on, tr(side_key(side))).changed() {
                            positional.sides.retain(|known| *known != side);
                            if on {
                                positional.sides.push(side);
                                positional.sides.sort_by_key(|side| {
                                    Side::ALL.iter().position(|known| known == side)
                                });
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("relief.tilt"))
                        .on_hover_text(tr("relief.tilt_hint"));
                    ui.add(
                        DragValue::new(&mut positional.tilt)
                            .range(10.0..=90.0)
                            .suffix("°"),
                    );
                });
                ui.label(RichText::new(tr("relief.positional_setup")).weak().small());
            }
            if let Some(diameter) = &mut dialog.rotary_diameter {
                ui.horizontal(|ui| {
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
                });
            }
            if let Some(indexed) = &mut dialog.indexed {
                ui.horizontal(|ui| {
                    ui.label(tr("relief.stock"));
                    ui.add(
                        DragValue::new(&mut indexed.stock_diameter)
                            .range(5.0..=400.0)
                            .prefix("Ø ")
                            .suffix(" mm")
                            .speed(0.5),
                    );
                    ui.label(tr("relief.faces"));
                    for faces in [2, 3, 4, 6, 8] {
                        ui.selectable_value(&mut indexed.faces, faces, faces.to_string());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("relief.stem"))
                        .on_hover_text(tr("relief.stem_hint"));
                    ui.add(
                        DragValue::new(&mut indexed.stem_diameter)
                            .range(0.0..=indexed.stock_diameter)
                            .prefix("Ø ")
                            .suffix(" mm")
                            .speed(0.1),
                    );
                });
                ui.checkbox(&mut indexed.continuous_finish, tr("relief.continuous"))
                    .on_hover_text(tr("relief.continuous_hint"));
                ui.label(RichText::new(tr("relief.indexed_setup")).weak().small());
                if !dialog.params.roughing {
                    ui.label(
                        RichText::new(tr("relief.indexed_rough")).color(crate::theme::WARNING),
                    );
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("relief.slices"))
                    .on_hover_text(tr("relief.slices_hint"));
                ui.add(
                    DragValue::new(&mut dialog.slice_thickness)
                        .range(0.5..=30.0)
                        .suffix(" mm")
                        .speed(0.1),
                );
                make_slices = ui.button(tr("relief.make_slices")).clicked();
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
    if make_slices && let Some(dialog) = app.relief.take() {
        create_slices(app, &dialog);
        return;
    }
    if generate && let Some(dialog) = &mut app.relief {
        let (sender, receiver) = channel();
        let triangles = dialog.triangles.clone();
        let params = dialog.params;
        let rotary = dialog.rotary_diameter;
        let indexed = dialog.indexed;
        let positional = dialog.positional.clone();
        let mold = dialog.mold;
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            if let Some(mold) = mold {
                let _ = sender.send(Ok(mml_relief::mold_gcode(&triangles, &params, &mold)));
                repaint.request_repaint();
                return;
            }
            if let (Some(positional), Some(trunnion)) = (positional, trunnion) {
                let _ = sender.send(
                    mml_relief::positional_gcode(&triangles, &params, &positional, &trunnion)
                        .map_err(|error| error.to_string()),
                );
                repaint.request_repaint();
                return;
            }
            if let Some(indexed) = indexed {
                let _ = sender.send(
                    mml_relief::indexed_gcode(&triangles, &params, &indexed)
                        .map_err(|error| error.to_string()),
                );
                repaint.request_repaint();
                return;
            }
            let gcode = mml_relief::relief_gcode(&triangles, &params);
            let gcode = match rotary {
                // Y of the unrolled relief becomes the A angle; Z0 is the
                // top of the cylinder, which must sit on the A axis line.
                Some(diameter) => mml_job::wrap_rotary(&gcode, mml_job::Rotary::AxisA { diameter }),
                None => gcode,
            };
            let _ = sender.send(Ok(gcode));
            repaint.request_repaint();
        });
        dialog.computing = Some(receiver);
    }
    if !open {
        app.relief = None;
    }
}

fn side_key(side: Side) -> &'static str {
    match side {
        Side::Top => "relief.side_top",
        Side::Front => "relief.side_front",
        Side::Back => "relief.side_back",
        Side::Left => "relief.side_left",
        Side::Right => "relief.side_right",
    }
}

/// Lays the stacked slices of the model out flat, numbered in the order
/// they are glued, the numbers engraved on their own layer.
fn create_slices(app: &mut MyApp, dialog: &ReliefDialog) {
    use mml_core::document::{Shape, ShapeKind};
    use mml_core::geometry::{Path, Point};
    use mml_core::layer::{Layer, LineParams, Operation};
    let layers = mml_relief::slice_mesh(
        &dialog.triangles,
        dialog.params.width,
        dialog.slice_thickness,
    );
    let parts: Vec<Vec<Path>> = layers
        .into_iter()
        .map(|loops| {
            loops
                .into_iter()
                .map(|points| {
                    Path::new(
                        points.into_iter().map(|[x, y]| Point::new(x, y)).collect(),
                        true,
                    )
                })
                .collect::<Vec<_>>()
        })
        .filter(|paths| !paths.is_empty())
        .collect();
    if parts.is_empty() {
        app.notify(tr("relief.no_slices"), Level::Error);
        return;
    }
    let bounds: Vec<_> = parts
        .iter()
        .map(|paths| {
            paths
                .iter()
                .fold(mml_core::geometry::Rect::EMPTY, |rect, path| {
                    rect.union(path.bounds())
                })
        })
        .collect();
    let sizes: Vec<(f64, f64)> = bounds
        .iter()
        .map(|rect| (rect.width(), rect.height()))
        .collect();
    let (corners, _) = mml_core::attach::shelf_pack(&sizes, app.machine.width.max(100.0), 3.0);
    app.begin_edit("history.add");
    let cut_layer = app.layer_for(false);
    let labels = Layer::new(
        tr("relief.slice_labels"),
        app.document.next_layer_color(),
        Operation::LaserLine(LineParams {
            power: 20.0,
            speed: 1500.0,
            ..LineParams::default()
        }),
    );
    let label_layer = labels.id;
    // Numbers are engraved before the parts are cut free.
    let position = app
        .document
        .layers
        .iter()
        .position(|layer| layer.id == cut_layer)
        .unwrap_or(0);
    app.document.layers.insert(position, labels);
    let family = mml_io::hershey_families()
        .into_iter()
        .next()
        .unwrap_or_default();
    let count = parts.len();
    app.selection.clear();
    for (index, ((mut paths, rect), corner)) in
        parts.into_iter().zip(&bounds).zip(&corners).enumerate()
    {
        let (dx, dy) = (corner.x - rect.min.x, corner.y - rect.min.y);
        for point in paths.iter_mut().flat_map(|path| path.points.iter_mut()) {
            point.x += dx;
            point.y += dy;
        }
        let number = format!("{}", index + 1);
        let size = (rect.width().min(rect.height()) * 0.3).clamp(2.0, 8.0);
        let centre = Point::new(
            rect.min.x + dx + rect.width() / 2.0,
            rect.min.y + dy + rect.height() / 2.0,
        );
        if inside(&paths, centre) {
            let mut text = mml_io::hershey_text(&number, &family, size, 1.2);
            let text_bounds = text
                .iter()
                .fold(mml_core::geometry::Rect::EMPTY, |rect, path| {
                    rect.union(path.bounds())
                });
            for point in text.iter_mut().flat_map(|path| path.points.iter_mut()) {
                point.x += centre.x - (text_bounds.min.x + text_bounds.max.x) / 2.0;
                point.y += centre.y - (text_bounds.min.y + text_bounds.max.y) / 2.0;
            }
            let label = Shape::new(
                number.clone(),
                label_layer,
                ShapeKind::Paths { paths: text },
            );
            app.selection.push(label.id);
            app.document.shapes.push(label);
        }
        let shape = Shape::new(
            trf("relief.slice_name", &[&(index + 1), &count]),
            cut_layer,
            ShapeKind::Paths { paths },
        );
        app.selection.push(shape.id);
        app.document.shapes.push(shape);
    }
    app.end_edit();
    app.mode = Mode::Design;
    app.canvas.request_fit();
    app.notify(trf("relief.slices_done", &[&count]), Level::Info);
}

fn inside(paths: &[mml_core::geometry::Path], point: mml_core::geometry::Point) -> bool {
    let mut inside = false;
    for path in paths {
        let count = path.points.len();
        for index in 0..count {
            let (a, b) = (path.points[index], path.points[(index + 1) % count]);
            if (a.y > point.y) != (b.y > point.y)
                && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
            {
                inside = !inside;
            }
        }
    }
    inside
}
