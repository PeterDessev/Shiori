//! Desktop GUI for Shiori.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod session;
mod settings;
mod views;

/// The 栞 window icon, pre-rasterized to raw RGBA from
/// assets/icon/display/rounded-1024-light.png (no image decoder needed at runtime).
fn app_icon() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: include_bytes!("../../../assets/icon/desktop/shiori-64.rgba").to_vec(),
        width: 64,
        height: 64,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 560.0])
            .with_title("Shiori")
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Shiori",
        options,
        Box::new(|cc| Ok(Box::new(app::ShioriGui::new(cc)))),
    )
}
