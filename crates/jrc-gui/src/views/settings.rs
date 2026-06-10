//! Settings view: LLM backend, reader preferences, data info.

use eframe::egui;

use crate::app::JrcGui;

impl JrcGui {
    pub fn show_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.heading("LLM backend (optional)");
                    ui.label(
                        "Powers sentence explanations in the reader and writing \
                         feedback in Production mode. Everything else works without it.",
                    );
                    ui.add_space(6.0);
                    egui::Grid::new("llm-grid").spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label("Anthropic API key:");
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut self.settings_draft.anthropic_api_key,
                            )
                            .password(true)
                            .hint_text("sk-ant-…")
                            .desired_width(360.0),
                        );
                        ui.end_row();
                        ui.label("Model:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings_draft.llm_model)
                                .desired_width(360.0),
                        );
                        ui.end_row();
                    });
                    ui.weak(
                        "The key is stored locally in settings.json and only ever sent \
                         to the Anthropic API. Leave empty to use the ANTHROPIC_API_KEY \
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

                    ui.add_space(14.0);
                    ui.heading("Appearance");
                    ui.horizontal(|ui| {
                        ui.label("Theme:");
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

                    ui.add_space(14.0);
                    ui.heading("Reader");
                    ui.checkbox(
                        &mut self.settings_draft.show_unknown_highlights,
                        "Tint words I haven't studied yet",
                    );
                    ui.weak("The selected word is always highlighted; this adds a subtle \
                             tint on unknown vocabulary as well.");

                    ui.add_space(14.0);
                    ui.heading("Keyboard shortcuts");
                    ui.weak(
                        "Key names: letters and digits (K, 1), Space, Enter, ArrowLeft, \
                         ArrowRight, ArrowUp, ArrowDown, Tab, Escape, F1–F12.",
                    );
                    ui.add_space(4.0);
                    egui::Grid::new("shortcut-grid")
                        .num_columns(4)
                        .spacing([10.0, 6.0])
                        .show(ui, |ui| {
                            let sc = &mut self.settings_draft.shortcuts;
                            let field =
                                |ui: &mut egui::Ui, label: &str, value: &mut String| {
                                    ui.label(label);
                                    ui.add(
                                        egui::TextEdit::singleline(value).desired_width(110.0),
                                    );
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

                    ui.add_space(14.0);
                    let dirty = self.settings != self.settings_draft;
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(dirty, egui::Button::new("💾 Save settings"))
                            .clicked()
                        {
                            self.apply_settings();
                        }
                        if dirty {
                            ui.weak("unsaved changes");
                        }
                    });

                    ui.add_space(18.0);
                    ui.separator();
                    ui.heading("Data");
                    ui.label(format!("Data directory: {}", self.data_dir.display()));
                    if let Some(status) = &self.data_status {
                        ui.label(format!(
                            "Dictionary entries: {} · frequency words: {}",
                            status.dict_entries, status.frequency_words
                        ));
                    }

                    ui.add_space(14.0);
                    ui.heading("About");
                    ui.label(
                        "Japanese Reading Companion — comprehensible-input reading \
                         with vocabulary mining and FSRS spaced repetition.",
                    );
                    ui.label("Dictionary: JMdict © EDRDG (via jmdict-simplified).");
                    ui.label("Frequency list: Leeds corpus derived (CC BY).");
                    if ui.button("Show getting-started guide").clicked() {
                        self.view = crate::app::View::Welcome;
                    }
                });
        });
    }
}
