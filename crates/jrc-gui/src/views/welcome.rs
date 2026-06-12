//! Getting-started page, shown on first launch and from the ❓ button.

use eframe::egui;

use crate::app::JrcGui;

impl JrcGui {
    pub fn show_welcome(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let width = ui.available_width().min(720.0);
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(width);
                        ui.add_space(24.0);
                        ui.heading(egui::RichText::new("Welcome 👋").size(28.0));
                        ui.add_space(4.0);
                        ui.label(
                            "This app teaches Japanese through reading. You read real \
                             Japanese text, and everything else exists to support that.",
                        );
                        ui.add_space(16.0);

                        section(ui, "1 · Import", &[
                            "Bring in anything you want to read — paste text, or import \
                             .txt, .html, .epub, and .pdf files (Aozora Bunko works \
                             great). The app analyzes every sentence and tracks each \
                             word it finds.",
                        ]);
                        section(ui, "2 · Read — the main activity", &[
                            "Open a document and just read. Click any word you don't \
                             know: you get its dictionary entry, how it's actually used \
                             (formal? colloquial? usually kana?), and what form it's \
                             conjugated into. From there one click adds it to your \
                             reviews, marks it known, or ignores it.",
                        ]);
                        section(ui, "3 · Review", &[
                            "Words you chose to learn come back for spaced-repetition \
                             review (FSRS) — always shown inside the sentence you found \
                             them in, never as an isolated flashcard. A few minutes a \
                             day keeps the queue short.",
                        ]);
                        section(ui, "4 · Stats — what should I read next?", &[
                            "For every document you'll see how much of it you already \
                             know. Aim for material with roughly 2–5% unknown words — \
                             hard enough to learn from, easy enough to enjoy. The \
                             library marks the best next read for you.",
                        ]);

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(
                            "Optional: connect an LLM in Settings to get sentence-level \
                             grammar explanations while reading, and feedback on your \
                             own writing in Production mode. Everything else works \
                             completely offline.",
                        );
                        ui.add_space(16.0);
                        if ui
                            .add_sized([220.0, 36.0], egui::Button::new("Get started"))
                            .clicked()
                        {
                            self.finish_onboarding();
                        }
                        ui.add_space(24.0);
                    });
                });
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str, paragraphs: &[&str]) {
    ui.add_space(10.0);
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.label(egui::RichText::new(title).size(18.0).strong());
        for p in paragraphs {
            ui.label(*p);
        }
    });
}
