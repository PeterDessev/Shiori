//! Settings view: a category rail on the left, one page per category,
//! and an always-visible save bar.

use eframe::egui;

use crate::app::JrcGui;

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

impl JrcGui {
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
        ui.heading("About");
        ui.label(
            "Japanese Reading Companion — comprehensible-input reading with \
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
    }

    fn settings_review(&mut self, ui: &mut egui::Ui) {
        ui.heading("Review");
        ui.weak("Review options will appear here.");
    }

    fn settings_ai(&mut self, ui: &mut egui::Ui) {
        ui.heading("LLM backend (optional)");
        ui.label(
            "Powers sentence explanations in the reader and conversation \
             practice in Production mode. Everything else works without it.",
        );
        ui.add_space(6.0);
        let field_width = (ui.available_width() - 160.0).clamp(240.0, 520.0);
        egui::Grid::new("llm-grid").spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label("Anthropic API key:");
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
            "The key is stored locally in settings.json and only ever sent to \
             the Anthropic API. Leave empty to use the ANTHROPIC_API_KEY \
             environment variable instead.",
        );
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

    fn settings_shortcuts(&mut self, ui: &mut egui::Ui) {
        ui.heading("Keyboard shortcuts");
        ui.weak(
            "Key names: letters and digits (K, 1), Space, Enter, ArrowLeft, \
             ArrowRight, ArrowUp, ArrowDown, Tab, F1–F12.",
        );
        ui.add_space(4.0);
        egui::Grid::new("shortcut-grid")
            .num_columns(4)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                let sc = &mut self.settings_draft.shortcuts;
                let field = |ui: &mut egui::Ui, label: &str, value: &mut String| {
                    ui.label(label);
                    ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(value));
                    if crate::settings::is_valid_key_name(value) {
                        ui.label("");
                    } else {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 90, 90),
                            "unknown key",
                        );
                    }
                };
                field(ui, "Review · show answer", &mut sc.review_reveal);
                ui.end_row();
                field(ui, "Review · correct", &mut sc.review_correct);
                ui.end_row();
                field(ui, "Review · incorrect", &mut sc.review_incorrect);
                ui.end_row();
                field(ui, "Reader · next word", &mut sc.reader_next);
                ui.end_row();
                field(ui, "Reader · previous word", &mut sc.reader_prev);
                ui.end_row();
                field(ui, "Reader · learn word", &mut sc.reader_learn);
                ui.end_row();
                field(ui, "Reader · mark known", &mut sc.reader_known);
                ui.end_row();
                field(ui, "Reader · ignore word", &mut sc.reader_ignore);
                ui.end_row();
                field(ui, "Reader · explain sentence", &mut sc.reader_explain);
                ui.end_row();
            });
        if ui.button("Reset shortcuts to defaults").clicked() {
            self.settings_draft.shortcuts = Default::default();
        }
    }

    fn settings_data(&mut self, ui: &mut egui::Ui) {
        ui.heading("Data");
        ui.label(format!("Data directory: {}", self.data_dir.display()));
        if let Some(status) = &self.data_status {
            ui.label(format!(
                "Dictionary entries: {} · frequency words: {}",
                status.dict_entries, status.frequency_words
            ));
        }
    }
}
