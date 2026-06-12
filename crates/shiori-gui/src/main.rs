//! Desktop GUI for Shiori.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod session;
mod settings;
mod views;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 560.0])
            .with_title("Shiori"),
        ..Default::default()
    };
    eframe::run_native(
        "Shiori",
        options,
        Box::new(|cc| Ok(Box::new(app::ShioriGui::new(cc)))),
    )
}
