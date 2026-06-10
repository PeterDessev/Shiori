//! Mining view: unknown words of a document ranked by usefulness.

use eframe::egui;
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
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    egui::Grid::new("mining-grid")
                        .striped(true)
                        .num_columns(6)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            ui.strong("Word");
                            ui.strong("Meaning");
                            ui.strong("Here");
                            ui.strong("Corpus");
                            ui.strong("Context");
                            ui.strong("");
                            ui.end_row();

                            for candidate in &self.mining.candidates {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(&candidate.word.key.lemma).size(20.0),
                                    );
                                    if !candidate.word.key.reading.is_empty() {
                                        ui.weak(&candidate.word.key.reading);
                                    }
                                });
                                let gloss = candidate
                                    .entry
                                    .as_ref()
                                    .map(|e| e.short_gloss())
                                    .unwrap_or_default();
                                ui.add(
                                    egui::Label::new(if gloss.is_empty() {
                                        "—".to_string()
                                    } else {
                                        gloss
                                    })
                                    .wrap(),
                                );
                                ui.label(format!("×{}", candidate.occurrences));
                                ui.label(
                                    candidate
                                        .corpus_rank
                                        .map(|r| format!("#{r}"))
                                        .unwrap_or_else(|| "—".into()),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&candidate.sentence.text).size(15.0),
                                    )
                                    .wrap(),
                                );
                                ui.horizontal(|ui| {
                                    if ui.button("Learn").clicked() {
                                        action = Some(MineAction::Learn(
                                            candidate.word.id,
                                            candidate.sentence.id,
                                        ));
                                    }
                                    if ui.button("Known").clicked() {
                                        action = Some(MineAction::Known(candidate.word.id));
                                    }
                                    if ui.button("Ignore").clicked() {
                                        action = Some(MineAction::Ignore(candidate.word.id));
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    if self.mining.candidates.is_empty() {
                        ui.add_space(10.0);
                        ui.weak("Nothing left to mine here — go read!");
                    }
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
