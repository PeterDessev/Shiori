//! Library view: file import (dialog + drag-and-drop), a full-width
//! sortable table of documents, and per-document metadata editing.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use jrc_core::DocumentId;

use crate::app::{JrcGui, MetaEdit, SortKey};
use crate::views::band_color;

impl JrcGui {
    pub fn show_library(&mut self, ctx: &egui::Context) {
        let hovering_files = ctx.input(|i| !i.raw.hovered_files.is_empty());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Library");
                ui.add_space(12.0);
                if ui
                    .button("📂 Import files…")
                    .on_hover_text("txt, md, html, epub, pdf — multiple files allowed")
                    .clicked()
                {
                    if let Some(paths) = rfd::FileDialog::new()
                        .add_filter(
                            "Readable",
                            &["txt", "md", "html", "htm", "xhtml", "epub", "pdf"],
                        )
                        .pick_files()
                    {
                        self.start_import_files(ctx, paths);
                    }
                }
                if self.import_jobs > 0 {
                    ui.spinner();
                    ui.label(format!(
                        "importing {} file{}…",
                        self.import_jobs,
                        if self.import_jobs == 1 { "" } else { "s" }
                    ));
                }
                ui.weak("· or drop files anywhere on this page");
            });
            ui.add_space(6.0);

            if self.library.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.weak("Nothing here yet.");
                    ui.weak("Import Japanese books or articles — txt, html (Aozora), epub, pdf.");
                });
            } else {
                // Horizontal scroll keeps the action buttons reachable
                // when the window is narrow.
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.documents_table(ui);
                    });
            }

            // Drop-target overlay.
            if hovering_files {
                let rect = ui.max_rect();
                let painter = ui.painter();
                painter.rect_filled(
                    rect,
                    8.0,
                    egui::Color32::from_rgba_unmultiplied(60, 120, 200, 40),
                );
                painter.rect_stroke(
                    rect.shrink(4.0),
                    8.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(90, 160, 240)),
                    egui::StrokeKind::Inside,
                );
                let n = ctx.input(|i| i.raw.hovered_files.len());
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("Drop to import {n} file{}", if n == 1 { "" } else { "s" }),
                    egui::FontId::proportional(26.0),
                    egui::Color32::from_rgb(200, 225, 255),
                );
            }
        });

        self.meta_edit_dialog(ctx);
    }

    fn documents_table(&mut self, ui: &mut egui::Ui) {
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

        let order = self.sorted_order();

        let mut to_open: Option<DocumentId> = None;
        let mut to_mine: Option<(DocumentId, String)> = None;
        let mut to_delete: Option<DocumentId> = None;
        let mut to_edit: Option<usize> = None;
        let mut clicked_sort: Option<SortKey> = None;

        let header = |ui: &mut egui::Ui,
                      label: &str,
                      key: SortKey,
                      current: SortKey,
                      asc: bool|
         -> bool {
            let arrow = if current == key {
                if asc {
                    " ▲"
                } else {
                    " ▼"
                }
            } else {
                ""
            };
            ui.add(
                egui::Label::new(egui::RichText::new(format!("{label}{arrow}")).strong())
                    .sense(egui::Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        };

        let sort_key = self.sort_key;
        let sort_asc = self.sort_asc;
        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(180.0).clip(true)) // title
            .column(Column::auto().at_least(90.0).clip(true)) // author
            .column(Column::auto().at_least(70.0)) // published
            .column(Column::auto().at_least(80.0)) // size
            .column(Column::auto().at_least(72.0)) // progress
            .column(Column::auto().at_least(56.0)) // known
            .column(Column::auto().at_least(80.0)) // difficulty
            .column(Column::auto().at_least(72.0)) // new words
            .column(Column::auto().at_least(150.0)) // actions
            .header(24.0, |mut row| {
                row.col(|ui| {
                    if header(ui, "Title", SortKey::Title, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Title);
                    }
                });
                row.col(|ui| {
                    if header(ui, "Author", SortKey::Author, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Author);
                    }
                });
                row.col(|ui| {
                    if header(ui, "Published", SortKey::Published, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Published);
                    }
                });
                row.col(|ui| {
                    if header(ui, "Size", SortKey::Size, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Size);
                    }
                });
                row.col(|ui| {
                    if header(ui, "Progress", SortKey::Progress, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Progress);
                    }
                });
                row.col(|ui| {
                    if header(ui, "Known", SortKey::Known, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Known);
                    }
                });
                row.col(|ui| {
                    if header(ui, "Difficulty", SortKey::Difficulty, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::Difficulty);
                    }
                });
                row.col(|ui| {
                    if header(ui, "New words", SortKey::NewWords, sort_key, sort_asc) {
                        clicked_sort = Some(SortKey::NewWords);
                    }
                });
                row.col(|_| {});
            })
            .body(|mut body| {
                for &i in &order {
                    let summary = &self.library[i];
                    let id = summary.document.id;
                    let stats = self.doc_stats.get(&id.0);
                    body.row(26.0, |mut row| {
                        row.col(|ui| {
                            ui.label(&summary.document.title);
                            if recommended == Some(id) && self.library.len() > 1 {
                                ui.label(
                                    egui::RichText::new("→ read next")
                                        .small()
                                        .color(egui::Color32::from_rgb(80, 160, 220)),
                                );
                            }
                        });
                        row.col(|ui| {
                            ui.label(&summary.document.author);
                        });
                        row.col(|ui| {
                            ui.label(&summary.document.published);
                        });
                        row.col(|ui| {
                            ui.label(format!("{}", summary.token_count)).on_hover_text(
                                format!(
                                    "{} tokens · {} sentences",
                                    summary.token_count, summary.sentence_count
                                ),
                            );
                        });
                        row.col(|ui| {
                            let frac = summary.document.last_sentence as f32
                                / summary.sentence_count.max(1) as f32;
                            if summary.document.last_sentence == 0 {
                                ui.weak("—");
                            } else {
                                ui.label(format!("{:.0}%", (frac * 100.0).min(100.0)));
                            }
                        });
                        row.col(|ui| match stats {
                            Some(s) => {
                                ui.label(format!("{:.0}%", s.known_share() * 100.0));
                            }
                            None => {
                                ui.label("–");
                            }
                        });
                        row.col(|ui| match stats {
                            Some(s) => {
                                ui.colored_label(band_color(s.band), s.band.label());
                            }
                            None => {
                                ui.label("–");
                            }
                        });
                        row.col(|ui| match stats {
                            Some(s) => {
                                ui.label(format!("{}", s.distinct_unknown_words));
                            }
                            None => {
                                ui.label("–");
                            }
                        });
                        row.col(|ui| {
                            if ui.button("Read").clicked() {
                                to_open = Some(id);
                            }
                            if ui.button("Mine").clicked() {
                                to_mine = Some((id, summary.document.title.clone()));
                            }
                            if ui
                                .small_button("✏")
                                .on_hover_text("Edit metadata")
                                .clicked()
                            {
                                to_edit = Some(i);
                            }
                            if ui
                                .small_button("🗑")
                                .on_hover_text("Delete document")
                                .clicked()
                            {
                                to_delete = Some(id);
                            }
                        });
                    });
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
        if let Some(i) = to_edit {
            let doc = &self.library[i].document;
            self.meta_edit = Some(MetaEdit {
                id: doc.id,
                meta: jrc_core::DocumentMeta {
                    title: doc.title.clone(),
                    author: doc.author.clone(),
                    publisher: doc.publisher.clone(),
                    published: doc.published.clone(),
                },
            });
        }
        if let Some(id) = to_open {
            self.open_reader(id);
        }
        if let Some((id, title)) = to_mine {
            self.open_mining(id, title);
        }
        if let Some(id) = to_delete {
            if self.reader.as_ref().is_some_and(|r| r.doc.id == id) {
                self.end_page_visit(crate::session::VisitEnd::Pause);
                self.reader = None;
            }
            if self.mining.doc_id == Some(id) {
                self.mining = Default::default();
            }
            self.with_app(|app| Ok(app.db().delete_document(id)?));
            self.refresh_caches();
        }
    }

    /// Sorted indices into `self.library` under the current sort key.
    fn sorted_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.library.len()).collect();
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
            SortKey::Added => order.sort_by_key(|i| lib[*i].document.added_at),
            SortKey::Title => order.sort_by(|a, b| lib[*a].document.title.cmp(&lib[*b].document.title)),
            SortKey::Author => {
                order.sort_by(|a, b| lib[*a].document.author.cmp(&lib[*b].document.author))
            }
            SortKey::Published => {
                order.sort_by(|a, b| lib[*a].document.published.cmp(&lib[*b].document.published))
            }
            SortKey::Size => order.sort_by_key(|i| lib[*i].token_count),
            SortKey::Progress => order.sort_by(|a, b| {
                let frac = |i: &usize| {
                    lib[*i].document.last_sentence as f32 / lib[*i].sentence_count.max(1) as f32
                };
                frac(a).total_cmp(&frac(b))
            }),
            SortKey::Known => order.sort_by(|a, b| known(a).total_cmp(&known(b))),
            SortKey::Difficulty => order.sort_by(|a, b| unknown(a).total_cmp(&unknown(b))),
            SortKey::NewWords => order.sort_by_key(new_words),
        }
        if !self.sort_asc {
            order.reverse();
        }
        order
    }

    fn meta_edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(edit) = &mut self.meta_edit else { return };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Edit metadata")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("meta-edit-grid")
                    .num_columns(2)
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Title:");
                        ui.add_sized([280.0, 20.0], egui::TextEdit::singleline(&mut edit.meta.title));
                        ui.end_row();
                        ui.label("Author:");
                        ui.add_sized(
                            [280.0, 20.0],
                            egui::TextEdit::singleline(&mut edit.meta.author),
                        );
                        ui.end_row();
                        ui.label("Publisher:");
                        ui.add_sized(
                            [280.0, 20.0],
                            egui::TextEdit::singleline(&mut edit.meta.publisher),
                        );
                        ui.end_row();
                        ui.label("Published:");
                        ui.add_sized(
                            [280.0, 20.0],
                            egui::TextEdit::singleline(&mut edit.meta.published),
                        );
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let valid = !edit.meta.title.trim().is_empty();
                    if ui.add_enabled(valid, egui::Button::new("💾 Save")).clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if !valid {
                        ui.weak("title required");
                    }
                });
            });

        if save {
            if let Some(edit) = self.meta_edit.take() {
                let mut meta = edit.meta;
                meta.title = meta.title.trim().to_string();
                self.with_app(|app| Ok(app.db().update_document_meta(edit.id, &meta)?));
                self.refresh_caches();
                // Keep the reader header in sync if this doc is open.
                if let Some(reader) = self.reader.as_mut() {
                    if reader.doc.id == edit.id {
                        reader.doc.title = meta.title.clone();
                    }
                }
            }
        } else if cancel || !open {
            self.meta_edit = None;
        }
    }
}
