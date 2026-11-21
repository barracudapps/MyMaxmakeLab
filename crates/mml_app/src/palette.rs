//! Command palette (Ctrl/Cmd+K): every action and window reachable by
//! typing a few letters, without hunting through menus.

use crate::app::{Mode, MyApp};
use crate::i18n::tr;
use eframe::egui::{self, Key, RichText};

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    query: String,
    selected: usize,
}

#[derive(Clone, Copy)]
enum Action {
    Mode(Mode),
    Import,
    Open,
    Save,
    SaveAs,
    NewTab,
    Undo,
    Redo,
    Fit,
    Toggle3d,
    ExportGcode,
    Stats,
    Fleet,
    Makers,
    Ornament,
    Sprue,
    Tools,
    Preferences,
    Camera,
    MachineSettings,
    Updates,
    Paper,
    StartPanel,
}

/// Menu entries reused as command names, so the palette speaks the
/// language of the interface.
const COMMANDS: [(&str, Action); 21] = [
    ("mode.design", Action::Mode(Mode::Design)),
    ("mode.prepare", Action::Mode(Mode::Prepare)),
    ("mode.run", Action::Mode(Mode::Run)),
    ("menu.import", Action::Import),
    ("menu.open", Action::Open),
    ("menu.save", Action::Save),
    ("menu.save_as", Action::SaveAs),
    ("palette.new_tab", Action::NewTab),
    ("palette.undo", Action::Undo),
    ("palette.redo", Action::Redo),
    ("menu.fit", Action::Fit),
    ("palette.toggle_3d", Action::Toggle3d),
    ("palette.export", Action::ExportGcode),
    ("stats.title", Action::Stats),
    ("fleet.title", Action::Fleet),
    ("makers.title", Action::Makers),
    ("ornament.title", Action::Ornament),
    ("sprue.title", Action::Sprue),
    ("tools.title", Action::Tools),
    ("menu.preferences", Action::Preferences),
    ("palette.camera", Action::Camera),
];

const EXTRA: [(&str, Action); 4] = [
    ("menu.machine_settings", Action::MachineSettings),
    ("update.check", Action::Updates),
    ("paper.title", Action::Paper),
    ("start.reopen", Action::StartPanel),
];

/// Lower case without accents, so "prepa" finds "Préparation".
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ß' => 's',
            other => other,
        })
        .collect()
}

/// Every word of the query must start a word of the label, or appear in
/// it; ranked by how early they match.
fn score(label: &str, query: &str) -> Option<usize> {
    let label = fold(label);
    let mut total = 0;
    for word in fold(query).split_whitespace() {
        let position = label.find(word)?;
        let at_word_start =
            position == 0 || !label.as_bytes()[position - 1].is_ascii_alphanumeric();
        total += if at_word_start {
            position
        } else {
            position + 100
        };
    }
    Some(total)
}

fn run(app: &mut MyApp, action: Action) {
    match action {
        Action::Mode(mode) => app.mode = mode,
        Action::Import => app.import_dialog(),
        Action::Open => app.open_dialog(),
        Action::Save => {
            app.save(false);
        }
        Action::SaveAs => {
            app.save(true);
        }
        Action::NewTab => app.new_tab(),
        Action::Undo => app.undo(),
        Action::Redo => app.redo(),
        Action::Fit => app.canvas.request_fit(),
        Action::Toggle3d => app.view3d.enabled = !app.view3d.enabled,
        Action::ExportGcode => app.export_gcode(),
        Action::Stats => app.stats_open = true,
        Action::Fleet => app.fleet.open = true,
        Action::Makers => app.makers = Some(Default::default()),
        Action::Ornament => app.ornament = Some(Default::default()),
        Action::Sprue => app.sprue = Some(Default::default()),
        Action::Tools => app.tools_open = true,
        Action::Preferences => app.settings_open = true,
        Action::Camera => app.camera.open = true,
        Action::MachineSettings => app.machine_dialog = Some(app.machine.clone()),
        Action::Updates => app.request_update_check(),
        Action::StartPanel => {
            app.start_panel = true;
            app.settings.show_start_panel = true;
            app.mode = Mode::Design;
            app.canvas.tool = crate::canvas::Tool::Select;
        }
        Action::Paper => app.paper = Some(crate::paper_dialog::PaperDialog::new(app)),
    }
}

pub fn show(app: &mut MyApp, ctx: &egui::Context) {
    let toggle = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::K);
    if ctx.input_mut(|input| input.consume_shortcut(&toggle)) {
        app.palette.open = !app.palette.open;
        app.palette.query.clear();
        app.palette.selected = 0;
    }
    if !app.palette.open {
        return;
    }
    let mut matches: Vec<(usize, &str, Action)> = COMMANDS
        .iter()
        .chain(EXTRA.iter())
        .filter_map(|&(key, action)| {
            let label = tr(key);
            score(label, &app.palette.query).map(|rank| (rank, label, action))
        })
        .collect();
    matches.sort_by_key(|(rank, ..)| *rank);
    let (up, down, enter, escape) = ctx.input(|input| {
        (
            input.key_pressed(Key::ArrowUp),
            input.key_pressed(Key::ArrowDown),
            input.key_pressed(Key::Enter),
            input.key_pressed(Key::Escape),
        )
    });
    let palette = &mut app.palette;
    if down {
        palette.selected = (palette.selected + 1).min(matches.len().saturating_sub(1));
    }
    if up {
        palette.selected = palette.selected.saturating_sub(1);
    }
    palette.selected = palette.selected.min(matches.len().saturating_sub(1));
    let mut chosen = enter
        .then(|| matches.get(palette.selected).map(|(.., action)| *action))
        .flatten();
    egui::Window::new("palette")
        .title_bar(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0))
        .fixed_size(egui::vec2(460.0, 0.0))
        .show(ctx, |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut palette.query)
                    .hint_text(tr("palette.hint"))
                    .desired_width(f32::INFINITY),
            );
            field.request_focus();
            if field.changed() {
                palette.selected = 0;
            }
            ui.separator();
            for (index, (_, label, action)) in matches.iter().take(10).enumerate() {
                let text = RichText::new(*label);
                if ui
                    .selectable_label(index == palette.selected, text)
                    .clicked()
                {
                    chosen = Some(*action);
                }
            }
            if matches.is_empty() {
                ui.label(RichText::new(tr("palette.none")).weak());
            }
        });
    if let Some(action) = chosen {
        app.palette.open = false;
        run(app, action);
    } else if escape {
        app.palette.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_and_word_starts_are_handled() {
        assert!(score("Préparation", "prepa").is_some());
        assert!(score("Statistiques et coûts", "couts").is_some());
        assert!(score("Générer un motif...", "zebra").is_none());
        // A match at the start of a word ranks before one inside a word.
        assert!(score("Open project", "pro").unwrap() < score("Improve", "pro").unwrap());
    }
}
