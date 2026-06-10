//! Stats view: knowledge totals, review activity, what to read next.

use chrono::Utc;
use eframe::egui;
use jrc_core::KnowledgeStatus;

use crate::app::JrcGui;
use crate::views::band_color;

impl JrcGui {
    pub fn show_stats(&mut self, ctx: &egui::Context) {
        // Cheap aggregate queries; fine to run per frame shown.
        let data = self.with_app(|app| {
            let words = app.db().word_status_counts()?;
            let total_reviews = app.db().review_count()?;
            let today = app.db().reviews_on_day(Utc::now())?;
            let cards = app.db().card_count()?;
            Ok((words, total_reviews, today, cards))
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some((words, total_reviews, today, cards)) = data else {
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
                    egui::Grid::new("vocab-grid").spacing([20.0, 4.0]).show(ui, |ui| {
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

                    ui.add_space(12.0);
                    ui.heading("Reviews");
                    egui::Grid::new("review-grid").spacing([20.0, 4.0]).show(ui, |ui| {
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
                    });

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
                                let Some(stats) = self.doc_stats.get(&summary.document.id.0)
                                else {
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
