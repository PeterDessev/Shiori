//! Mining view: unknown words of a document ranked by usefulness.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use jrc_core::{SentenceId, WordId};

use crate::app::JrcGui;

enum MineAction {
    Learn(WordId, SentenceId),
    Known(WordId),
    Ignore(WordId),
}

impl JrcGui {
    pub fn show_mining(&mut self, ctx: &egui::Context) {
        let mut action: Option<MineAction> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.mining.doc_id.is_none() {
                ui.weak("Pick a document in the library and press “Mine”.");
                return;
            }
            ui.heading(format!("Vocabulary mining — {}", self.mining.doc_title));
            ui.label(format!(
                "{} unknown words, most useful first (corpus frequency × occurrences here).",
                self.mining.candidates.len()
            ));
            if self.mining.candidates.is_empty() {
                ui.add_space(10.0);
                ui.weak("Nothing left to mine here — go read!");
                return;
            }
            ui.add_space(6.0);

            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::auto().at_least(120.0).clip(true)) // word
                        .column(Column::auto().at_least(170.0).clip(true)) // meaning
                        .column(Column::auto().at_least(44.0)) // here
                        .column(Column::auto().at_least(60.0)) // corpus
                        .column(Column::remainder().at_least(200.0).clip(true)) // context
                        .column(Column::auto().at_least(170.0)) // actions
                        .header(24.0, |mut row| {
                            for label in ["Word", "Meaning", "Here", "Corpus", "Context", ""] {
                                row.col(|ui| {
                                    ui.strong(label);
                                });
                            }
                        })
                        .body(|mut body| {
                            for candidate in &self.mining.candidates {
                                body.row(28.0, |mut row| {
                                    row.col(|ui| {
                                        let word = if candidate.word.key.reading.is_empty()
                                            || candidate.word.key.reading
                                                == candidate.word.key.lemma
                                        {
                                            candidate.word.key.lemma.clone()
                                        } else {
                                            format!(
                                                "{}（{}）",
                                                candidate.word.key.lemma,
                                                candidate.word.key.reading
                                            )
                                        };
                                        ui.label(egui::RichText::new(word).size(17.0));
                                    });
                                    row.col(|ui| {
                                        let gloss = candidate
                                            .entry
                                            .as_ref()
                                            .map(|e| e.short_gloss())
                                            .unwrap_or_default();
                                        if gloss.is_empty() {
                                            ui.weak("—");
                                        } else {
                                            ui.label(&gloss).on_hover_text(gloss.clone());
                                        }
                                    });
                                    row.col(|ui| {
                                        ui.label(format!("×{}", candidate.occurrences));
                                    });
                                    row.col(|ui| {
                                        ui.label(
                                            candidate
                                                .corpus_rank
                                                .map(|r| format!("#{r}"))
                                                .unwrap_or_else(|| "—".into()),
                                        );
                                    });
                                    row.col(|ui| {
                                        ui.label(&candidate.sentence.text)
                                            .on_hover_text(&candidate.sentence.text);
                                    });
                                    row.col(|ui| {
                                        if ui.button("Learn").clicked() {
                                            action = Some(MineAction::Learn(
                                                candidate.word.id,
                                                candidate.sentence.id,
                                            ));
                                        }
                                        if ui.button("Known").clicked() {
                                            action =
                                                Some(MineAction::Known(candidate.word.id));
                                        }
                                        if ui.button("Ignore").clicked() {
                                            action =
                                                Some(MineAction::Ignore(candidate.word.id));
                                        }
                                    });
                                });
                            }
                        });
                });
        });

        if let Some(action) = action {
            let done = match action {
                MineAction::Learn(word, sentence) => {
                    self.with_app(|app| app.start_learning(word, sentence))
                }
                MineAction::Known(word) => self.with_app(|app| app.mark_known(word)),
                MineAction::Ignore(word) => self.with_app(|app| app.ignore_word(word)),
            };
            if done.is_some() {
                self.reload_mining();
                self.refresh_reader_tokens();
                self.refresh_caches();
            }
        }
    }
}
