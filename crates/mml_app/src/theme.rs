//! Visual identity: soft, warm and rounded rather than the grey
//! industrial look of machine software. Applied to both egui themes so
//! switching light / dark keeps the same character.

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle, Theme, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(124, 108, 242);
pub const SUCCESS: Color32 = Color32::from_rgb(52, 168, 112);
pub const WARNING: Color32 = Color32::from_rgb(226, 152, 40);
pub const DANGER: Color32 = Color32::from_rgb(222, 74, 66);

struct Palette {
    panel: Color32,
    window: Color32,
    extreme: Color32,
    faint: Color32,
    widget: Color32,
    hovered: Color32,
    active: Color32,
    text: Color32,
    weak_text: Color32,
    border: Color32,
}

const DARK: Palette = Palette {
    panel: Color32::from_rgb(30, 31, 37),
    window: Color32::from_rgb(36, 37, 44),
    extreme: Color32::from_rgb(22, 23, 28),
    faint: Color32::from_rgb(38, 39, 47),
    widget: Color32::from_rgb(46, 48, 57),
    hovered: Color32::from_rgb(58, 60, 72),
    active: Color32::from_rgb(70, 72, 88),
    text: Color32::from_rgb(232, 230, 236),
    weak_text: Color32::from_rgb(150, 148, 160),
    border: Color32::from_rgb(52, 54, 64),
};

const LIGHT: Palette = Palette {
    panel: Color32::from_rgb(246, 244, 240),
    window: Color32::from_rgb(255, 255, 255),
    extreme: Color32::from_rgb(255, 255, 255),
    faint: Color32::from_rgb(240, 237, 232),
    widget: Color32::from_rgb(234, 231, 225),
    hovered: Color32::from_rgb(224, 220, 212),
    active: Color32::from_rgb(212, 207, 198),
    text: Color32::from_rgb(43, 42, 40),
    weak_text: Color32::from_rgb(118, 114, 108),
    border: Color32::from_rgb(222, 218, 210),
};

pub fn card_fill(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(37, 38, 46)
    } else {
        Color32::WHITE
    }
}

pub fn apply(ctx: &egui::Context) {
    for (theme, palette) in [(Theme::Dark, &DARK), (Theme::Light, &LIGHT)] {
        ctx.style_mut_of(theme, |style| {
            let radius = CornerRadius::same(9);
            let visuals = &mut style.visuals;
            visuals.panel_fill = palette.panel;
            visuals.window_fill = palette.window;
            visuals.extreme_bg_color = palette.extreme;
            visuals.faint_bg_color = palette.faint;
            visuals.window_corner_radius = CornerRadius::same(14);
            visuals.menu_corner_radius = CornerRadius::same(10);
            visuals.window_stroke = Stroke::new(1.0, palette.border);
            visuals.hyperlink_color = ACCENT;
            visuals.selection.bg_fill =
                ACCENT.gamma_multiply(if theme == Theme::Dark { 0.75 } else { 0.85 });
            visuals.selection.stroke = Stroke::new(1.5, Color32::WHITE);
            visuals.override_text_color = Some(palette.text);
            let widgets = &mut visuals.widgets;
            widgets.noninteractive.bg_fill = palette.panel;
            widgets.noninteractive.weak_bg_fill = palette.panel;
            widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
            widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.weak_text);
            widgets.noninteractive.corner_radius = radius;
            for (state, fill) in [
                (&mut widgets.inactive, palette.widget),
                (&mut widgets.hovered, palette.hovered),
                (&mut widgets.active, palette.active),
                (&mut widgets.open, palette.hovered),
            ] {
                state.bg_fill = fill;
                state.weak_bg_fill = fill;
                state.bg_stroke = Stroke::NONE;
                state.fg_stroke = Stroke::new(1.2, palette.text);
                state.corner_radius = radius;
                state.expansion = 0.0;
            }
            widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
            let spacing = &mut style.spacing;
            spacing.item_spacing = Vec2::new(8.0, 7.0);
            spacing.button_padding = Vec2::new(11.0, 5.0);
            spacing.interact_size = Vec2::new(40.0, 26.0);
            spacing.window_margin = egui::Margin::same(14);
            spacing.menu_margin = egui::Margin::same(8);
            spacing.slider_width = 130.0;
            style.text_styles = [
                (TextStyle::Small, FontId::proportional(11.5)),
                (TextStyle::Body, FontId::proportional(14.0)),
                (TextStyle::Button, FontId::proportional(14.0)),
                (TextStyle::Heading, FontId::proportional(20.0)),
                (TextStyle::Monospace, FontId::monospace(13.0)),
            ]
            .into();
        });
    }
}

/// Rounded card grouping one block of a side panel.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let dark = ui.visuals().dark_mode;
    let stroke = if dark {
        Stroke::NONE
    } else {
        Stroke::new(1.0, LIGHT.border)
    };
    let inner = egui::Frame::new()
        .fill(card_fill(dark))
        .stroke(stroke)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(8.0);
    inner
}

/// Step pill of the workflow bar: numbered circle and label.
pub fn step_pill(ui: &mut egui::Ui, number: usize, label: &str, selected: bool) -> egui::Response {
    let font = FontId::proportional(15.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE);
    let size = Vec2::new(galley.size().x + 58.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let dark = ui.visuals().dark_mode;
    let palette = if dark { &DARK } else { &LIGHT };
    let fill = if selected {
        ACCENT
    } else if response.hovered() {
        palette.hovered
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 18.0, fill);
    let circle = egui::pos2(rect.left() + 20.0, rect.center().y);
    let (circle_fill, number_color, text_color) = if selected {
        (Color32::WHITE, ACCENT, Color32::WHITE)
    } else {
        (palette.widget, palette.text, palette.text)
    };
    ui.painter().circle_filled(circle, 12.0, circle_fill);
    ui.painter().text(
        circle,
        egui::Align2::CENTER_CENTER,
        number.to_string(),
        FontId::proportional(13.0),
        number_color,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 40.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        text_color,
    );
    response
}

/// Friendly name of the nearest basic colour, for layers created from
/// the colours of an imported drawing.
pub fn color_name(color: [u8; 3]) -> &'static str {
    const NAMED: [([u8; 3], &str); 12] = [
        ([0, 0, 0], "color.black"),
        ([128, 128, 128], "color.grey"),
        ([220, 40, 40], "color.red"),
        ([255, 140, 0], "color.orange"),
        ([240, 210, 30], "color.yellow"),
        ([40, 160, 60], "color.green"),
        ([30, 180, 190], "color.cyan"),
        ([30, 100, 200], "color.blue"),
        ([130, 80, 200], "color.purple"),
        ([230, 110, 190], "color.pink"),
        ([140, 90, 50], "color.brown"),
        ([250, 250, 250], "color.white"),
    ];
    let distance = |a: [u8; 3], b: [u8; 3]| {
        (0..3)
            .map(|channel| (a[channel] as i32 - b[channel] as i32).pow(2))
            .sum::<i32>()
    };
    let key = NAMED
        .iter()
        .min_by_key(|(named, _)| distance(*named, color))
        .map_or("color.grey", |(_, key)| key);
    crate::i18n::tr(key)
}
