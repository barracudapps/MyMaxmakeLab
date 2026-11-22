//! Sample projects shipped with the application: machine tests and
//! things to make, each opened in a new tab with its guide.

use crate::app::{Level, MyApp};
use crate::i18n::{code, tr, trf};
use eframe::egui::{self, RichText};
use mml_core::MachineKind;
use mml_samples::{Category, Sample, catalog};

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
        open_sample(app, sample);
        open = false;
    }
    if !open {
        app.samples_open = false;
    }
}

/// Opens `sample` in a tab of its own (the current one when empty).
pub fn open_sample(app: &mut MyApp, sample: &Sample) {
    let language = code();
    if app.is_modified() || !app.document.shapes.is_empty() || app.project_path.is_some() {
        app.new_tab();
    }
    app.load_document(sample.build(language));
    app.notify(
        trf("samples.opened", &[&sample.title(language)]),
        Level::Info,
    );
    if let Some(warning) = machine_mismatch(app, sample.machine) {
        app.notify(warning.to_owned(), Level::Warning);
    }
}

/// Why the current machine does not suit a project made for `kind`.
pub fn machine_mismatch(app: &MyApp, kind: MachineKind) -> Option<&'static str> {
    match (kind, app.machine.kind) {
        (MachineKind::Cnc, MachineKind::Laser) => Some(tr("samples.needs_cnc")),
        (MachineKind::Laser, MachineKind::Cnc) if !app.machine.laser_mode => {
            Some(tr("samples.needs_laser"))
        }
        _ => None,
    }
}
