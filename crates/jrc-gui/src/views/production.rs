//! Production mode: write Japanese, get naturalness feedback.

use eframe::egui;

use crate::app::JrcGui;

impl JrcGui {
    pub fn show_production(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Production practice");
            ui.label("Write a few sentences in Japanese in response to the prompt.");
            ui.add_space(10.0);

            let prompts = jrc_llm::writing_prompts();
            let prompt = prompts[self.production.prompt_idx % prompts.len()];
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(prompt).size(20.0).strong());
                if ui.small_button("another prompt").clicked() {
                    self.production.prompt_idx =
                        (self.production.prompt_idx + 1) % prompts.len();
                    self.production.feedback = None;
                }
            });
            ui.add_space(8.0);

            ui.add(
                egui::TextEdit::multiline(&mut self.production.text)
                    .hint_text("ここに日本語で書いてください…")
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Heading),
            );
            ui.add_space(6.0);

            if self.explainer.is_available() {
                let can_submit =
                    !self.production.waiting && !self.production.text.trim().is_empty();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_submit, egui::Button::new("Get feedback"))
                        .clicked()
                    {
                        self.request_feedback(ctx);
                    }
                    if self.production.waiting {
                        ui.spinner();
                        ui.label("the tutor is reading…");
                    }
                });
            } else {
                ui.weak(
                    "Feedback needs an LLM backend. Set ANTHROPIC_API_KEY and restart \
                     to enable it — writing practice works without it.",
                );
            }

            if let Some(feedback) = &self.production.feedback {
                ui.add_space(10.0);
                ui.separator();
                ui.heading("Feedback");
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.label(feedback);
                    });
            }
        });
    }
}
