//! Settings view: a category rail on the left, one page per category,
//! and an always-visible save bar.

use eframe::egui;

use crate::app::ShioriGui;

/// Which settings page is open. UI state only, not persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    General,
    Appearance,
    Reading,
    Review,
    Ai,
    Shortcuts,
    Data,
}

impl SettingsCategory {
    pub const ALL: [SettingsCategory; 7] = [
        SettingsCategory::General,
        SettingsCategory::Appearance,
        SettingsCategory::Reading,
        SettingsCategory::Review,
        SettingsCategory::Ai,
        SettingsCategory::Shortcuts,
        SettingsCategory::Data,
    ];

    fn label(self) -> &'static str {
        match self {
            SettingsCategory::General => "General",
            SettingsCategory::Appearance => "Appearance",
            SettingsCategory::Reading => "Reading",
            SettingsCategory::Review => "Review",
            SettingsCategory::Ai => "AI",
            SettingsCategory::Shortcuts => "Shortcuts",
            SettingsCategory::Data => "Data",
        }
    }
}

impl ShioriGui {
    pub fn show_settings(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("settings-categories")
            .resizable(false)
            .exact_width(150.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.heading("Settings");
                ui.add_space(8.0);
                for cat in SettingsCategory::ALL {
                    if ui
                        .selectable_label(self.settings_category == cat, cat.label())
                        .clicked()
                    {
                        self.settings_category = cat;
                    }
                }
            });

        egui::TopBottomPanel::bottom("settings-save").show(ctx, |ui| {
            ui.add_space(4.0);
            let dirty = self.settings != self.settings_draft;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(dirty, egui::Button::new("💾 Save settings"))
                    .clicked()
                {
                    self.apply_settings();
                }
                if dirty {
                    if ui.button("Discard changes").clicked() {
                        self.settings_draft = self.settings.clone();
                    }
                    ui.weak("unsaved changes");
                }
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| match self.settings_category {
                    SettingsCategory::General => self.settings_general(ui),
                    SettingsCategory::Appearance => self.settings_appearance(ui),
                    SettingsCategory::Reading => self.settings_reading(ui),
                    SettingsCategory::Review => self.settings_review(ui),
                    SettingsCategory::Ai => self.settings_ai(ui),
                    SettingsCategory::Shortcuts => self.settings_shortcuts(ui),
                    SettingsCategory::Data => self.settings_data(ui),
                });
        });
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        ui.heading("Language");
        let languages = self
            .with_app(|app| Ok(app.available_languages()))
            .unwrap_or_default();
        let active = self.settings.active_language.clone();
        let active_name = languages
            .iter()
            .find(|(code, _)| *code == active)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| active.clone());
        let mut selected: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label("Active language:");
            egui::ComboBox::from_id_salt("active-language")
                .selected_text(active_name)
                .show_ui(ui, |ui| {
                    for (code, name) in &languages {
                        if ui.selectable_label(*code == active, name).clicked() {
                            selected = Some(code.clone());
                        }
                    }
                });
        });
        if languages.len() == 1 {
            ui.weak(
                "More languages install as packs: drop a pack folder into \
                 the data directory under packs\\<code>\\ and restart.",
            );
        } else {
            ui.weak(
                "Library, reviews, statistics, and chat all follow the \
                 active language; nothing mixes.",
            );
        }
        if let Some(code) = selected {
            if code != active {
                self.switch_language(&code);
            }
        }

        ui.add_space(12.0);
        ui.heading("About");
        ui.label(
            "Shiori（栞・bookmark） — comprehensible-input reading with \
             FSRS spaced repetition.",
        );
        ui.label("Dictionary: JMdict © EDRDG (via jmdict-simplified).");
        ui.label("Frequency list: Leeds corpus derived (CC BY).");
        ui.add_space(10.0);
        if ui.button("Show getting-started guide").clicked() {
            self.open_welcome();
        }
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        ui.heading("Theme");
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.settings_draft.theme,
                crate::settings::Theme::Dark,
                "Dark",
            );
            ui.selectable_value(
                &mut self.settings_draft.theme,
                crate::settings::Theme::Light,
                "Light",
            );
            ui.selectable_value(
                &mut self.settings_draft.theme,
                crate::settings::Theme::Sepia,
                "Sepia",
            );
        });

        ui.add_space(12.0);
        ui.heading("Japanese font");
        for font in [
            crate::settings::ReaderFont::System,
            crate::settings::ReaderFont::NotoSans,
            crate::settings::ReaderFont::NotoSerif,
        ] {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.settings_draft.reader_font, font, font.label());
                if !crate::fonts::font_available(&self.data_dir, font) {
                    ui.weak("~5–8 MB download on first use");
                }
            });
        }
        if self.font_downloading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("downloading font…");
            });
        }
        ui.weak("Applies everywhere Japanese is rendered, after saving.");

        ui.add_space(12.0);
        ui.heading("Reader text");
        egui::Grid::new("reader-text-grid")
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Font size:");
                ui.add(egui::Slider::new(
                    &mut self.settings_draft.reader_font_size,
                    14.0..=40.0,
                ));
                ui.end_row();
                ui.label("Line spacing:");
                ui.add(
                    egui::Slider::new(&mut self.settings_draft.reader_line_spacing, 0.6..=2.0)
                        .fixed_decimals(1),
                );
                ui.end_row();
            });
    }

    fn settings_reading(&mut self, ui: &mut egui::Ui) {
        ui.heading("Reader");
        ui.checkbox(
            &mut self.settings_draft.show_unknown_highlights,
            "Tint words I haven't studied yet",
        );
        ui.weak(
            "The selected word is always highlighted; this adds a subtle tint \
             on unknown vocabulary as well.",
        );

        ui.add_space(12.0);
        ui.heading("Furigana");
        for mode in [
            crate::settings::FuriganaMode::None,
            crate::settings::FuriganaMode::Unknown,
            crate::settings::FuriganaMode::UnknownFirstX,
            crate::settings::FuriganaMode::All,
        ] {
            ui.radio_value(&mut self.settings_draft.furigana, mode, mode.label());
        }
        ui.horizontal(|ui| {
            ui.add_enabled(
                self.settings_draft.furigana == crate::settings::FuriganaMode::UnknownFirstX,
                egui::DragValue::new(&mut self.settings_draft.furigana_first_x)
                    .range(1..=50)
                    .prefix("X = "),
            );
            ui.weak("instances of each word, counted in reading order per book");
        });
        ui.weak(
            "Readings anchor to specific occurrences in the book: the first X \
             stay annotated no matter how you flip around, the rest never are.",
        );
    }

    fn settings_review(&mut self, ui: &mut egui::Ui) {
        ui.heading("Review");
        ui.checkbox(
            &mut self.settings_draft.review_examples,
            "Show example sentences from other books on cards",
        );
        ui.weak(
            "After revealing the answer, up to three other sentences from \
             your library that use the word appear, other books first.",
        );
    }

    fn settings_ai(&mut self, ui: &mut egui::Ui) {
        use crate::settings::LlmProvider;

        ui.heading("LLM backend (optional)");
        ui.label(
            "Powers sentence explanations in the reader and conversation \
             practice in Production mode. Everything else works without it.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.settings_draft.llm_provider,
                LlmProvider::Anthropic,
                "Anthropic",
            );
            ui.selectable_value(
                &mut self.settings_draft.llm_provider,
                LlmProvider::Ollama,
                "Ollama (local)",
            );
            ui.selectable_value(
                &mut self.settings_draft.llm_provider,
                LlmProvider::Custom,
                "Custom endpoint",
            );
        });
        ui.add_space(8.0);

        let field_width = (ui.available_width() - 160.0).clamp(240.0, 520.0);
        match self.settings_draft.llm_provider {
            LlmProvider::Anthropic => {
                egui::Grid::new("llm-grid")
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("API key:");
                        ui.add_sized(
                            [field_width, 22.0],
                            egui::TextEdit::singleline(&mut self.settings_draft.anthropic_api_key)
                                .password(true)
                                .hint_text("sk-ant-…"),
                        );
                        ui.end_row();
                        ui.label("Model:");
                        ui.add_sized(
                            [field_width, 22.0],
                            egui::TextEdit::singleline(&mut self.settings_draft.llm_model),
                        );
                        ui.end_row();
                    });
                ui.weak(
                    "The key is stored locally in settings.json and only ever sent \
                     to the Anthropic API. Leave empty to use the \
                     ANTHROPIC_API_KEY environment variable instead.",
                );
            }
            LlmProvider::Ollama => self.settings_ollama(ui, field_width),
            LlmProvider::Custom => {
                egui::Grid::new("custom-llm-grid")
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Base URL:");
                        ui.add_sized(
                            [field_width, 22.0],
                            egui::TextEdit::singleline(&mut self.settings_draft.custom_url)
                                .hint_text("http://localhost:1234/v1"),
                        );
                        ui.end_row();
                        ui.label("API key:");
                        ui.add_sized(
                            [field_width, 22.0],
                            egui::TextEdit::singleline(&mut self.settings_draft.custom_api_key)
                                .password(true)
                                .hint_text("optional for local servers"),
                        );
                        ui.end_row();
                        ui.label("Model:");
                        ui.add_sized(
                            [field_width, 22.0],
                            egui::TextEdit::singleline(&mut self.settings_draft.custom_model),
                        );
                        ui.end_row();
                    });
                ui.weak(
                    "Any server speaking the OpenAI chat-completions dialect: \
                     LM Studio, llama.cpp server, vLLM, or a cloud provider.",
                );
            }
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Current backend:");
            if self.explainer.is_available() {
                ui.colored_label(
                    egui::Color32::from_rgb(110, 180, 110),
                    format!("{} (active)", self.explainer.name()),
                );
            } else {
                ui.weak("none");
            }
        });
    }

    /// Ollama section: liveness, installed models, in-app pulls.
    fn settings_ollama(&mut self, ui: &mut egui::Ui, field_width: f32) {
        egui::Grid::new("ollama-grid")
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Server URL:");
                ui.add_sized(
                    [field_width, 22.0],
                    egui::TextEdit::singleline(&mut self.settings_draft.ollama_url)
                        .hint_text(shiori_llm::DEFAULT_OLLAMA_URL),
                );
                ui.end_row();
            });

        // Probe once automatically; afterwards on demand.
        if self.ollama_probe.is_none() && !self.ollama_probing {
            self.probe_ollama(ui.ctx());
        }
        ui.horizontal(|ui| {
            ui.label("Status:");
            if self.ollama_probing {
                ui.spinner();
            } else {
                match &self.ollama_probe {
                    Some(Ok((version, models))) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(110, 180, 110),
                            format!("running (v{version}) · {} models installed", models.len()),
                        );
                    }
                    Some(Err(e)) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 90, 90),
                            format!("not reachable: {e}"),
                        );
                    }
                    None => {
                        ui.weak("unknown");
                    }
                }
            }
            if ui.small_button("⟳ refresh").clicked() {
                self.ollama_probe = None;
            }
        });
        if matches!(&self.ollama_probe, Some(Err(_))) {
            ui.weak(
                "Install Ollama from ollama.com and make sure it is running, \
                 then refresh.",
            );
        }

        if let Some(Ok((_, models))) = &self.ollama_probe {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Model:");
                let selected = if self.settings_draft.ollama_model.is_empty() {
                    "choose a model".to_string()
                } else {
                    self.settings_draft.ollama_model.clone()
                };
                egui::ComboBox::from_id_salt("ollama-model")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for m in models {
                            let label = format!(
                                "{}  ({:.1} GB{})",
                                m.model,
                                m.size as f64 / 1e9,
                                m.details
                                    .as_ref()
                                    .and_then(|d| d.parameter_size.as_deref())
                                    .map(|p| format!(", {p}"))
                                    .unwrap_or_default()
                            );
                            ui.selectable_value(
                                &mut self.settings_draft.ollama_model,
                                m.model.clone(),
                                label,
                            );
                        }
                    });
            });
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Pull model:");
            ui.add_sized(
                [200.0, 22.0],
                egui::TextEdit::singleline(&mut self.ollama_pull_input).hint_text("e.g. qwen3:8b"),
            );
            let pulling = self.ollama_pull.is_some();
            if ui
                .add_enabled(!pulling, egui::Button::new("⬇ Pull"))
                .clicked()
            {
                let model = self.ollama_pull_input.clone();
                self.pull_ollama_model(ui.ctx(), model);
            }
        });
        if let Some((status, frac)) = self.ollama_pull.clone() {
            ui.horizontal(|ui| {
                match frac {
                    Some(f) => {
                        ui.add(
                            egui::ProgressBar::new(f)
                                .desired_width(260.0)
                                .show_percentage(),
                        );
                    }
                    None => {
                        ui.spinner();
                    }
                }
                ui.weak(status);
            });
        }
        ui.weak(
            "Models run entirely on this machine; nothing leaves it. \
             Japanese-capable picks: qwen3, gemma3, llama3.1-swallow.",
        );
    }

    fn settings_shortcuts(&mut self, ui: &mut egui::Ui) {
        self.handle_shortcut_recording(ui.ctx());

        ui.heading("Keyboard shortcuts");
        ui.weak(
            "Click a binding, then press the keys (e.g. Ctrl+Shift+4). The \
             combo is set when you release; Escape cancels.",
        );
        ui.add_space(4.0);

        let recording_id = self.shortcut_recording.as_ref().map(|r| r.id);
        let mut start_recording = None;
        egui::Grid::new("shortcut-grid")
            .num_columns(3)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for (id, label) in crate::settings::Shortcuts::FIELDS {
                    ui.label(label);
                    let value = self.settings_draft.shortcuts.get(id);
                    let recording = recording_id == Some(id);
                    let text = if recording {
                        egui::RichText::new("press keys…").italics()
                    } else {
                        egui::RichText::new(value)
                    };
                    if ui
                        .add_sized([170.0, 20.0], egui::Button::new(text))
                        .clicked()
                        && !recording
                    {
                        start_recording = Some(id);
                    }
                    if !recording && !crate::settings::is_valid_key_name(value) {
                        ui.colored_label(egui::Color32::from_rgb(220, 90, 90), "invalid binding");
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        if let Some(id) = start_recording {
            self.shortcut_notice = None;
            self.shortcut_recording = Some(crate::app::ShortcutRecording {
                id,
                captured: None,
                prev_modifiers: ui.ctx().input(|i| i.modifiers),
            });
        }
        if let Some(notice) = &self.shortcut_notice {
            ui.colored_label(egui::Color32::from_rgb(230, 160, 60), notice);
        }
        ui.add_space(6.0);
        if ui.button("Reset shortcuts to defaults").clicked() {
            self.settings_draft.shortcuts = Default::default();
            self.shortcut_notice = None;
        }
    }

    /// Drive an in-progress shortcut capture from this frame's input.
    ///
    /// The binding commits when the first held key is released — either
    /// the non-modifier key (its release event carries the modifiers that
    /// were still held) or a modifier (detected as a flag dropping while
    /// a key is captured, committed with last frame's modifier state).
    fn handle_shortcut_recording(&mut self, ctx: &egui::Context) {
        let Some(rec) = &mut self.shortcut_recording else {
            return;
        };
        let (events, modifiers) = ctx.input(|i| (i.events.clone(), i.modifiers));

        // None = cancelled; Some((mods, key)) = commit.
        let mut outcome: Option<Option<(egui::Modifiers, egui::Key)>> = None;
        for event in &events {
            let egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if *pressed {
                if *key == egui::Key::Escape {
                    outcome = Some(None);
                    break;
                }
                rec.captured = Some((*modifiers, *key));
            } else if rec.captured.is_some_and(|(_, k)| k == *key) {
                outcome = Some(Some((*modifiers, *key)));
                break;
            }
        }
        if outcome.is_none() {
            let lost_modifier = (rec.prev_modifiers.ctrl && !modifiers.ctrl)
                || (rec.prev_modifiers.command && !modifiers.command)
                || (rec.prev_modifiers.alt && !modifiers.alt)
                || (rec.prev_modifiers.shift && !modifiers.shift);
            if lost_modifier {
                if let Some((_, key)) = rec.captured {
                    outcome = Some(Some((rec.prev_modifiers, key)));
                }
            }
        }
        rec.prev_modifiers = modifiers;

        let Some(result) = outcome else { return };
        let id = rec.id;
        self.shortcut_recording = None;
        if let Some((mods, key)) = result {
            let combo = crate::settings::format_shortcut(mods, key);
            if let Some(label) = self.settings_draft.shortcuts.conflict(&combo, id) {
                self.shortcut_notice = Some(format!("{combo} is already bound to \"{label}\""));
            } else {
                *self.settings_draft.shortcuts.get_mut(id) = combo;
                self.shortcut_notice = None;
            }
        }
    }

    fn settings_data(&mut self, ui: &mut egui::Ui) {
        ui.heading("Data");
        ui.label(format!("Data directory: {}", self.data_dir.display()));
        if let Some(status) = &self.data_status {
            ui.label(format!(
                "Dictionary entries: {} · frequency words: {} · kanji: {}",
                status.dict_entries, status.frequency_words, status.kanji
            ));
        }

        ui.add_space(12.0);
        ui.heading("Anki");
        ui.horizontal(|ui| {
            if ui.button("⬆ Export deck (.apkg)…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("shiori.apkg")
                    .add_filter("Anki deck", &["apkg"])
                    .save_file()
                {
                    self.run_transfer(ui.ctx(), move |app| {
                        let n = app.export_apkg(&path)?;
                        Ok(format!("exported {n} cards to {}", path.display()))
                    });
                }
            }
            if ui.button("⬇ Import deck (.apkg)…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Anki deck", &["apkg"])
                    .pick_file()
                {
                    self.run_transfer(ui.ctx(), move |app| {
                        let (imported, skipped) = app.import_apkg(&path)?;
                        Ok(format!(
                            "imported {imported} cards ({skipped} skipped — \
                             non-Japanese or already scheduled)"
                        ))
                    });
                }
            }
        });
        ui.weak(
            "Export carries approximate scheduling (FSRS → SM-2); import seeds \
             FSRS from SM-2 intervals and never overwrites existing cards.",
        );

        ui.add_space(12.0);
        ui.heading("Settings file");
        ui.horizontal(|ui| {
            if ui.button("⬆ Export settings…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("shiori-settings.json")
                    .add_filter("JSON", &["json"])
                    .save_file()
                {
                    let result = self
                        .settings
                        .save_to(&path)
                        .map(|()| format!("settings exported to {}", path.display()))
                        .map_err(|e| format!("settings export failed: {e}"));
                    match result {
                        Ok(msg) => self.notice = Some(msg),
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            if ui.button("⬇ Import settings…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match crate::settings::Settings::load_from(&path) {
                        Some(settings) => {
                            self.settings_draft = settings;
                            self.apply_settings();
                            self.notice = Some("settings imported and applied".into());
                        }
                        None => {
                            self.error = Some("that file is not a valid settings export".into())
                        }
                    }
                }
            }
        });

        ui.add_space(12.0);
        ui.heading("Database");
        ui.horizontal(|ui| {
            if ui.button("💾 Back up database…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("shiori-backup.sqlite3")
                    .add_filter("SQLite database", &["sqlite3", "db"])
                    .save_file()
                {
                    self.run_transfer(ui.ctx(), move |app| {
                        app.db().backup_to(&path)?;
                        Ok(format!("database backed up to {}", path.display()))
                    });
                }
            }
            if ui.button("↩ Restore from backup…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("SQLite database", &["sqlite3", "db"])
                    .pick_file()
                {
                    self.run_transfer(ui.ctx(), move |app| {
                        app.stage_restore(&path)?;
                        Ok("restore staged — restart the app to complete it".into())
                    });
                }
            }
        });
        ui.weak(
            "Backups are clean single-file copies, safe to take while the app \
             runs. Restoring swaps the database in on the next launch; the \
             current database is kept aside as jrc.sqlite3.pre-restore.",
        );

        ui.add_space(12.0);
        ui.weak("Kanji data: KANJIDIC2 © EDRDG (CC BY-SA 4.0).");
        ui.weak("Stroke order: KanjiVG © Ulrich Apel (CC BY-SA 3.0).");
        ui.weak("JLPT lists: stephenmk/yomitan-jlpt-vocab (CC BY-SA 4.0).");
    }
}
