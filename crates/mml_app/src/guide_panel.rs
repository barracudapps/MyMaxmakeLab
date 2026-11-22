//! Guide of a sample project: what to prepare, then one step at a time.
//! "Prepare this step" keeps only the layers of that step, so the job
//! holds one tool on one material; changing either is a new step.

use crate::app::{Mode, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, RichText};

fn bullets(ui: &mut egui::Ui, lines: &[String]) {
    for line in lines {
        ui.horizontal_top(|ui| {
            ui.label("•");
            ui.add(egui::Label::new(line).wrap());
        });
    }
}

fn numbered(ui: &mut egui::Ui, lines: &[String]) {
    for (index, line) in lines.iter().enumerate() {
        ui.horizontal_top(|ui| {
            ui.label(RichText::new(format!("{}.", index + 1)).weak());
            ui.add(egui::Label::new(line).wrap());
        });
    }
}

fn prepare(app: &mut MyApp, step: usize) {
    app.begin_edit("history.guide_step");
    app.document.prepare_step(step);
    app.end_edit();
    app.selection.clear();
    if let Some(layer) = app.document.layers.iter().find(|layer| layer.output) {
        app.current_layer = layer.id;
    }
    app.canvas.request_fit();
}

/// The whole guide, in the design and preparation panels.
pub fn guide_card(app: &mut MyApp, ui: &mut egui::Ui) {
    let Some(guide) = app.document.guide.clone() else {
        return;
    };
    let prepared = app.document.prepared_step();
    egui::CollapsingHeader::new(RichText::new(tr("guide.title")).strong())
        .id_salt("guide")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(RichText::new(&guide.title).strong().size(15.0));
            ui.add(egui::Label::new(RichText::new(&guide.summary).weak()).wrap());
            if let Some(warning) =
                crate::samples_dialog::machine_mismatch(app, guide.machine, guide.hybrid)
            {
                ui.colored_label(crate::theme::WARNING, warning);
            }
            egui::CollapsingHeader::new(tr("guide.materials"))
                .id_salt("guide-materials")
                .default_open(prepared.is_none())
                .show(ui, |ui| bullets(ui, &guide.materials));
            egui::CollapsingHeader::new(tr("guide.tools"))
                .id_salt("guide-tools")
                .default_open(prepared.is_none())
                .show(ui, |ui| bullets(ui, &guide.tools));
            ui.add_space(4.0);
            let count = guide.steps.len();
            for (index, step) in guide.steps.iter().enumerate() {
                let current = prepared == Some(index);
                let title = format!(
                    "{} · {}",
                    trf("guide.step", &[&(index + 1), &count]),
                    step.title
                );
                let mut header = RichText::new(title);
                if current {
                    header = header.strong().color(crate::theme::ACCENT);
                }
                egui::CollapsingHeader::new(header)
                    .id_salt(("guide-step", index))
                    .open(current.then_some(true))
                    .show(ui, |ui| {
                        if !step.runs_machine() {
                            ui.label(RichText::new(tr("guide.by_hand")).weak().small());
                        }
                        numbered(ui, &step.lines);
                        if step.runs_machine() {
                            ui.horizontal(|ui| {
                                if current {
                                    ui.label(
                                        RichText::new(tr("guide.prepared"))
                                            .color(crate::theme::ACCENT),
                                    );
                                    if ui.button(tr("guide.go_run")).clicked() {
                                        app.mode = Mode::Run;
                                    }
                                } else if ui
                                    .button(RichText::new(tr("guide.prepare")).strong())
                                    .on_hover_text(tr("guide.prepare_hint"))
                                    .clicked()
                                {
                                    prepare(app, index);
                                }
                            });
                        }
                    });
            }
            ui.add_space(4.0);
            if ui
                .small_button(tr("guide.show_all"))
                .on_hover_text(tr("guide.show_all_hint"))
                .clicked()
            {
                app.begin_edit("history.guide_step");
                app.document.show_all_steps();
                app.end_edit();
            }
        });
}

/// The prepared step at the machine, so its instructions stay in view.
pub fn run_card(app: &mut MyApp, ui: &mut egui::Ui) {
    let Some(guide) = app.document.guide.clone() else {
        return;
    };
    let Some(index) = app.document.prepared_step() else {
        return;
    };
    let step = &guide.steps[index];
    ui.label(
        RichText::new(format!(
            "{} · {}",
            trf("guide.step", &[&(index + 1), &guide.steps.len()]),
            step.title
        ))
        .strong()
        .color(crate::theme::ACCENT),
    );
    egui::CollapsingHeader::new(tr("guide.instructions"))
        .id_salt("guide-run")
        .default_open(true)
        .show(ui, |ui| numbered(ui, &step.lines));
    // The next step, to keep going without going back to the design.
    if let Some(next) = (index + 1..guide.steps.len()).find(|i| guide.steps[*i].runs_machine())
        && ui
            .button(trf("guide.next", &[&(next + 1)]))
            .on_hover_text(tr("guide.prepare_hint"))
            .clicked()
    {
        prepare(app, next);
    }
}
