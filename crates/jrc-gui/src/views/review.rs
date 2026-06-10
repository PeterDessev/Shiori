//! Review view: due cards, always shown in their source sentence.

use chrono::Utc;
use eframe::egui;
use jrc_dict::register::UsageProfile;
use jrc_srs::Rating;

use crate::app::JrcGui;
use crate::views::human_duration;

impl JrcGui {
    pub fn show_review(&mut self, ctx: &egui::Context) {
        let mut answered: Option<Rating> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.review.queue.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading("All caught up 🎉");
                    ui.label("No cards are due right now. Go read something!");
                    if ui.button("Check again").clicked() {
                        self.load_review_queue();
                        self.refresh_caches();
                    }
                });
                return;
            }

            let item = &self.review.queue[0];
            ui.horizontal(|ui| {
                ui.label(format!("{} due", self.review.queue.len()));
                ui.separator();
                ui.label(format!(
                    "state: {} · reps: {} · lapses: {}",
                    item.card.state.as_str(),
                    item.card.reps,
                    item.card.lapses
                ));
            });
            ui.add_space(24.0);

            // Front: the sentence with the word emphasized; the word alone
            // if the source sentence is gone.
            ui.vertical_centered(|ui| {
                match &item.sentence {
                    Some(sentence) => {
                        let lemma = &item.word.key.lemma;
                        // Render sentence with the target word underlined where it
                        // (or its surface form) appears.
                        ui.label(egui::RichText::new(&sentence.text).size(26.0));
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("→ {lemma}"))
                                .size(22.0)
                                .strong()
                                .color(egui::Color32::from_rgb(80, 140, 240)),
                        );
                    }
                    None => {
                        ui.label(egui::RichText::new(&item.word.key.lemma).size(34.0).strong());
                    }
                }

                ui.add_space(20.0);

                if !self.review.revealed {
                    if ui
                        .add_sized([200.0, 36.0], egui::Button::new("Show answer"))
                        .clicked()
                    {
                        self.review.revealed = true;
                    }
                } else {
                    if !item.word.key.reading.is_empty() {
                        ui.label(egui::RichText::new(&item.word.key.reading).size(22.0));
                    }
                    if let Some(entry) = &item.entry {
                        ui.label(egui::RichText::new(entry.short_gloss()).size(18.0));
                        let profile = UsageProfile::from_misc_codes(entry.misc_codes());
                        if !profile.is_neutral() {
                            let labels: Vec<&str> =
                                profile.registers.iter().map(|r| r.label()).collect();
                            ui.weak(labels.join(" · "));
                        }
                    }
                    ui.add_space(18.0);

                    // Rating buttons with interval previews.
                    let now = Utc::now();
                    let scheduler = self
                        .app
                        .as_ref()
                        .map(|_| jrc_srs::Scheduler::default())
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center)
                                .with_main_align(egui::Align::Center),
                            |ui| {
                                for (rating, label, color) in [
                                    (Rating::Again, "Again", egui::Color32::from_rgb(220, 90, 90)),
                                    (Rating::Hard, "Hard", egui::Color32::from_rgb(230, 160, 60)),
                                    (Rating::Good, "Good", egui::Color32::from_rgb(110, 180, 110)),
                                    (Rating::Easy, "Easy", egui::Color32::from_rgb(80, 160, 220)),
                                ] {
                                    let preview = scheduler.review(&item.card, rating, now);
                                    let interval = human_duration(preview.due - now);
                                    let text = format!("{label}\n{interval}");
                                    if ui
                                        .add_sized(
                                            [90.0, 44.0],
                                            egui::Button::new(
                                                egui::RichText::new(text).color(color),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        answered = Some(rating);
                                    }
                                }
                            },
                        );
                    });
                }
            });
        });

        if let Some(rating) = answered {
            let word_id = self.review.queue[0].word.id;
            if self.with_app(|app| app.answer_review(word_id, rating)).is_some() {
                self.review.queue.remove(0);
                self.review.revealed = false;
                if self.review.queue.is_empty() {
                    self.load_review_queue();
                }
                self.refresh_caches();
                self.refresh_reader_tokens();
            }
        }
    }
}
