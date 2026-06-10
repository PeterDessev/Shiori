//! UI views, one module per screen, all methods on `JrcGui`.

mod library;
mod mining;
mod production;
mod reader;
mod review;
mod settings;
mod setup;
mod stats;
mod welcome;

use eframe::egui::Color32;

/// The single selection-highlight color used in the reader.
pub const SELECTION_FILL: Color32 = Color32::from_rgba_premultiplied(45, 80, 135, 110);

/// Optional tint for unknown words (off by default, Settings toggle).
pub const UNKNOWN_FILL: Color32 = Color32::from_rgba_premultiplied(90, 70, 25, 80);

/// Human duration like "10m", "3.5h", "12d".
pub fn human_duration(d: chrono::Duration) -> String {
    let mins = d.num_minutes();
    if mins < 1 {
        "<1m".to_string()
    } else if mins < 60 {
        format!("{mins}m")
    } else if mins < 36 * 60 {
        format!("{:.1}h", mins as f64 / 60.0)
    } else {
        format!("{}d", d.num_days())
    }
}

pub fn band_color(band: jrc_app::DifficultyBand) -> Color32 {
    match band {
        jrc_app::DifficultyBand::Comfortable => Color32::from_rgb(120, 180, 120),
        jrc_app::DifficultyBand::SweetSpot => Color32::from_rgb(80, 160, 220),
        jrc_app::DifficultyBand::Challenging => Color32::from_rgb(230, 160, 60),
        jrc_app::DifficultyBand::TooHard => Color32::from_rgb(220, 90, 90),
    }
}
