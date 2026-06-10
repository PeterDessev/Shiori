//! Library view: imported documents, difficulty, recommendations, import.

use eframe::egui;
use jrc_core::DocumentId;

use crate::app::JrcGui;
use crate::views::band_color;

impl JrcGui {
    pub fn show_library(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    self.import_section(ctx, ui);
                    ui.separator();
                    self.documents_section(ui);
                });
        });
    }

    fn import_section(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Import text");
        ui.horizontal(|ui| {
            ui.label("Title:");
            ui.add(
                egui::TextEdit::singleline(&mut self.import.title)
                    .hint_text("e.g. 走れメロス")
                    .desired_width(280.0),
            );
            if ui
                .add_enabled(!self.importing, egui::Button::new("Import file…"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Text", &["txt", "md"])
                    .pick_file()
                {
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            let title = if self.import.title.trim().is_empty() {
                                path.file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "Untitled".into())
                            } else {
                                self.import.title.clone()
                            };
                            self.start_import(ctx, title, text);
                        }
                        Err(e) => self.error = Some(format!("could not read file: {e}")),
                    }
                }
            }
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.import.text)
                .hint_text("…or paste Japanese text here")
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        let can_import = !self.importing
            && !self.import.text.trim().is_empty()
            && !self.import.title.trim().is_empty();
        if ui
            .add_enabled(can_import, egui::Button::new("Import pasted text"))
            .clicked()
        {
            let title = self.import.title.clone();
            let text = self.import.text.clone();
            self.start_import(ctx, title, text);
        }
        if self.import.title.trim().is_empty() && !self.import.text.trim().is_empty() {
            ui.weak("Give the text a title to import it.");
        }
    }

    fn documents_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Library");
        if self.library.is_empty() {
            ui.weak("Nothing here yet — import some Japanese text above.");
            return;
        }

        // Best next read, by sweet-spot distance.
        let recommended: Option<DocumentId> = self
            .library
            .iter()
            .filter_map(|d| {
                self.doc_stats
                    .get(&d.document.id.0)
                    .map(|s| (d.document.id, s.unknown_share()))
            })
            .min_by(|a, b| {
                let score = |share: f64| {
                    const IDEAL: f64 = 0.035;
                    if share >= IDEAL {
                        (share - IDEAL) * 2.0
                    } else {
                        IDEAL - share
                    }
                };
                score(a.1).total_cmp(&score(b.1))
            })
            .map(|(id, _)| id);

        let mut to_open: Option<DocumentId> = None;
        let mut to_mine: Option<(DocumentId, String)> = None;
        let mut to_delete: Option<DocumentId> = None;

        egui::Grid::new("library-grid")
            .striped(true)
            .num_columns(6)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Title");
                ui.strong("Size");
                ui.strong("Known");
                ui.strong("Difficulty");
                ui.strong("New words");
                ui.strong("");
                ui.end_row();

                for summary in &self.library {
                    let id = summary.document.id;
                    ui.horizontal(|ui| {
                        ui.label(&summary.document.title);
                        if recommended == Some(id) && self.library.len() > 1 {
                            ui.label(
                                egui::RichText::new("→ read next")
                                    .small()
                                    .color(egui::Color32::from_rgb(80, 160, 220)),
                            );
                        }
                    });
                    ui.label(format!(
                        "{} sentences / {} tokens",
                        summary.sentence_count, summary.token_count
                    ));
                    match self.doc_stats.get(&id.0) {
                        Some(stats) => {
                            ui.label(format!("{:.0}%", stats.known_share() * 100.0));
                            ui.colored_label(band_color(stats.band), stats.band.label());
                            ui.label(format!("{}", stats.distinct_unknown_words));
                        }
                        None => {
                            ui.label("–");
                            ui.label("–");
                            ui.label("–");
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Read").clicked() {
                            to_open = Some(id);
                        }
                        if ui.button("Mine").clicked() {
                            to_mine = Some((id, summary.document.title.clone()));
                        }
                        if ui.small_button("🗑").on_hover_text("Delete document").clicked() {
                            to_delete = Some(id);
                        }
                    });
                    ui.end_row();
                }
            });

        if let Some(id) = to_open {
            self.open_reader(id);
        }
        if let Some((id, title)) = to_mine {
            self.open_mining(id, title);
        }
        if let Some(id) = to_delete {
            if self.reader.as_ref().is_some_and(|r| r.doc.id == id) {
                self.reader = None;
            }
            if self.mining.doc_id == Some(id) {
                self.mining = Default::default();
            }
            self.with_app(|app| Ok(app.db().delete_document(id)?));
            self.refresh_caches();
        }
    }
}
