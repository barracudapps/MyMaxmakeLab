//! Sample projects shipped with the application: machine tests and
//! things to make, each opened in a new tab with its guide.

use crate::app::{Level, MyApp};
use crate::i18n::{code, tr, trf};
use eframe::egui::{self, RichText};
use mml_core::MachineKind;
use mml_samples::fit::Fit;
use mml_samples::{Category, FitError, Sample, catalog};

/// Choosing the plywood, the boards and the size of a resizable sample
/// before opening it.
pub struct FitDialog {
    sample: &'static Sample,
    /// Percent of the size in the guide.
    scale: f64,
    /// Largest scale for the board it was worked out for.
    largest: Option<((f64, f64), f64)>,
    error: Option<String>,
}

/// Boards to choose from: name key (or a fixed label), width, height.
const BOARDS: [(&str, f64, f64); 4] = [
    ("samples.board_project", 260.0, 180.0),
    ("A4", 297.0, 210.0),
    ("A5", 210.0, 148.0),
    ("A6", 148.0, 105.0),
];

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    if !app.samples_open {
        return;
    }
    let language = code();
    let mut open = true;
    let mut chosen: Option<&'static Sample> = None;
    egui::Window::new(tr("samples.title"))
        .open(&mut open)
        .collapsible(false)
        .default_width(560.0)
        .default_height(560.0)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("samples.intro")).weak());
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for category in Category::ALL {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(mml_samples::text::get(language, category.key()))
                            .strong()
                            .size(15.0),
                    );
                    for sample in catalog().iter().filter(|s| s.category == category) {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(sample.title(language)).strong());
                                let machine = match sample.machine {
                                    _ if sample.hybrid => tr("samples.hybrid"),
                                    MachineKind::Cnc => tr("samples.cnc"),
                                    MachineKind::Laser => tr("samples.laser"),
                                };
                                ui.label(
                                    RichText::new(machine).small().color(crate::theme::ACCENT),
                                );
                                ui.label(
                                    RichText::new(trf("samples.difficulty", &[&sample.difficulty]))
                                        .small()
                                        .weak(),
                                );
                                if sample.resizable {
                                    ui.label(
                                        RichText::new(tr("samples.resizable"))
                                            .small()
                                            .color(crate::theme::ACCENT),
                                    )
                                    .on_hover_text(tr("samples.fit_intro"));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(tr("samples.open")).clicked() {
                                            chosen = Some(sample);
                                        }
                                    },
                                );
                            });
                            ui.add(
                                egui::Label::new(RichText::new(sample.summary(language)).weak())
                                    .wrap(),
                            );
                        });
                    }
                }
            });
        });
    if let Some(sample) = chosen {
        if sample.resizable {
            app.sample_fit = Some(FitDialog {
                sample,
                scale: 100.0,
                largest: None,
                error: None,
            });
        } else {
            open_sample(app, sample);
        }
        open = false;
    }
    if !open {
        app.samples_open = false;
    }
}

/// The size and stock window of a resizable sample.
pub fn show_fit(app: &mut MyApp, ctx: &egui::Context) {
    let Some(mut dialog) = app.sample_fit.take() else {
        return;
    };
    let language = code();
    let mut open = true;
    let mut go = false;
    let stock = &mut app.settings.sample_stock;
    let (machine_width, machine_height) = (app.machine.width, app.machine.height);
    egui::Window::new(format!(
        "{} — {}",
        tr("samples.fit_title"),
        dialog.sample.title(language)
    ))
    .open(&mut open)
    .collapsible(false)
    .resizable(false)
    .show(ctx, |ui| {
        ui.add(egui::Label::new(RichText::new(tr("samples.fit_intro")).weak()).wrap());
        ui.add_space(6.0);
        egui::Grid::new("sample-fit")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr("samples.thickness"));
                ui.add(
                    egui::DragValue::new(&mut stock.thickness)
                        .range(1.0..=12.0)
                        .speed(0.05)
                        .fixed_decimals(1)
                        .suffix(" mm"),
                );
                ui.end_row();
                ui.label(tr("samples.board"));
                ui.vertical(|ui| {
                    let label = |key: &'static str| {
                        if key.starts_with("samples.") {
                            tr(key).to_owned()
                        } else {
                            key.to_owned()
                        }
                    };
                    let current = BOARDS
                        .iter()
                        .find(|(_, w, h)| (*w, *h) == stock.board)
                        .map(|(key, w, h)| format!("{} ({w:.0} × {h:.0})", label(key)))
                        .unwrap_or_else(|| tr("samples.board_custom").to_owned());
                    egui::ComboBox::from_id_salt("sample-board")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (key, w, h) in BOARDS {
                                ui.selectable_value(
                                    &mut stock.board,
                                    (w, h),
                                    format!("{} ({w:.0} × {h:.0})", label(key)),
                                );
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut stock.board.0)
                                .range(50.0..=2000.0)
                                .speed(1.0)
                                .suffix(" mm"),
                        );
                        ui.label("×");
                        ui.add(
                            egui::DragValue::new(&mut stock.board.1)
                                .range(50.0..=2000.0)
                                .speed(1.0)
                                .suffix(" mm"),
                        );
                    });
                });
                ui.end_row();
                // The largest size, worked out again when the board changes.
                let largest = match dialog.largest {
                    Some((board, largest)) if board == stock.board => largest,
                    _ => {
                        let largest = dialog.sample.largest_scale(stock.board);
                        dialog.largest = Some((stock.board, largest));
                        largest
                    }
                };
                let largest_percent = (largest * 100.0).floor();
                ui.label(tr("samples.scale"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut dialog.scale, 20.0..=200.0)
                            .suffix(" %")
                            .fixed_decimals(0),
                    );
                    if ui.button(tr("samples.largest_button")).clicked() {
                        dialog.scale = largest_percent.clamp(20.0, 200.0);
                    }
                });
                ui.end_row();
                ui.label("");
                ui.label(
                    RichText::new(trf("samples.largest", &[&format!("{largest_percent:.0}")]))
                        .weak()
                        .small(),
                );
                ui.end_row();
            });
        let (corner_x, corner_y) = mml_samples::shapes::BOARD_AT;
        let off_machine = stock.board.0 + corner_x > machine_width + 1e-6
            || stock.board.1 + corner_y > machine_height + 1e-6;
        let too_big = dialog
            .largest
            .is_some_and(|(_, largest)| dialog.scale > (largest * 100.0).floor() + 1e-6);
        if off_machine {
            ui.colored_label(
                ui.visuals().error_fg_color,
                trf(
                    "samples.board_machine",
                    &[
                        &format!("{:.0}", stock.board.0),
                        &format!("{:.0}", stock.board.1),
                        &format!("{machine_width:.0}"),
                        &format!("{machine_height:.0}"),
                    ],
                ),
            );
        }
        if too_big {
            ui.colored_label(
                ui.visuals().error_fg_color,
                trf(
                    "samples.too_big",
                    &[&format!(
                        "{:.0}",
                        (dialog.largest.map_or(0.0, |(_, l)| l) * 100.0).floor()
                    )],
                ),
            );
        }
        if let Some(error) = &dialog.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !off_machine && !too_big,
                    egui::Button::new(tr("samples.open")),
                )
                .clicked()
            {
                go = true;
            }
        });
    });
    if go {
        let fit = Fit {
            scale: dialog.scale / 100.0,
            thickness: app.settings.sample_stock.thickness,
            board: app.settings.sample_stock.board,
        };
        match dialog.sample.build_fitted(language, fit) {
            Ok(document) => {
                open_document(app, dialog.sample, document);
                return;
            }
            Err(FitError::TooBig { largest }) => {
                dialog.error = Some(trf(
                    "samples.too_big",
                    &[&format!("{:.0}", (largest * 100.0).floor())],
                ));
            }
        }
    }
    if open {
        app.sample_fit = Some(dialog);
    }
}

/// Opens `sample` in a tab of its own (the current one when empty).
pub fn open_sample(app: &mut MyApp, sample: &Sample) {
    let document = sample.build(code());
    open_document(app, sample, document);
}

/// Opens the document of `sample` in a tab of its own (the current one
/// when empty).
fn open_document(app: &mut MyApp, sample: &Sample, document: mml_core::Document) {
    let language = code();
    if app.is_modified() || !app.document.shapes.is_empty() || app.project_path.is_some() {
        app.new_tab();
    }
    app.load_document(document);
    app.notify(
        trf("samples.opened", &[&sample.title(language)]),
        Level::Info,
    );
    if let Some(warning) = machine_mismatch(app, sample.machine, sample.hybrid) {
        app.notify(warning.to_owned(), Level::Warning);
    }
}

/// Why the current machine does not suit a project made for `kind`
/// (with a laser head as well when `hybrid`).
pub fn machine_mismatch(app: &MyApp, kind: MachineKind, hybrid: bool) -> Option<&'static str> {
    if hybrid && !(app.machine.kind == MachineKind::Cnc && app.machine.laser_mode) {
        return Some(tr("samples.needs_hybrid"));
    }
    match (kind, app.machine.kind) {
        (MachineKind::Cnc, MachineKind::Laser) => Some(tr("samples.needs_cnc")),
        (MachineKind::Laser, MachineKind::Cnc) if !app.machine.laser_mode => {
            Some(tr("samples.needs_laser"))
        }
        _ => None,
    }
}
