//! Reader view: the text with tokens tinted by knowledge status, plus a
//! dictionary side panel for the selected word.

use eframe::egui;
use jrc_core::{KnowledgeStatus, WordId};
use jrc_dict::register::UsageProfile;

use crate::app::JrcGui;
use crate::views::status_fill;

/// Action chosen in the dictionary panel.
enum WordAction {
    Learn(WordId, jrc_core::SentenceId),
    Known(WordId),
    Ignore(WordId),
    Reset(WordId),
}

impl JrcGui {
    pub fn show_reader(&mut self, ctx: &egui::Context) {
        if self.reader.is_none() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.weak("Open a document from the library to start reading.");
            });
            return;
        }

        let mut action: Option<WordAction> = None;
        let mut clicked_token: Option<(usize, usize)> = None;
        let mut explain_requested = false;

        egui::SidePanel::right("dict-panel")
            .resizable(true)
            .default_width(330.0)
            .show(ctx, |ui| {
                action = self.dictionary_panel(ui, &mut explain_requested);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let reader = self.reader.as_ref().unwrap();
            ui.heading(&reader.doc.title);
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let mut last_paragraph = u32::MAX;
                    for (s_idx, view) in reader.sentences.iter().enumerate() {
                        if view.sentence.paragraph != last_paragraph {
                            if last_paragraph != u32::MAX {
                                ui.add_space(10.0);
                            }
                            last_paragraph = view.sentence.paragraph;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for (t_idx, row) in view.tokens.iter().enumerate() {
                                let selected = reader.selected == Some((s_idx, t_idx));
                                let mut text = egui::RichText::new(&row.token.surface).size(20.0);
                                if let Some(fill) =
                                    status_fill(row.status, row.token.is_content_word())
                                {
                                    text = text.background_color(fill);
                                }
                                if selected {
                                    text = text
                                        .underline()
                                        .background_color(egui::Color32::from_rgba_unmultiplied(
                                            160, 120, 240, 90,
                                        ));
                                }
                                let response = ui.add(
                                    egui::Label::new(text).sense(egui::Sense::click()),
                                );
                                if response.clicked() && row.token.is_content_word() {
                                    clicked_token = Some((s_idx, t_idx));
                                }
                                if row.token.is_content_word() {
                                    response.on_hover_text(format!(
                                        "{}（{}）",
                                        row.token.lemma, row.token.reading
                                    ));
                                }
                            }
                        });
                    }
                    ui.add_space(20.0);
                });
        });

        if let Some((s_idx, t_idx)) = clicked_token {
            let word_id = self
                .reader
                .as_ref()
                .and_then(|r| r.sentences.get(s_idx))
                .and_then(|v| v.tokens.get(t_idx))
                .map(|row| row.word_id);
            if let Some(word_id) = word_id {
                let panel = self.load_word_panel(word_id);
                if let Some(reader) = self.reader.as_mut() {
                    reader.selected = Some((s_idx, t_idx));
                    reader.panel = panel;
                    reader.explanation = None;
                }
            }
        }

        if explain_requested {
            self.request_explanation(ctx);
        }

        if let Some(action) = action {
            let result = match action {
                WordAction::Learn(word, sentence) => {
                    self.with_app(|app| app.start_learning(word, sentence))
                }
                WordAction::Known(word) => self.with_app(|app| app.mark_known(word)),
                WordAction::Ignore(word) => self.with_app(|app| app.ignore_word(word)),
                WordAction::Reset(word) => self.with_app(|app| app.reset_word(word)),
            };
            if result.is_some() {
                self.refresh_reader_tokens();
                self.refresh_caches();
                // Refresh the panel so the status line is current.
                let word_id = self
                    .reader
                    .as_ref()
                    .and_then(|r| r.panel.as_ref())
                    .map(|p| p.word.id);
                if let Some(word_id) = word_id {
                    let panel = self.load_word_panel(word_id);
                    if let Some(reader) = self.reader.as_mut() {
                        reader.panel = panel;
                    }
                }
            }
        }
    }

    fn dictionary_panel(
        &mut self,
        ui: &mut egui::Ui,
        explain_requested: &mut bool,
    ) -> Option<WordAction> {
        let mut action = None;
        let reader = self.reader.as_ref()?;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let Some(panel) = &reader.panel else {
                    ui.add_space(8.0);
                    ui.weak("Click a highlighted word in the text to look it up.");
                    ui.add_space(12.0);
                    legend(ui);
                    return;
                };

                ui.add_space(4.0);
                ui.label(egui::RichText::new(&panel.word.key.lemma).size(30.0).strong());
                if !panel.word.key.reading.is_empty()
                    && panel.word.key.reading != panel.word.key.lemma
                {
                    ui.label(egui::RichText::new(format!("（{}）", panel.word.key.reading)).size(18.0));
                }
                ui.horizontal(|ui| {
                    ui.label(panel.word.key.pos.as_str());
                    ui.label("·");
                    ui.label(format!("status: {}", panel.word.status.as_str()));
                });
                if let Some(rank) = panel.rank {
                    ui.label(format!("corpus frequency rank: #{rank}"));
                }
                ui.add_space(6.0);

                match &panel.entry {
                    Some(entry) => {
                        // Register / nuance chips.
                        let profile = UsageProfile::from_misc_codes(entry.misc_codes());
                        if !profile.is_neutral() || !profile.notes.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for reg in &profile.registers {
                                    ui.label(
                                        egui::RichText::new(reg.label())
                                            .small()
                                            .background_color(
                                                egui::Color32::from_rgba_unmultiplied(
                                                    200, 120, 60, 70,
                                                ),
                                            ),
                                    );
                                }
                                for note in &profile.notes {
                                    ui.label(
                                        egui::RichText::new(note).small().background_color(
                                            egui::Color32::from_rgba_unmultiplied(
                                                100, 140, 100, 60,
                                            ),
                                        ),
                                    );
                                }
                            });
                            ui.add_space(4.0);
                        }

                        for (i, sense) in entry.senses.iter().enumerate() {
                            let glosses: Vec<&str> =
                                sense.gloss.iter().map(|g| g.text.as_str()).collect();
                            if glosses.is_empty() {
                                continue;
                            }
                            ui.label(format!("{}. {}", i + 1, glosses.join("; ")));
                            if !sense.misc.is_empty() {
                                ui.weak(format!("   [{}]", sense.misc.join(", ")));
                            }
                            if !sense.info.is_empty() {
                                ui.weak(format!("   {}", sense.info.join("; ")));
                            }
                        }

                        let related = entry.related_words();
                        if !related.is_empty() {
                            ui.add_space(4.0);
                            ui.label(format!("see also: {}", related.join("、")));
                        }
                        let antonyms = entry.antonyms();
                        if !antonyms.is_empty() {
                            ui.label(format!("antonyms: {}", antonyms.join("、")));
                        }
                    }
                    None => {
                        ui.weak("No dictionary entry found for this word.");
                    }
                }

                ui.add_space(10.0);
                ui.separator();

                let sentence_id = reader
                    .selected
                    .and_then(|(s, _)| reader.sentences.get(s))
                    .map(|v| v.sentence.id);
                ui.horizontal_wrapped(|ui| {
                    if panel.word.status != KnowledgeStatus::Learning {
                        if let Some(sid) = sentence_id {
                            if ui.button("➕ Learn (SRS)").clicked() {
                                action = Some(WordAction::Learn(panel.word.id, sid));
                            }
                        }
                    }
                    if panel.word.status != KnowledgeStatus::Known
                        && ui.button("✔ Known").clicked()
                    {
                        action = Some(WordAction::Known(panel.word.id));
                    }
                    if panel.word.status != KnowledgeStatus::Ignored
                        && ui.button("🚫 Ignore").clicked()
                    {
                        action = Some(WordAction::Ignore(panel.word.id));
                    }
                    if panel.word.status != KnowledgeStatus::Unknown
                        && ui.button("Reset").clicked()
                    {
                        action = Some(WordAction::Reset(panel.word.id));
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Sentence explanation").strong());
                if self.explainer.is_available() {
                    if reader.explaining {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("asking the tutor…");
                        });
                    } else if ui.button("Explain this sentence").clicked() {
                        *explain_requested = true;
                    }
                    if let Some(explanation) = &reader.explanation {
                        ui.add_space(4.0);
                        ui.label(explanation);
                    }
                } else {
                    ui.weak("Set ANTHROPIC_API_KEY and restart to enable LLM explanations.");
                }
            });

        action
    }
}

fn legend(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Legend").strong());
    for (status, label) in [
        (KnowledgeStatus::Unknown, "unknown — not yet studied"),
        (KnowledgeStatus::Learning, "learning — in the SRS"),
        (KnowledgeStatus::Known, "known — no highlight"),
        (KnowledgeStatus::Ignored, "ignored"),
    ] {
        ui.horizontal(|ui| {
            let mut sample = egui::RichText::new("　例　");
            if let Some(fill) = status_fill(status, true) {
                sample = sample.background_color(fill);
            }
            ui.label(sample);
            ui.label(label);
        });
    }
}
