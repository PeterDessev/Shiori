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

pub use settings::SettingsCategory;

use eframe::egui;
use eframe::egui::Color32;

/// A background rect that hugs the text instead of filling the whole text
/// row. Label rects span the full line height (ascent + descent + line
/// gap), which reads as highlight bleeding far below the glyphs.
pub fn tight_highlight_rect(rect: egui::Rect, font_size: f32) -> egui::Rect {
    let height = (font_size * 1.22).min(rect.height());
    egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), height))
}

/// Theme-aware tint for unknown words. Opaque, so adjacent token rects
/// never double up where they meet.
pub fn unknown_fill(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        Color32::from_rgb(84, 63, 24)
    } else {
        Color32::from_rgb(255, 236, 195)
    }
}

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
