//! Desktop GUI for the Japanese Reading Companion.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod settings;
mod views;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 560.0])
            .with_title("Japanese Reading Companion"),
        ..Default::default()
    };
    eframe::run_native(
        "Japanese Reading Companion",
        options,
        Box::new(|cc| {
            fonts::install_japanese_fonts(&cc.egui_ctx);
            Ok(Box::new(app::JrcGui::new(cc)))
        }),
    )
}
