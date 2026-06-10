//! Settings / about view.

use eframe::egui;

use crate::app::JrcGui;

impl JrcGui {
    pub fn show_settings(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Settings");
            ui.add_space(8.0);

            ui.label(format!("Data directory: {}", self.data_dir.display()));
            if let Some(status) = &self.data_status {
                ui.label(format!(
                    "Dictionary entries: {} · frequency words: {}",
                    status.dict_entries, status.frequency_words
                ));
            }
            ui.add_space(8.0);

            ui.heading("LLM backend");
            if self.explainer.is_available() {
                ui.label(format!("Backend: {} (active)", self.explainer.name()));
            } else {
                ui.label("Backend: none");
                ui.weak(
                    "Sentence explanations and production feedback are optional. To \
                     enable them, set the ANTHROPIC_API_KEY environment variable and \
                     restart the app. Everything else works offline.",
                );
            }
            ui.add_space(8.0);

            ui.heading("About");
            ui.label(
                "Japanese Reading Companion — comprehensible-input reading with \
                 vocabulary mining and FSRS spaced repetition.",
            );
            ui.label("Dictionary: JMdict © EDRDG (via jmdict-simplified).");
            ui.label("Frequency list: Leeds corpus derived (CC BY).");
        });
    }
}
