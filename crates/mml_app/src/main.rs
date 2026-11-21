#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assist;
mod camera_panel;
mod canvas;
mod collision;
mod firmware;
mod fleet;
mod flow;
mod i18n;
mod icons;
mod library;
mod machine_panel;
mod makers_dialog;
mod manual_probe;
mod nest_dialog;
mod ornament_dialog;
mod os;
mod pairing;
mod pairing_dialog;
mod palette;
mod panels;
mod paper_dialog;
mod preview;
mod relief_dialog;
mod settings;
mod sources_dialog;
mod sprue_dialog;
mod stats_dialog;
mod text_tools;
mod theme;
mod tools_dialog;
mod update;
mod view3d;
mod workflow;

use eframe::egui;

fn main() -> eframe::Result {
    // Without it eframe shows its own logo, even in the macOS Dock while
    // the app runs (it replaces the bundle icon at launch).
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
        .expect("bundled icon is a valid PNG");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MyMaxmakeLab")
            .with_icon(icon)
            .with_app_id("mymaxmakelab")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([980.0, 620.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "MyMaxmakeLab",
        options,
        Box::new(|creation| Ok(Box::new(app::MyApp::new(creation)))),
    )
}
