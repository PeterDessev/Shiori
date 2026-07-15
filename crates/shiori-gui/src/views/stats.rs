//! Stats view: vocabulary, level grading, review forecast and health,
//! reading time, and per-document difficulty.

use chrono::{Datelike, Utc};
use eframe::egui;
use shiori_core::KnowledgeStatus;

use crate::app::ShioriGui;
use crate::views::band_color;

impl ShioriGui {
    pub fn show_stats(&mut self, ctx: &egui::Context) {
        // Cheap aggregate queries; fine to run per frame shown.
        let data = self.with_app(|app| {
            let words = app.db().word_status_counts(app.active_lang())?;
            let total_reviews = app.db().review_count()?;
            let today = app.db().reviews_on_day(Utc::now())?;
            let cards = app.db().card_count()?;
            let overview = app.stats_overview()?;
            Ok((words, total_reviews, today, cards, overview))
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some((words, total_reviews, today, cards, overview)) = data else {
                ui.weak("Statistics unavailable.");
                return;
            };

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.heading("Vocabulary");
                    let count_of = |status: KnowledgeStatus| {
                        words
                            .iter()
                            .find(|(s, _)| *s == status)
                            .map(|(_, n)| *n)
                            .unwrap_or(0)
                    };
                    egui::Grid::new("vocab-grid")
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Known");
                            ui.strong(count_of(KnowledgeStatus::Known).to_string());
                            ui.end_row();
                            ui.label("Learning");
                            ui.strong(count_of(KnowledgeStatus::Learning).to_string());
                            ui.end_row();
                            ui.label("Seen but unknown");
                            ui.strong(count_of(KnowledgeStatus::Unknown).to_string());
                            ui.end_row();
                            ui.label("Ignored");
                            ui.strong(count_of(KnowledgeStatus::Ignored).to_string());
                            ui.end_row();
                        });

                    // Level grading.
                    if !overview.jlpt.is_empty() {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("Level").strong());
                        match &overview.comfortable_level {
                            Some(level) => {
                                ui.label(format!("Comfortable reading level: {level}"));
                            }
                            None => {
                                ui.weak(
                                    "Comfortable reading level: not enough known \
                                     vocabulary yet (keep reading!)",
                                );
                            }
                        }
                        ui.add_space(4.0);
                        for share in &overview.jlpt {
                            let frac = if share.total > 0 {
                                share.known as f32 / share.total as f32
                            } else {
                                0.0
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!("N{}", share.level));
                                ui.add(egui::ProgressBar::new(frac).desired_width(220.0).text(
                                    format!(
                                        "{}/{} ({:.0}%)",
                                        share.known,
                                        share.total,
                                        frac * 100.0
                                    ),
                                ));
                            });
                        }
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.weak("Corpus coverage:");
                            for (bound, known) in &overview.rank_bands {
                                ui.weak(format!(
                                    "top {}k: {:.0}%",
                                    bound / 1000,
                                    *known as f64 / f64::from(*bound) * 100.0
                                ));
                            }
                        });
                    }

                    ui.add_space(12.0);
                    ui.heading("Reviews");
                    egui::Grid::new("review-grid")
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Active cards");
                            ui.strong(cards.to_string());
                            ui.end_row();
                            ui.label("Due now");
                            ui.strong(self.due_count.to_string());
                            ui.end_row();
                            ui.label("Reviews today");
                            ui.strong(today.to_string());
                            ui.end_row();
                            ui.label("Reviews all time");
                            ui.strong(total_reviews.to_string());
                            ui.end_row();
                            ui.label("Retention (30 days)");
                            ui.strong(match overview.retention_30d {
                                Some(r) => format!("{:.0}%", r * 100.0),
                                None => "—".into(),
                            });
                            ui.end_row();
                            ui.label("New words/day (30 days)");
                            ui.strong(format!("{:.1}", overview.learning_rate_30d));
                            ui.end_row();
                        });

                    if !overview.due_forecast.is_empty() {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Due over the next two weeks").strong());
                        due_forecast_bars(ui, &overview.due_forecast);
                    }

                    ui.add_space(12.0);
                    ui.heading("Reading");
                    if overview.total_reading_seconds > 0.0 {
                        let mins = overview.total_reading_seconds / 60.0;
                        let mut line = format!(
                            "{} total · {} characters",
                            crate::views::human_duration(chrono::Duration::seconds(
                                overview.total_reading_seconds as i64
                            )),
                            overview.total_reading_chars
                        );
                        match overview.velocity_cpm {
                            Some(v) => line.push_str(&format!(" · {v:.0} chars/min")),
                            None if mins < 10.0 => {
                                line.push_str(" · velocity appears after ~10 minutes")
                            }
                            None => {}
                        }
                        ui.label(line);
                        ui.add_space(4.0);
                        reading_heatmap(ui, &overview.reading_by_day);
                    } else {
                        ui.weak(
                            "No reading time recorded yet — the clock runs while \
                             a book is open in the reader.",
                        );
                    }

                    if !overview.matured_by_day.is_empty() {
                        ui.add_space(8.0);
                        let total: u32 = overview.matured_by_day.iter().map(|(_, n)| n).sum();
                        ui.label(format!(
                            "{total} words matured in the SRS (stability past the \
                             known threshold)."
                        ));
                    }

                    ui.add_space(12.0);
                    ui.heading("Reading difficulty");
                    if self.library.is_empty() {
                        ui.weak("Import documents to see difficulty estimates.");
                        return;
                    }
                    ui.label(
                        "Known: tokens you know or ignore · Learning: in the SRS (“just out \
                         of reach”) · Unknown: never studied. The sweet spot for \
                         comprehensible input is roughly 2–5% unknown.",
                    );
                    ui.add_space(6.0);

                    egui::Grid::new("difficulty-grid")
                        .striped(true)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.strong("Document");
                            ui.strong("Known");
                            ui.strong("Learning");
                            ui.strong("Unknown");
                            ui.strong("Verdict");
                            ui.end_row();
                            for summary in &self.library {
                                let Some(stats) = self.doc_stats.get(&summary.document.id.0) else {
                                    continue;
                                };
                                ui.label(&summary.document.title);
                                ui.label(format!("{:.1}%", stats.known_share() * 100.0));
                                ui.label(format!("{:.1}%", stats.learning_share() * 100.0));
                                ui.label(format!("{:.1}%", stats.unknown_share() * 100.0));
                                ui.colored_label(band_color(stats.band), stats.band.label());
                                ui.end_row();
                            }
                        });
                });
        });
    }
}

/// Simple bar row for the 14-day due forecast.
fn due_forecast_bars(ui: &mut egui::Ui, forecast: &[(String, u32)]) {
    let max = forecast.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1);
    let bar_w = 26.0;
    let height = 56.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(forecast.len() as f32 * (bar_w + 4.0), height + 16.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let accent = egui::Color32::from_rgb(80, 140, 240);
    for (i, (day, n)) in forecast.iter().enumerate() {
        let x = rect.left() + i as f32 * (bar_w + 4.0);
        let h = height * (*n as f32 / max as f32);
        let bar = egui::Rect::from_min_max(
            egui::pos2(x, rect.top() + height - h),
            egui::pos2(x + bar_w, rect.top() + height),
        );
        painter.rect_filled(bar, 2.0, accent);
        painter.text(
            egui::pos2(x + bar_w / 2.0, rect.top() + height - h - 2.0),
            egui::Align2::CENTER_BOTTOM,
            n.to_string(),
            egui::FontId::proportional(10.0),
            ui.visuals().text_color(),
        );
        // Day-of-month label.
        let label = day.get(8..10).unwrap_or("");
        painter.text(
            egui::pos2(x + bar_w / 2.0, rect.top() + height + 2.0),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::proportional(9.0),
            ui.visuals().weak_text_color(),
        );
    }
}

/// GitHub-style calendar of credited reading time, last ~18 weeks.
fn reading_heatmap(ui: &mut egui::Ui, by_day: &[(String, f64)]) {
    use std::collections::HashMap;
    let minutes: HashMap<&str, f64> = by_day.iter().map(|(d, s)| (d.as_str(), s / 60.0)).collect();

    const WEEKS: i64 = 18;
    let cell = 11.0;
    let gap = 2.0;
    let today = Utc::now().date_naive();
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
                egui::Color32::from_rgb(
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
