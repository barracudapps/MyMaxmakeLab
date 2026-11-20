//! Tool library window: quick creation, advanced cutting data and
//! import from Fusion 360.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, DragValue, RichText};
use mml_materials::{Tool, ToolKind};

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.tools_open {
        return;
    }
    let mut open = true;
    let mut remove = None;
    let mut save = false;
    egui::Window::new(tr("tools.title"))
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(tr("tools.add")).clicked() {
                    app.library.tools.push(Tool {
                        name: tr("tools.new_name").to_owned(),
                        ..Tool::default()
                    });
                    app.tool_selected = Some(app.library.tools.len() - 1);
                    save = true;
                }
                if ui
                    .button(tr("tools.import_fusion"))
                    .on_hover_text(tr("tools.import_fusion_hint"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Fusion 360", &["json", "tools"])
                        .pick_file()
                {
                    match std::fs::read_to_string(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|text| {
                            mml_materials::import_fusion(&text).map_err(|error| error.to_string())
                        }) {
                        Ok(tools) => {
                            let count = tools.len();
                            app.library.tools.extend(tools);
                            app.notices.push((
                                trf("tools.imported", &[&count]),
                                Level::Info,
                                std::time::Instant::now(),
                            ));
                            save = true;
                        }
                        Err(error) => app.notices.push((
                            trf("file.open_failed", &[&path.display(), &error]),
                            Level::Error,
                            std::time::Instant::now(),
                        )),
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for (index, tool) in app.library.tools.iter().enumerate() {
                        let label = format!("{} · {}", tool.label(), tr(tool.kind.key()));
                        if ui
                            .selectable_label(app.tool_selected == Some(index), label)
                            .clicked()
                        {
                            app.tool_selected = Some(index);
                        }
                    }
                    if app.library.tools.is_empty() {
                        ui.label(RichText::new(tr("tools.empty")).weak());
                    }
                });
            let Some(index) = app
                .tool_selected
                .filter(|index| *index < app.library.tools.len())
            else {
                return;
            };
            ui.separator();
            let tool = &mut app.library.tools[index];
            let before = tool.clone();
            egui::Grid::new("tool-simple")
                .num_columns(2)
                .spacing([10.0, 5.0])
                .show(ui, |ui| {
                    ui.label(tr("materials.name"));
                    ui.text_edit_singleline(&mut tool.name);
                    ui.end_row();
                    ui.label(tr("tools.type"));
                    ui.horizontal_wrapped(|ui| {
                        let angle = match tool.kind {
                            ToolKind::VBit { angle } => angle,
                            _ => 60.0,
                        };
                        for kind in [
                            ToolKind::FlatEndMill,
                            ToolKind::BallEndMill,
                            ToolKind::VBit { angle },
                            ToolKind::Drill,
                        ] {
                            let same =
                                std::mem::discriminant(&kind) == std::mem::discriminant(&tool.kind);
                            if ui.selectable_label(same, tr(kind.key())).clicked() && !same {
                                tool.kind = kind;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(tr("op.tool_diameter"));
                    ui.add(
                        DragValue::new(&mut tool.diameter)
                            .range(0.05..=50.0)
                            .suffix(" mm")
                            .speed(0.01),
                    );
                    ui.end_row();
                    if let ToolKind::VBit { angle } = &mut tool.kind {
                        ui.label(tr("op.vbit_angle"));
                        ui.add(DragValue::new(angle).range(10.0..=170.0).suffix(" °"));
                        ui.end_row();
                    }
                });
            egui::CollapsingHeader::new(tr("tools.advanced")).show(ui, |ui| {
                egui::Grid::new("tool-advanced")
                    .num_columns(2)
                    .spacing([10.0, 5.0])
                    .show(ui, |ui| {
                        ui.label(tr("tools.flutes"));
                        ui.add(DragValue::new(&mut tool.flutes).range(1..=12));
                        ui.end_row();
                        ui.label(tr("op.feed"));
                        ui.add(
                            DragValue::new(&mut tool.feed)
                                .range(1.0..=20_000.0)
                                .suffix(" mm/min")
                                .speed(10.0),
                        );
                        ui.end_row();
                        ui.label(tr("op.plunge"));
                        ui.add(
                            DragValue::new(&mut tool.plunge)
                                .range(1.0..=5_000.0)
                                .suffix(" mm/min")
                                .speed(5.0),
                        );
                        ui.end_row();
                        ui.label(tr("op.spindle"));
                        ui.add(
                            DragValue::new(&mut tool.spindle)
                                .range(0.0..=60_000.0)
                                .suffix(" rpm")
                                .speed(100.0),
                        );
                        ui.end_row();
                        ui.label(tr("op.pass_depth"));
                        ui.add(
                            DragValue::new(&mut tool.stepdown)
                                .range(0.01..=20.0)
                                .suffix(" mm")
                                .speed(0.05),
                        );
                        ui.end_row();
                        ui.label(tr("op.stepover"));
                        let mut stepover = tool.stepover * 100.0;
                        if ui
                            .add(egui::Slider::new(&mut stepover, 5.0..=95.0).suffix(" %"))
                            .changed()
                        {
                            tool.stepover = stepover / 100.0;
                        }
                        ui.end_row();
                        ui.label(tr("tools.notes"));
                        ui.text_edit_singleline(&mut tool.notes);
                        ui.end_row();
                    });
            });
            save |= *tool != before;
            if ui
                .button(RichText::new(tr("tools.remove")).color(crate::theme::DANGER))
                .clicked()
            {
                remove = Some(index);
            }
        });
    if let Some(index) = remove {
        app.library.tools.remove(index);
        app.tool_selected = None;
        save = true;
    }
    if save {
        app.save_library();
    }
    app.tools_open = open;
}
