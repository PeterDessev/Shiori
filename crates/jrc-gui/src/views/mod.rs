//! UI views, one module per screen, all methods on `JrcGui`.

mod library;
mod mining;
mod production;
mod reader;
mod review;
mod settings;
mod setup;
mod stats;

use eframe::egui::Color32;
use jrc_core::KnowledgeStatus;

/// Background tint for a token by knowledge status. Function words get no
/// tint at all — they are not vocabulary.
pub fn status_fill(status: KnowledgeStatus, is_content: bool) -> Option<Color32> {
    if !is_content {
        return None;
    }
    match status {
        KnowledgeStatus::Unknown => Some(Color32::from_rgba_unmultiplied(235, 150, 50, 70)),
        KnowledgeStatus::Learning => Some(Color32::from_rgba_unmultiplied(80, 140, 240, 70)),
        KnowledgeStatus::Known => None,
        KnowledgeStatus::Ignored => Some(Color32::from_rgba_unmultiplied(128, 128, 128, 50)),
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
