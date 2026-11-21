//! "Import from your software": tool and material libraries of Fusion,
//! LightBurn and MaxmakeLAB found in their usual folders, imported in
//! one go and, if wanted, read again at start when they change.

use crate::app::{Level, MyApp};
use crate::i18n::{tr, trf};
use eframe::egui::{self, RichText};
use mml_materials::sources::{self, Imported, Roots, Source, SourceKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A library followed: read again at start when its date changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncedSource {
    pub path: PathBuf,
    pub tools: bool,
    pub modified: u64,
}

struct Row {
    source: Source,
    /// Items in the library, or why it cannot be read.
    count: Result<usize, String>,
    selected: bool,
}

pub struct SourcesDialog {
    rows: Vec<Row>,
    keep: bool,
}

impl SourcesDialog {
    pub fn new(app: &MyApp) -> Self {
        let rows = sources::discover(&Roots::from_env())
            .into_iter()
            .map(|source| {
                let count = sources::read(&source).map(|imported| imported.len());
                let followed = app
                    .settings
                    .synced_sources
                    .iter()
                    .any(|synced| synced.path == source.path);
                let selected = !followed && count.as_ref().is_ok_and(|count| *count > 0);
                Row {
                    source,
                    count,
                    selected,
                }
            })
            // Other JSON files in Fusion's folder are not tool libraries.
            .filter(|row| row.source.kind != SourceKind::FusionTools || row.count != Ok(0))
            .collect();
        Self { rows, keep: true }
    }

    pub fn found_anything(&self) -> bool {
        !self.rows.is_empty()
    }
}

/// Adds what `imported` holds to the library; returns (tools, presets)
/// actually added.
fn merge(app: &mut MyApp, imported: Imported) -> (usize, usize) {
    match imported {
        Imported::Tools(tools) => (app.library.merge_tools(tools), 0),
        Imported::Presets(presets) => (0, app.library.merge(presets)),
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let Some(dialog) = &mut app.sources else {
        return;
    };
    let mut open = true;
    let mut import = false;
    let mut rescan = false;
    let synced: Vec<PathBuf> = app
        .settings
        .synced_sources
        .iter()
        .map(|synced| synced.path.clone())
        .collect();
    let mut forget: Option<PathBuf> = None;
    egui::Window::new(tr("sources.title"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("sources.intro")).weak());
            ui.add_space(4.0);
            if dialog.rows.is_empty() {
                ui.label(tr("sources.none"));
                ui.label(RichText::new(tr("sources.where")).weak().small());
            }
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    let mut software = "";
                    for row in &mut dialog.rows {
                        let name = row.source.kind.software();
                        if name != software {
                            software = name;
                            ui.add_space(4.0);
                            ui.label(RichText::new(name).strong());
                        }
                        ui.horizontal_wrapped(|ui| {
                            let count = match &row.count {
                                Ok(count) if row.source.kind.is_tools() => {
                                    trf("sources.tools", &[count])
                                }
                                Ok(count) => trf("sources.presets", &[count]),
                                Err(_) => tr("sources.unreadable").to_owned(),
                            };
                            ui.add_enabled(
                                row.count.is_ok(),
                                egui::Checkbox::new(
                                    &mut row.selected,
                                    format!("{} — {count}", row.source.name()),
                                ),
                            )
                            .on_hover_text(row.source.path.display().to_string());
                            if synced.contains(&row.source.path) {
                                ui.label(
                                    RichText::new(tr("sources.followed"))
                                        .small()
                                        .color(crate::theme::SUCCESS),
                                )
                                .on_hover_text(tr("sources.followed_hint"));
                                if ui.small_button(tr("sources.stop")).clicked() {
                                    forget = Some(row.source.path.clone());
                                }
                            }
                        });
                    }
                });
            ui.add_space(4.0);
            ui.checkbox(&mut dialog.keep, tr("sources.keep"))
                .on_hover_text(tr("sources.keep_hint"));
            ui.horizontal(|ui| {
                let any = dialog.rows.iter().any(|row| row.selected);
                import = ui
                    .add_enabled(any, egui::Button::new(tr("sources.import")))
                    .clicked();
                rescan = ui.button(tr("sources.rescan")).clicked();
            });
        });
    if let Some(path) = forget {
        app.settings
            .synced_sources
            .retain(|synced| synced.path != path);
    }
    if import {
        let dialog = app.sources.take().unwrap();
        let mut added = (0, 0);
        for row in dialog.rows.iter().filter(|row| row.selected) {
            match sources::read(&row.source) {
                Ok(imported) => {
                    let (tools, presets) = merge(app, imported);
                    added = (added.0 + tools, added.1 + presets);
                    if dialog.keep {
                        follow(app, &row.source);
                    }
                }
                Err(error) => app.notify(
                    trf("file.open_failed", &[&row.source.path.display(), &error]),
                    Level::Error,
                ),
            }
        }
        app.save_library();
        app.notify(trf("sources.done", &[&added.0, &added.1]), Level::Info);
        return;
    }
    if rescan {
        app.sources = Some(SourcesDialog::new(app));
    } else if !open {
        app.sources = None;
    }
}

fn follow(app: &mut MyApp, source: &Source) {
    let modified = sources::modified(&source.path).unwrap_or(0);
    let entry = SyncedSource {
        path: source.path.clone(),
        tools: source.kind.is_tools(),
        modified,
    };
    match app
        .settings
        .synced_sources
        .iter_mut()
        .find(|synced| synced.path == source.path)
    {
        Some(existing) => *existing = entry,
        None => app.settings.synced_sources.push(entry),
    }
}

/// At start: libraries followed and changed since are read again (only
/// new tools and settings are added), and on the very first start the
/// dialog opens if anything was found.
pub fn at_start(app: &mut MyApp) {
    let mut added = (0, 0);
    let mut followed = std::mem::take(&mut app.settings.synced_sources);
    for synced in &mut followed {
        let Some(modified) = sources::modified(&synced.path) else {
            continue;
        };
        if modified == synced.modified {
            continue;
        }
        let kind = if synced.tools {
            SourceKind::FusionTools
        } else {
            SourceKind::LightBurnMaterials
        };
        let source = Source {
            kind,
            path: synced.path.clone(),
        };
        if let Ok(imported) = sources::read(&source) {
            let (tools, presets) = merge(app, imported);
            added = (added.0 + tools, added.1 + presets);
            synced.modified = modified;
        }
    }
    app.settings.synced_sources = followed;
    if added != (0, 0) {
        app.save_library();
        app.notify(trf("sources.updated", &[&added.0, &added.1]), Level::Info);
    }
    if !app.settings.sources_offered {
        app.settings.sources_offered = true;
        let dialog = SourcesDialog::new(app);
        if dialog.found_anything() {
            app.sources = Some(dialog);
        }
    }
}
