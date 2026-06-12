//! Getting-started page, shown on first launch and from the ❓ button.
//! Each section keeps a one-line summary with the detail folded away.

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

                        section(
                            ui,
                            "1 · Import — bring in anything you want to read",
                            "Drop files on the library, or fetch books straight from \
                             the internet.",
                            |ui| {
                                ui.label(
                                    "The library accepts .txt, .html (Aozora Bunko \
                                     works great), .epub, and .pdf — drag files onto \
                                     the page or use the import button. Every sentence \
                                     is analyzed and each word is tracked from then on.",
                                );
                                ui.label(
                                    "The 🌐 view searches Aozora Bunko's 17,000+ \
                                     public-domain works and Japanese Wikisource, and \
                                     imports them in one click.",
                                );
                                ui.label(
                                    "Each book's ⓘ panel shows its difficulty, a \
                                     coverage forecast, your reading time, and the most \
                                     useful unknown words in it.",
                                );
                            },
                        );

                        section(
                            ui,
                            "2 · Read — the main activity",
                            "Click any word for its dictionary entry; one click adds \
                             it to reviews.",
                            |ui| {
                                ui.label(
                                    "Open a document and just read. Click any word you \
                                     don't know: you get its dictionary entry, usage \
                                     register (formal? colloquial? usually kana?), its \
                                     conjugated form explained, and the kanji it \
                                     contains — with stroke order a click away.",
                                );
                                ui.label(
                                    "Furigana is configurable in Settings → Reading: \
                                     none, unknown words, only the first few \
                                     occurrences of each word per book (they fade as \
                                     you meet a word again), or everything.",
                                );
                                ui.label(
                                    "Pages turn with the scroll wheel or PgUp/PgDn. \
                                     Your place is saved automatically, and the pause \
                                     button (or its shortcut) stops the reading clock — \
                                     it also pauses itself if you wander off.",
                                );
                                ui.label(
                                    "When you finish a book, its ⓘ panel offers to \
                                     mark every word you never looked up as known — \
                                     with a review list of any that look unusual for \
                                     your level.",
                                );
                            },
                        );

                        section(
                            ui,
                            "3 · Word knowledge — four statuses",
                            "unknown · learning · known · ignored — they drive every \
                             statistic.",
                            |ui| {
                                ui.label(
                                    "unknown — the default. It doesn't mean you don't \
                                     know the word, only that the app has no \
                                     information yet.",
                                );
                                ui.label(
                                    "learning — in spaced-repetition rotation. Words \
                                     graduate to known automatically as their memory \
                                     stability grows.",
                                );
                                ui.label(
                                    "known — counted as your vocabulary; drives \
                                     difficulty estimates and the level grade.",
                                );
                                ui.label(
                                    "ignored — opted out of the accounting: names, \
                                     loanwords you read for free, noise. Ignored words \
                                     count as readable but never as vocabulary, and \
                                     are never suggested for study.",
                                );
                            },
                        );

                        section(
                            ui,
                            "4 · Review",
                            "Spaced repetition (FSRS), always in the sentence you met \
                             the word in.",
                            |ui| {
                                ui.label(
                                    "Words you chose to learn come back for review — \
                                     shown inside their source sentence, framed by its \
                                     neighbors, never as an isolated flashcard. Other \
                                     sentences from your library using the word appear \
                                     under the answer.",
                                );
                                ui.label(
                                    "Answer with Correct/Incorrect (keyboard works \
                                     too). A few minutes a day keeps the queue short; \
                                     the 📊 page forecasts the next two weeks.",
                                );
                            },
                        );

                        section(
                            ui,
                            "5 · Dictionary & kanji",
                            "Search anything; kanji cards show readings, meanings, \
                             and stroke order.",
                            |ui| {
                                ui.label(
                                    "The 🔍 view searches by kanji, kana, or any word \
                                     form, prefix included. Words can go straight into \
                                     the SRS from here, and every kanji in the query \
                                     gets a card with on/kun readings, meanings, \
                                     school grade, and a numbered stroke-order diagram.",
                                );
                            },
                        );

                        section(
                            ui,
                            "6 · Stats — what should I read next?",
                            "Difficulty per book, reading velocity, a JLPT-ish level \
                             grade, review forecasts.",
                            |ui| {
                                ui.label(
                                    "Aim for material with roughly 2–5% unknown words — \
                                     hard enough to learn from, easy enough to enjoy. \
                                     The library marks the best next read for you.",
                                );
                                ui.label(
                                    "The 📊 page tracks reading speed and time (with a \
                                     calendar), grades your comfortable reading level \
                                     against JLPT vocabulary lists, and shows review \
                                     retention and intake.",
                                );
                            },
                        );

                        section(
                            ui,
                            "7 · Production — chat in Japanese",
                            "A native-speaker persona converses with you; corrections \
                             arrive as paper-style underlines.",
                            |ui| {
                                ui.label(
                                    "Write Japanese in the ✏ view. The partner replies \
                                     like a friend — it never corrects you mid-\
                                     conversation. Mistakes and clunky phrasing in \
                                     your messages get underlined afterwards (red = \
                                     wrong, orange = unnatural); hover or click for \
                                     the explanation.",
                                );
                                ui.label(
                                    "Every word in the chat is clickable like the \
                                     reader, and a challenge dial sets how hard the \
                                     partner's Japanese pushes you. Conversations are \
                                     kept in a history sidebar.",
                                );
                                ui.label(
                                    "This needs an LLM backend (Settings → AI): \
                                     Anthropic, a local model via Ollama — nothing \
                                     leaves your machine — or any OpenAI-compatible \
                                     server.",
                                );
                            },
                        );

                        section(
                            ui,
                            "8 · Make it yours",
                            "Themes, fonts, shortcuts, Anki export — all in Settings.",
                            |ui| {
                                ui.label(
                                    "Appearance: dark, light, and sepia themes; \
                                     gothic or mincho Japanese fonts; reader text size \
                                     and line spacing.",
                                );
                                ui.label(
                                    "Shortcuts: click a binding and press the keys — \
                                     modifier combos like Ctrl+Shift+4 included.",
                                );
                                ui.label(
                                    "Data: export your cards to Anki (or import an \
                                     existing deck), and back up the database in one \
                                     click.",
                                );
                            },
                        );

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(
                            "Everything except the LLM features and online book search \
                             works completely offline once the dictionary is downloaded.",
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

/// One numbered section: bold summary line, details folded underneath.
fn section(
    ui: &mut egui::Ui,
    title: &str,
    summary: &str,
    details: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(10.0);
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.label(egui::RichText::new(title).size(18.0).strong());
        ui.label(summary);
        egui::CollapsingHeader::new("details")
            .id_salt(title)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                details(ui);
            });
    });
}
