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
                    ui.heading("Reader");
                    ui.checkbox(
                        &mut self.settings_draft.show_unknown_highlights,
                        "Tint words I haven't studied yet",
                    );
                    ui.weak("The selected word is always highlighted; this adds a subtle \
                             tint on unknown vocabulary as well.");

                    ui.add_space(14.0);
                    let dirty = settings_differ(&self.settings, &self.settings_draft);
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

fn settings_differ(a: &crate::settings::Settings, b: &crate::settings::Settings) -> bool {
    a.anthropic_api_key != b.anthropic_api_key
        || a.llm_model != b.llm_model
        || a.show_unknown_highlights != b.show_unknown_highlights
}
