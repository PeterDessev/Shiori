//! Review view: due cards shown in their source sentence, answered with a
//! simple correct/incorrect judgment (FSRS Good/Again underneath).

use chrono::Utc;
use eframe::egui;
use shiori_dict::register::UsageProfile;
use shiori_srs::Rating;

use crate::app::ShioriGui;
use crate::settings::shortcut_pressed;
use crate::views::human_duration;

impl ShioriGui {
    pub fn show_review(&mut self, ctx: &egui::Context) {
        let mut answered: Option<Rating> = None;

        // Keyboard shortcuts.
        let shortcuts = self.settings.shortcuts.clone();
        if !self.review.queue.is_empty() {
            if !self.review.revealed {
                if shortcut_pressed(ctx, &shortcuts.review_reveal) {
                    self.review.revealed = true;
                }
            } else {
                if shortcut_pressed(ctx, &shortcuts.review_correct) {
                    answered = Some(Rating::Good);
                }
                if shortcut_pressed(ctx, &shortcuts.review_incorrect) {
                    answered = Some(Rating::Again);
                }
            }
        }

        // Action bar pinned to the bottom of the screen.
        if !self.review.queue.is_empty() {
            egui::TopBottomPanel::bottom("review-actions")
                .min_height(76.0)
                .show(ctx, |ui| {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        if !self.review.revealed {
                            if ui
                                .add_sized(
                                    [260.0, 44.0],
                                    egui::Button::new(format!(
                                        "Show answer  ({})",
                                        shortcuts.review_reveal
                                    )),
                                )
                                .clicked()
                            {
                                self.review.revealed = true;
                            }
                        } else {
                            let item = &self.review.queue[0];
                            let now = Utc::now();
                            let scheduler = shiori_srs::Scheduler::default();
                            let again = scheduler.review(&item.card, Rating::Again, now);
                            let good = scheduler.review(&item.card, Rating::Good, now);
                            ui.horizontal(|ui| {
                                let total = 2.0 * 200.0 + 16.0;
                                ui.add_space((ui.available_width() - total).max(0.0) / 2.0);
                                if ui
                                    .add_sized(
                                        [200.0, 48.0],
                                        egui::Button::new(
                                            egui::RichText::new(format!(
                                                "✗ Incorrect ({})\n{}",
                                                shortcuts.review_incorrect,
                                                human_duration(again.due - now)
                                            ))
                                            .color(egui::Color32::from_rgb(220, 90, 90)),
                                        ),
                                    )
                                    .clicked()
                                {
                                    answered = Some(Rating::Again);
                                }
                                ui.add_space(16.0);
                                if ui
                                    .add_sized(
                                        [200.0, 48.0],
                                        egui::Button::new(
                                            egui::RichText::new(format!(
                                                "✓ Correct ({})\n{}",
                                                shortcuts.review_correct,
                                                human_duration(good.due - now)
                                            ))
                                            .color(egui::Color32::from_rgb(110, 180, 110)),
                                        ),
                                    )
                                    .clicked()
                                {
                                    answered = Some(Rating::Good);
                                }
                            });
                        }
                    });
                    ui.add_space(10.0);
                });
        }

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
            ui.add_space(30.0);

            ui.vertical_centered(|ui| {
                // Front: the sentence the word was mined from, with the
                // target word highlighted in place, framed by its
                // neighboring sentences in gray.
                match &item.sentence {
                    Some(_) => {
                        let max_width = ui.available_width().min(760.0);
                        ui.set_max_width(max_width);
                        if let Some(prev) = &item.prev_text {
                            ui.label(
                                egui::RichText::new(prev)
                                    .size(17.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                            ui.add_space(6.0);
                        }
                        let accent = egui::Color32::from_rgb(80, 140, 240);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for row in &item.sentence_tokens {
                                let mut text = egui::RichText::new(&row.token.surface).size(26.0);
                                if row.word_id == item.word.id {
                                    text = text.underline().strong().color(accent);
                                }
                                ui.add(
                                    egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend),
                                );
                            }
                        });
                        if let Some(next) = &item.next_text {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(next)
                                    .size(17.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(&item.word.key.lemma)
                                .size(34.0)
                                .strong(),
                        );
                    }
                }

                if self.review.revealed {
                    ui.add_space(22.0);
                    ui.separator();
                    ui.add_space(12.0);
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

                    // The word elsewhere in the library, other books first.
                    if self.settings.review_examples && !item.examples.is_empty() {
                        ui.add_space(14.0);
                        ui.label(
                            egui::RichText::new("Elsewhere in your library")
                                .small()
                                .strong(),
                        );
                        for (sentence, title) in &item.examples {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(&sentence.text).size(16.0));
                            ui.weak(format!("— {title}"));
                        }
                    }
                }
            });
        });

        if let Some(rating) = answered {
            let word_id = self.review.queue[0].word.id;
            if self
                .with_app(|app| app.answer_review(word_id, rating))
                .is_some()
            {
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
