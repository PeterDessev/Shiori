//! Library view: imported documents with sortable columns, import form
//! with metadata, recommendations.

use eframe::egui;
use jrc_core::DocumentId;

use crate::app::{JrcGui, SortKey};
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

        egui::Grid::new("import-meta")
            .num_columns(4)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Title:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.import.title)
                        .hint_text("e.g. 走れメロス")
                        .desired_width(240.0),
                );
                ui.label("Author:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.import.author)
                        .hint_text("e.g. 太宰治")
                        .desired_width(200.0),
                );
                ui.end_row();
                ui.label("Publisher:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.import.publisher)
                        .desired_width(240.0),
                );
                ui.label("Published:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.import.published)
                        .hint_text("e.g. 1940")
                        .desired_width(200.0),
                );
                ui.end_row();
            });

        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.importing && !self.import.extracting,
                    egui::Button::new("📂 Choose file…"),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Readable", &["txt", "md", "html", "htm", "xhtml", "epub", "pdf"])
                    .pick_file()
                {
                    self.start_extract(ctx, path);
                }
            }
            if self.import.extracting {
                ui.spinner();
                ui.label("extracting…");
            }
            if let Some(file) = &self.import.file {
                ui.label(format!(
                    "✔ {} ({} characters)",
                    file.name,
                    file.text.chars().count()
                ));
                if ui.small_button("✖").on_hover_text("Discard file").clicked() {
                    self.import.file = None;
                }
            }
        });

        if self.import.file.is_none() {
            ui.add(
                egui::TextEdit::multiline(&mut self.import.text)
                    .hint_text("…or paste Japanese text here")
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
        }

        let has_content = self.import.file.is_some() || !self.import.text.trim().is_empty();
        let can_import = !self.importing && has_content && !self.import.title.trim().is_empty();
        if ui
            .add_enabled(can_import, egui::Button::new("Import"))
            .clicked()
        {
            let meta = self.import.meta();
            let text = match &self.import.file {
                Some(file) => file.text.clone(),
                None => self.import.text.clone(),
            };
            self.start_import(ctx, meta, text);
        }
        if has_content && self.import.title.trim().is_empty() {
            ui.weak("A title is required (picked files prefill it when possible).");
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

        // Sort a view of the library without disturbing the cache.
        let mut order: Vec<usize> = (0..self.library.len()).collect();
        {
            let stats = &self.doc_stats;
            let lib = &self.library;
            let unknown = |i: &usize| {
                stats
                    .get(&lib[*i].document.id.0)
                    .map(|s| s.unknown_share())
                    .unwrap_or(1.0)
            };
            let known = |i: &usize| {
                stats
                    .get(&lib[*i].document.id.0)
                    .map(|s| s.known_share())
                    .unwrap_or(0.0)
            };
            let new_words = |i: &usize| {
                stats
                    .get(&lib[*i].document.id.0)
                    .map(|s| s.distinct_unknown_words)
                    .unwrap_or(0)
            };
            match self.sort_key {
                SortKey::Added => order.sort_by_key(|i| self.library[*i].document.added_at),
                SortKey::Title => {
                    order.sort_by(|a, b| lib[*a].document.title.cmp(&lib[*b].document.title))
                }
                SortKey::Author => {
                    order.sort_by(|a, b| lib[*a].document.author.cmp(&lib[*b].document.author))
                }
                SortKey::Published => order.sort_by(|a, b| {
                    lib[*a].document.published.cmp(&lib[*b].document.published)
                }),
                SortKey::Size => order.sort_by_key(|i| lib[*i].token_count),
                SortKey::Known => order.sort_by(|a, b| known(a).total_cmp(&known(b))),
                SortKey::Difficulty => order.sort_by(|a, b| unknown(a).total_cmp(&unknown(b))),
                SortKey::NewWords => order.sort_by_key(new_words),
            }
            if !self.sort_asc {
                order.reverse();
            }
        }

        let mut clicked_sort: Option<SortKey> = None;
        egui::Grid::new("library-grid")
            .striped(true)
            .num_columns(8)
            .spacing([14.0, 6.0])
            .show(ui, |ui| {
                for (label, key) in [
                    ("Title", SortKey::Title),
                    ("Author", SortKey::Author),
                    ("Published", SortKey::Published),
                    ("Size", SortKey::Size),
                    ("Known", SortKey::Known),
                    ("Difficulty", SortKey::Difficulty),
                    ("New words", SortKey::NewWords),
                ] {
                    let arrow = if self.sort_key == key {
                        if self.sort_asc {
                            " ▲"
                        } else {
                            " ▼"
                        }
                    } else {
                        ""
                    };
                    if ui
                        .button(egui::RichText::new(format!("{label}{arrow}")).strong())
                        .clicked()
                    {
                        clicked_sort = Some(key);
                    }
                }
                ui.strong("");
                ui.end_row();

                for &i in &order {
                    let summary = &self.library[i];
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
                    ui.label(&summary.document.author);
                    ui.label(&summary.document.published);
                    ui.label(format!("{} tokens", summary.token_count))
                        .on_hover_text(format!("{} sentences", summary.sentence_count));
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

        if let Some(key) = clicked_sort {
            if self.sort_key == key {
                self.sort_asc = !self.sort_asc;
            } else {
                self.sort_key = key;
                self.sort_asc = true;
            }
        }

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
