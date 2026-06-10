//! Reader view: flowing paragraphs of clickable text with a dictionary
//! side panel for the selected phrase.
//!
//! Phrases (conjugated verbs with their endings, noun+suffix compounds)
//! are selected as a unit, highlighted in a single color, and the panel
//! explains the conjugation. No permanent per-status tinting — an optional
//! settings toggle can mark unknown words.

use eframe::egui;
use jrc_core::{KnowledgeStatus, WordId};
use jrc_dict::register::UsageProfile;

use crate::app::JrcGui;
use crate::settings::shortcut_pressed;
use crate::views::{tight_highlight_rect, unknown_fill};

const READER_FONT_SIZE: f32 = 21.0;

/// Action chosen in the dictionary panel.
enum WordAction {
    Learn(WordId, jrc_core::SentenceId),
    Known(WordId),
    Ignore(WordId),
    Reset(WordId),
    Forgot(WordId, Option<jrc_core::SentenceId>),
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
        let mut clicked: Option<(usize, usize)> = None; // (sentence, token)
        let mut explain_requested = false;

        // Keyboard shortcuts (ignored while a text field has focus).
        let shortcuts = self.settings.shortcuts.clone();
        if shortcut_pressed(ctx, &shortcuts.reader_next) {
            self.navigate_selection(1);
        } else if shortcut_pressed(ctx, &shortcuts.reader_prev) {
            self.navigate_selection(-1);
        }
        if let Some(reader) = self.reader.as_ref() {
            if let Some(panel) = &reader.panel {
                let sentence_id = reader
                    .selected
                    .and_then(|(s, _)| reader.sentences.get(s))
                    .map(|v| v.sentence.id);
                if shortcut_pressed(ctx, &shortcuts.reader_learn)
                    && panel.word.status != KnowledgeStatus::Learning
                {
                    if let Some(sid) = sentence_id {
                        action = Some(WordAction::Learn(panel.word.id, sid));
                    }
                }
                if shortcut_pressed(ctx, &shortcuts.reader_known)
                    && panel.word.status != KnowledgeStatus::Known
                {
                    action = Some(WordAction::Known(panel.word.id));
                }
                if shortcut_pressed(ctx, &shortcuts.reader_ignore)
                    && panel.word.status != KnowledgeStatus::Ignored
                {
                    action = Some(WordAction::Ignore(panel.word.id));
                }
                if shortcut_pressed(ctx, &shortcuts.reader_explain)
                    && self.explainer.is_available()
                    && !reader.explaining
                {
                    explain_requested = true;
                }
            }
        }

        egui::SidePanel::right("dict-panel")
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                action = self.dictionary_panel(ui, &mut explain_requested);
            });

        let show_unknown = self.settings.show_unknown_highlights;
        egui::CentralPanel::default().show(ctx, |ui| {
            let reader = self.reader.as_ref().unwrap();
            ui.heading(&reader.doc.title);
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    // Render each paragraph as one flowing wrapped block.
                    let mut s_idx = 0;
                    while s_idx < reader.sentences.len() {
                        let paragraph = reader.sentences[s_idx].sentence.paragraph;
                        let para_end = reader.sentences[s_idx..]
                            .iter()
                            .position(|v| v.sentence.paragraph != paragraph)
                            .map(|off| s_idx + off)
                            .unwrap_or(reader.sentences.len());

                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.spacing_mut().item_spacing.y = 8.0;
                            let selection_fill = ui.visuals().selection.bg_fill;
                            let unknown_fill = unknown_fill(ui.visuals());
                            for si in s_idx..para_end {
                                let view = &reader.sentences[si];
                                for (ti, row) in view.tokens.iter().enumerate() {
                                    let group = reader.group_of(si, ti);
                                    let selected = match (reader.selected, group) {
                                        (Some((ss, sg)), Some(g)) => ss == si && sg == g,
                                        _ => false,
                                    };
                                    let fill = if selected {
                                        Some(selection_fill)
                                    } else if show_unknown
                                        && row.status == KnowledgeStatus::Unknown
                                        && row.token.is_content_word()
                                    {
                                        Some(unknown_fill)
                                    } else {
                                        None
                                    };
                                    let text = egui::RichText::new(&row.token.surface)
                                        .size(READER_FONT_SIZE);
                                    let clickable =
                                        jrc_nlp::kana::is_japanese(&row.token.surface);
                                    // Reserve a paint slot *under* the text,
                                    // then fill it once the rect is known —
                                    // tight around the glyphs, opaque so
                                    // adjacent tokens never double up.
                                    let bg_slot = ui.painter().add(egui::Shape::Noop);
                                    let response = ui.add(
                                        egui::Label::new(text).sense(if clickable {
                                            egui::Sense::click()
                                        } else {
                                            egui::Sense::hover()
                                        }),
                                    );
                                    if let Some(fill) = fill {
                                        let rect = tight_highlight_rect(
                                            response.rect,
                                            READER_FONT_SIZE,
                                        );
                                        ui.painter().set(
                                            bg_slot,
                                            egui::Shape::rect_filled(rect, 0.0, fill),
                                        );
                                    }
                                    if clickable {
                                        let response = response
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                        if response.clicked() {
                                            clicked = Some((si, ti));
                                        }
                                    }
                                }
                            }
                        });
                        ui.add_space(14.0);
                        s_idx = para_end;
                    }
                    ui.add_space(20.0);
                });
        });

        if let Some((s_idx, t_idx)) = clicked {
            self.select_token(s_idx, t_idx);
        }
        if explain_requested {
            self.request_explanation(ctx);
        }
        if let Some(action) = action {
            self.apply_word_action(action);
        }
    }

    /// Move the selection to the next/previous selectable phrase group.
    fn navigate_selection(&mut self, delta: isize) {
        let Some(reader) = self.reader.as_ref() else { return };
        // Ordered list of (sentence, group, first-token) for every group
        // that contains Japanese.
        let mut selectable: Vec<(usize, usize, usize)> = Vec::new();
        for (s, groups) in reader.groups.iter().enumerate() {
            for (g, (start, end)) in groups.iter().enumerate() {
                let has_japanese = reader.sentences[s].tokens[*start..*end]
                    .iter()
                    .any(|r| jrc_nlp::kana::is_japanese(&r.token.surface));
                if has_japanese {
                    selectable.push((s, g, *start));
                }
            }
        }
        if selectable.is_empty() {
            return;
        }
        let current = reader
            .selected
            .and_then(|(ss, sg)| selectable.iter().position(|(s, g, _)| *s == ss && *g == sg));
        let next = match current {
            Some(pos) => (pos as isize + delta).rem_euclid(selectable.len() as isize) as usize,
            None => {
                if delta >= 0 {
                    0
                } else {
                    selectable.len() - 1
                }
            }
        };
        let (s, _, t) = selectable[next];
        self.select_token(s, t);
    }

    /// Select the phrase group containing a clicked token and load the
    /// panel for that token's word.
    fn select_token(&mut self, s_idx: usize, t_idx: usize) {
        let Some(reader) = self.reader.as_ref() else { return };
        let Some(g_idx) = reader.group_of(s_idx, t_idx) else { return };
        let (start, end) = reader.groups[s_idx][g_idx];
        let view = &reader.sentences[s_idx];

        let group_tokens: Vec<jrc_core::Token> = view.tokens[start..end]
            .iter()
            .map(|r| r.token.clone())
            .collect();
        let phrase: String = group_tokens.iter().map(|t| t.surface.as_str()).collect();
        let inflection = jrc_nlp::analyze_inflection(&group_tokens);
        let word_id = view.tokens[t_idx].word_id;

        let panel = self.load_word_panel(word_id, phrase, inflection);
        if let Some(reader) = self.reader.as_mut() {
            reader.selected = Some((s_idx, g_idx));
            reader.panel = panel;
            reader.explanation = None;
        }
    }

    fn apply_word_action(&mut self, action: WordAction) {
        let result = match action {
            WordAction::Learn(word, sentence) => {
                self.with_app(|app| app.start_learning(word, sentence))
            }
            WordAction::Known(word) => self.with_app(|app| app.mark_known(word)),
            WordAction::Ignore(word) => self.with_app(|app| app.ignore_word(word)),
            WordAction::Reset(word) => self.with_app(|app| app.reset_word(word)),
            WordAction::Forgot(word, sentence) => {
                self.with_app(|app| app.mark_forgotten(word, sentence))
            }
        };
        if result.is_some() {
            self.refresh_reader_tokens();
            self.refresh_caches();
            // Refresh the panel so the status line is current.
            let current = self.reader.as_ref().and_then(|r| {
                r.panel
                    .as_ref()
                    .map(|p| (p.word.id, p.phrase.clone(), p.inflection.clone()))
            });
            if let Some((word_id, phrase, inflection)) = current {
                let panel = self.load_word_panel(word_id, phrase, inflection);
                if let Some(reader) = self.reader.as_mut() {
                    reader.panel = panel;
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
                    ui.weak("Click any word in the text to look it up.");
                    ui.add_space(6.0);
                    ui.weak(
                        "Conjugated verbs are selected with their endings, and the \
                         panel explains the form.",
                    );
                    return;
                };

                // Headword with furigana above it.
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let reading = &panel.word.key.reading;
                    if !reading.is_empty() && reading != &panel.word.key.lemma {
                        ui.label(egui::RichText::new(reading).size(13.0).weak());
                    }
                    ui.label(
                        egui::RichText::new(&panel.word.key.lemma).size(30.0).strong(),
                    );
                });
                if panel.phrase != panel.word.key.lemma {
                    ui.label(format!("in text: {}", panel.phrase));
                }
                ui.horizontal(|ui| {
                    ui.label(panel.word.key.pos.as_str());
                    ui.label("·");
                    ui.label(format!("status: {}", panel.word.status.as_str()));
                });
                if let Some(rank) = panel.rank {
                    ui.label(format!("corpus frequency rank: #{rank}"));
                }

                // Conjugation/form information.
                if !panel.inflection.is_plain() {
                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Form").strong());
                        if let Some(summary) = &panel.inflection.summary {
                            ui.label(summary);
                        }
                        for part in &panel.inflection.parts {
                            ui.weak(part);
                        }
                    });
                }
                ui.add_space(6.0);

                match &panel.entry {
                    Some(entry) => {
                        let profile = UsageProfile::from_misc_codes(entry.misc_codes());
                        if !profile.is_neutral() || !profile.notes.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for reg in &profile.registers {
                                    ui.label(
                                        egui::RichText::new(reg.label()).small().background_color(
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
                    if panel.word.status == KnowledgeStatus::Known {
                        if ui
                            .button("↺ Forgot this")
                            .on_hover_text("Put it back into review rotation")
                            .clicked()
                        {
                            action = Some(WordAction::Forgot(panel.word.id, sentence_id));
                        }
                    } else if ui.button("✔ Known").clicked() {
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
                    ui.weak("Add an API key in Settings to enable LLM explanations.");
                }
            });

        action
    }
}
