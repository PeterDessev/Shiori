//! UI views, one module per screen, all methods on `ShioriGui`.

mod dictionary;
mod home;
mod library;
mod modal;
mod production;
mod reader;
mod review;
mod settings;
mod setup;
mod sources;
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
/// never double up where they meet. The light tint is amber enough to
/// stay visible on the sepia theme's paper background too.
pub fn unknown_fill(visuals: &egui::Visuals) -> Color32 {
    if visuals.dark_mode {
        Color32::from_rgb(84, 63, 24)
    } else {
        Color32::from_rgb(243, 213, 145)
    }
}

/// Human size like "740 kB", "12 MB", "1.4 GB". (Currently only the
/// dormant catalog browser renders sizes; kept for its return.)
#[allow(dead_code)]
pub fn human_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b < MB {
        format!("{:.0} kB", (b / 1024.0).max(1.0))
    } else if b < 1024.0 * MB {
        format!("{:.0} MB", b / MB)
    } else {
        format!("{:.1} GB", b / (1024.0 * MB))
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

pub fn band_color(band: shiori_app::DifficultyBand) -> Color32 {
    match band {
        shiori_app::DifficultyBand::Comfortable => Color32::from_rgb(120, 180, 120),
        shiori_app::DifficultyBand::SweetSpot => Color32::from_rgb(80, 160, 220),
        shiori_app::DifficultyBand::Challenging => Color32::from_rgb(230, 160, 60),
        shiori_app::DifficultyBand::TooHard => Color32::from_rgb(220, 90, 90),
    }
}

/// GitHub-style calendar of credited reading time, last ~18 weeks.
/// Shared by the statistics and home pages.
pub fn reading_heatmap(ui: &mut egui::Ui, by_day: &[(String, f64)]) {
    use chrono::Datelike;
    use std::collections::HashMap;
    let minutes: HashMap<&str, f64> = by_day.iter().map(|(d, s)| (d.as_str(), s / 60.0)).collect();

    const WEEKS: i64 = 18;
    let cell = 11.0;
    let gap = 2.0;
    let today = chrono::Utc::now().date_naive();
    // Grid starts on the Monday WEEKS-1 weeks back.
    let start = today
        - chrono::Duration::days((WEEKS - 1) * 7 + today.weekday().num_days_from_monday() as i64);

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(WEEKS as f32 * (cell + gap), 7.0 * (cell + gap)),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let empty = ui.visuals().faint_bg_color;
    for week in 0..WEEKS {
        for dow in 0..7 {
            let date = start + chrono::Duration::days(week * 7 + dow);
            if date > today {
                continue;
            }
            let mins = minutes
                .get(date.format("%Y-%m-%d").to_string().as_str())
                .copied()
                .unwrap_or(0.0);
            // 0 → faint, 60+ minutes → full green.
            let t = (mins / 60.0).clamp(0.0, 1.0) as f32;
            let color = if mins <= 0.0 {
                empty
            } else {
                Color32::from_rgb(
                    (40.0 + 20.0 * t) as u8,
                    (120.0 + 90.0 * t) as u8,
                    (60.0 + 20.0 * t) as u8,
                )
            };
            let min = egui::pos2(
                rect.left() + week as f32 * (cell + gap),
                rect.top() + dow as f32 * (cell + gap),
            );
            painter.rect_filled(
                egui::Rect::from_min_size(min, egui::vec2(cell, cell)),
                2.0,
                color,
            );
        }
    }
}
