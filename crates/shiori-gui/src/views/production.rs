//! Production practice: chat with a native-speaker persona. Replies
//! converse, never correct; corrections appear as paper-style underlines
//! on the user's own messages. Every message is clickable like the
//! reader, and clicking an underlined word shows the dictionary entry
//! and the write-up note together.

use eframe::egui;
use shiori_core::WordKey;

use crate::app::JrcGui;

/// Action chosen in the chat word panel.
enum ChatWordAction {
    Learn(shiori_core::WordId),
    Known(shiori_core::WordId),
    Ignore(shiori_core::WordId),
}

impl JrcGui {
    pub fn show_production(&mut self, ctx: &egui::Context) {
        if !self.production.loaded {
            self.load_conversations();
        }

        self.chat_history_panel(ctx);
        let action = self.chat_word_panel(ctx);
        self.chat_input_panel(ctx);
        let clicked = self.chat_transcript(ctx);

        if let Some((msg_idx, s_idx, g_idx)) = clicked {
            self.select_chat_word(msg_idx, s_idx, g_idx);
        }
        if let Some(action) = action {
            let result = match action {
                ChatWordAction::Learn(id) => {
                    self.with_app(|app| app.start_learning_uncontexted(id))
                }
                ChatWordAction::Known(id) => self.with_app(|app| app.mark_known(id)),
                ChatWordAction::Ignore(id) => self.with_app(|app| app.ignore_word(id)),
            };
            if result.is_some() {
                self.refresh_caches();
                // Refresh the panel's status line.
                let reload = self.production.panel.as_ref().map(|p| {
                    (p.word.id, p.phrase.clone(), p.inflection.clone(), p.compound.is_some())
                });
                if let Some((word_id, phrase, inflection, had_compound)) = reload {
                    self.production.panel =
                        self.load_word_panel(word_id, phrase, inflection, had_compound);
                }
            }
        }
    }

    /// Conversation list on the left.
    fn chat_history_panel(&mut self, ctx: &egui::Context) {
        let mut to_open: Option<i64> = None;
        let mut to_delete: Option<i64> = None;
        let mut new_conversation = false;
        egui::SidePanel::left("chat-history")
            .resizable(false)
            .exact_width(190.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                if ui
                    .add_sized([170.0, 26.0], egui::Button::new("✚ New conversation"))
                    .clicked()
                {
                    new_conversation = true;
                }
                ui.add_space(6.0);
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for conv in &self.production.conversations {
                            let title = if conv.title.is_empty() {
                                "(untitled)".to_string()
                            } else {
                                conv.title.clone()
                            };
                            ui.horizontal(|ui| {
                                let selected = self.production.current == Some(conv.id);
                                if ui
                                    .selectable_label(
                                        selected,
                                        crate::app::truncate_title(&title, 14),
                                    )
                                    .on_hover_text(format!(
                                        "{title}\n{} messages · {}",
                                        conv.message_count,
                                        conv.started_at.format("%Y-%m-%d")
                                    ))
                                    .clicked()
                                {
                                    to_open = Some(conv.id);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("🗑").clicked() {
                                            to_delete = Some(conv.id);
                                        }
                                    },
                                );
                            });
                        }
                    });
            });

        if new_conversation {
            self.production.current = None;
            self.production.messages.clear();
            self.production.panel = None;
            self.production.panel_note = None;
        }
        if let Some(id) = to_open {
            self.open_conversation(id);
        }
        if let Some(id) = to_delete {
            self.with_app(|app| Ok(app.db().delete_conversation(id)?));
            if self.production.current == Some(id) {
                self.production.current = None;
                self.production.messages.clear();
            }
            self.load_conversations();
        }
    }

    /// Right panel: dictionary entry for the clicked word plus the
    /// write-up note covering it (user messages).
    fn chat_word_panel(&mut self, ctx: &egui::Context) -> Option<ChatWordAction> {
        self.production.panel.as_ref()?;
        let mut action = None;
        let mut close = false;
        egui::SidePanel::right("chat-word-panel")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let Some(panel) = &self.production.panel else { return };
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.heading(&panel.word.key.lemma);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("✕").clicked() {
                                        close = true;
                                    }
                                },
                            );
                        });
                        if !panel.word.key.reading.is_empty() {
                            ui.label(&panel.word.key.reading);
                        }
                        ui.horizontal(|ui| {
                            ui.weak(panel.word.key.pos.as_str());
                            ui.weak("·");
                            ui.weak(format!("status: {}", panel.word.status.as_str()));
                        });

                        if let Some(note) = &self.production.panel_note {
                            ui.add_space(6.0);
                            egui::Frame::group(ui.style())
                                .fill(egui::Color32::from_rgba_unmultiplied(200, 120, 60, 26))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Write-up").strong());
                                    ui.label(note);
                                });
                        }

                        if !panel.inflection.is_plain() {
                            if let Some(summary) = &panel.inflection.summary {
                                ui.add_space(4.0);
                                ui.weak(summary);
                            }
                        }

                        ui.add_space(6.0);
                        match &panel.entry {
                            Some(entry) => {
                                for (i, sense) in entry.senses.iter().take(5).enumerate() {
                                    let glosses: Vec<&str> =
                                        sense.gloss.iter().map(|g| g.text.as_str()).collect();
                                    if !glosses.is_empty() {
                                        ui.label(format!("{}. {}", i + 1, glosses.join("; ")));
                                    }
                                }
                            }
                            None => {
                                if self.dict_ready() {
                                    ui.weak("No dictionary entry found.");
                                } else {
                                    ui.weak("No dictionary installed.");
                                }
                            }
                        }

                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            use shiori_core::KnowledgeStatus;
                            if panel.word.status != KnowledgeStatus::Learning
                                && ui.button("➕ Learn").clicked()
                            {
                                action = Some(ChatWordAction::Learn(panel.word.id));
                            }
                            if panel.word.status != KnowledgeStatus::Known
                                && ui.button("✔ Known").clicked()
                            {
                                action = Some(ChatWordAction::Known(panel.word.id));
                            }
                            if panel.word.status != KnowledgeStatus::Ignored
                                && ui.button("🚫 Ignore").clicked()
                            {
                                action = Some(ChatWordAction::Ignore(panel.word.id));
                            }
                        });
                    });
            });
        if close {
            self.production.panel = None;
            self.production.panel_note = None;
        }
        action
    }

    /// Input box, send button, and the challenge dial at the bottom.
    fn chat_input_panel(&mut self, ctx: &egui::Context) {
        let mut send = false;
        egui::TopBottomPanel::bottom("chat-input").show(ctx, |ui| {
            ui.add_space(6.0);
            if !self.explainer.is_available() {
                ui.weak(
                    "Conversation practice needs an LLM backend — configure one \
                     under Settings → AI (Ollama runs fully offline).",
                );
                ui.add_space(6.0);
                return;
            }
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [ui.available_width() - 170.0, 54.0],
                    egui::TextEdit::multiline(&mut self.production.input)
                        .hint_text("日本語で書いてみてください…")
                        .desired_rows(2),
                );
                // Enter sends; Shift+Enter inserts the newline.
                if response.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                {
                    self.production.input =
                        self.production.input.trim_end_matches('\n').to_string();
                    send = true;
                }
                ui.vertical(|ui| {
                    let can_send = !self.production.waiting
                        && !self.production.input.trim().is_empty();
                    if ui
                        .add_enabled(can_send, egui::Button::new("Send ➤"))
                        .clicked()
                    {
                        send = true;
                    }
                    if self.production.waiting {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("typing…");
                        });
                    }
                    let mut challenge = self.settings.chat_challenge;
                    egui::ComboBox::from_id_salt("chat-challenge")
                        .selected_text(challenge.label())
                        .width(140.0)
                        .show_ui(ui, |ui| {
                            for c in [
                                crate::settings::ChatChallenge::Match,
                                crate::settings::ChatChallenge::Push,
                                crate::settings::ChatChallenge::Immerse,
                            ] {
                                ui.selectable_value(&mut challenge, c, c.label());
                            }
                        });
                    if challenge != self.settings.chat_challenge {
                        self.settings.chat_challenge = challenge;
                        self.settings_draft.chat_challenge = challenge;
                        let _ = self.settings.save(&self.data_dir);
                    }
                });
            });
            ui.add_space(6.0);
        });

        if send {
            self.send_chat_message(ctx);
        }
    }

    /// The transcript. Returns (message, sentence, group) of a clicked
    /// phrase.
    fn chat_transcript(&mut self, ctx: &egui::Context) -> Option<(usize, usize, usize)> {
        let mut clicked = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.production.messages.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Conversation practice");
                    ui.label(
                        "Chat in Japanese with a native-speaker persona. It talks \
                         with you — it never corrects you mid-conversation.",
                    );
                    ui.label(
                        "Anything wrong or unnatural in your messages gets \
                         underlined afterwards, like a marked-up paper: red for \
                         errors, orange for clunky phrasing. Hover or click an \
                         underline to see why.",
                    );
                    ui.add_space(8.0);
                    ui.weak("Click any word in the chat to look it up, just like the reader.");
                });
                return;
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    for (m_idx, message) in self.production.messages.iter().enumerate() {
                        let is_user = message.role == "user";
                        let max_bubble = ui.available_width() * 0.8;
                        let layout = if is_user {
                            egui::Layout::right_to_left(egui::Align::TOP)
                        } else {
                            egui::Layout::left_to_right(egui::Align::TOP)
                        };
                        ui.with_layout(layout, |ui| {
                            let fill = if is_user {
                                ui.visuals().faint_bg_color
                            } else {
                                ui.visuals().extreme_bg_color
                            };
                            egui::Frame::group(ui.style())
                                .fill(fill)
                                .inner_margin(egui::Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.set_max_width(max_bubble);
                                    if let Some(c) =
                                        chat_message_body(ui, message, m_idx)
                                    {
                                        clicked = Some(c);
                                    }
                                });
                        });
                        ui.add_space(8.0);
                    }
                    if self.production.waiting {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.weak("…");
                        });
                    }
                });
        });
        clicked
    }

    /// Resolve a clicked phrase group to a word panel + write-up note.
    fn select_chat_word(&mut self, m_idx: usize, s_idx: usize, g_idx: usize) {
        let Some(message) = self.production.messages.get(m_idx) else { return };
        let Some((tokens, groups)) = message.sentences.get(s_idx) else { return };
        let Some(&(start, end)) = groups.get(g_idx) else { return };
        let group: Vec<shiori_core::Token> =
            tokens[start..end].iter().map(|r| r.token.clone()).collect();
        if group.is_empty() {
            return;
        }
        let phrase: String = group.iter().map(|t| t.surface.as_str()).collect();
        let inflection = shiori_nlp::analyze_inflection(&group);
        // The clicked group's head token decides the word.
        let head = &tokens[start];
        let key = WordKey {
            lemma: head.token.lemma.clone(),
            reading: head.token.reading.clone(),
            pos: head.token.pos,
        };
        let try_compound = group.len() > 1
            && matches!(
                group[0].pos,
                shiori_core::PartOfSpeech::Noun
                    | shiori_core::PartOfSpeech::ProperNoun
                    | shiori_core::PartOfSpeech::AdjectivalNoun
                    | shiori_core::PartOfSpeech::Prefix
            );
        // The write-up note overlapping the clicked phrase, if any.
        let phrase_start = head.token.start;
        let phrase_end = tokens[end - 1].token.end;
        let note = message
            .annotations
            .iter()
            .find(|a| a.start < phrase_end && phrase_start < a.end)
            .map(|a| a.note.clone());

        let Some(word) = self.with_app(|app| app.ensure_word(&key)) else { return };
        self.production.panel = self.load_word_panel(word.id, phrase, inflection, try_compound);
        self.production.panel_note = note;
    }
}

/// Render one message's tokens with annotation underlines; returns a
/// clicked (message, sentence, group) triple.
fn chat_message_body(
    ui: &mut egui::Ui,
    message: &crate::app::ChatMessageView,
    m_idx: usize,
) -> Option<(usize, usize, usize)> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.spacing_mut().item_spacing.y = 6.0;
        for (s_idx, (tokens, groups)) in message.sentences.iter().enumerate() {
            for (t_idx, row) in tokens.iter().enumerate() {
                let japanese = shiori_nlp::kana::is_japanese(&row.token.surface);
                let text = egui::RichText::new(&row.token.surface).size(17.0);
                let label = egui::Label::new(text)
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .sense(if japanese {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    });
                let response = ui.add(label);

                // Paper-style underline where the write-up flagged this
                // span (user messages only carry annotations).
                let covering = message
                    .annotations
                    .iter()
                    .find(|a| a.start < row.token.end && row.token.start < a.end);
                if let Some(annotation) = covering {
                    let color = if annotation.severity == "error" {
                        egui::Color32::from_rgb(220, 90, 90)
                    } else {
                        egui::Color32::from_rgb(230, 160, 60)
                    };
                    let rect = response.rect;
                    ui.painter().line_segment(
                        [
                            egui::pos2(rect.left(), rect.bottom() - 1.0),
                            egui::pos2(rect.right(), rect.bottom() - 1.0),
                        ],
                        egui::Stroke::new(2.0, color),
                    );
                    response.clone().on_hover_text(&annotation.note);
                }

                if japanese {
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        let group = groups
                            .iter()
                            .position(|(s, e)| (*s..*e).contains(&t_idx));
                        if let Some(g_idx) = group {
                            clicked = Some((m_idx, s_idx, g_idx));
                        }
                    }
                }
            }
        }
    });
    clicked
}
