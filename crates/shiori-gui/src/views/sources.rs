//! Sources view: search free online libraries and import books directly.
//!
//! Aozora Bunko searches the locally cached catalog (instant, offline);
//! Wikisource queries the MediaWiki API. Imports run through the normal
//! pipeline and land in the library.

use eframe::egui;

use crate::app::{JrcGui, SourceImport, SourceTab};

impl JrcGui {
    pub fn show_sources(&mut self, ctx: &egui::Context) {
        let mut import: Option<SourceImport> = None;
        let mut reload_catalog = false;
        let mut search_ws = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("Find books online");
                ui.add_space(10.0);
                ui.selectable_value(&mut self.sources.tab, SourceTab::Aozora, "青空文庫");
                ui.selectable_value(
                    &mut self.sources.tab,
                    SourceTab::Wikisource,
                    "Wikisource",
                );
                if self.import_jobs > 0 {
                    ui.spinner();
                    ui.weak(format!("importing {}…", self.import_jobs));
                }
            });
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [(ui.available_width() - 180.0).clamp(220.0, 460.0), 24.0],
                    egui::TextEdit::singleline(&mut self.sources.query).hint_text(
                        match self.sources.tab {
                            SourceTab::Aozora => "title, reading, or author — 坊っちゃん, なつめ…",
                            SourceTab::Wikisource => "search Japanese Wikisource…",
                        },
                    ),
                );
                match self.sources.tab {
                    SourceTab::Aozora => {
                        if ui
                            .button("⟳ Reload catalog")
                            .on_hover_text("Fetch today's catalog from Aozora")
                            .clicked()
                        {
                            reload_catalog = true;
                        }
                    }
                    SourceTab::Wikisource => {
                        let enter = response.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button("Search").clicked() || enter {
                            search_ws = true;
                        }
                        if self.sources.ws_searching {
                            ui.spinner();
                        }
                    }
                }
            });
            ui.add_space(8.0);

            match self.sources.tab {
                SourceTab::Aozora => {
                    self.aozora_results(ui, &mut import);
                }
                SourceTab::Wikisource => {
                    self.wikisource_results(ui, &mut import);
                }
            }
        });

        if reload_catalog {
            self.sources.catalog = None;
            self.start_catalog_load(ctx, true);
        }
        if search_ws {
            self.start_wikisource_search(ctx);
        }
        if let Some(job) = import {
            self.start_source_import(ctx, job);
        }
    }

    fn aozora_results(&mut self, ui: &mut egui::Ui, import: &mut Option<SourceImport>) {
        let Some(catalog) = &self.sources.catalog else {
            if self.sources.catalog_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Loading the Aozora catalog…");
                });
            } else {
                ui.weak(
                    "Catalog not available — check your connection and reload. \
                     The catalog is cached after the first fetch.",
                );
            }
            return;
        };

        let query = self.sources.query.trim();
        let matches: Vec<&shiori_app::AozoraWork> = if query.is_empty() {
            Vec::new()
        } else {
            catalog
                .iter()
                .filter(|w| {
                    w.title.contains(query)
                        || w.title_reading.contains(query)
                        || w.author.contains(query)
                })
                .take(50)
                .collect()
        };

        if query.is_empty() {
            ui.weak(format!(
                "{} public-domain works available. Search by title, reading, \
                 or author.",
                catalog.len()
            ));
            return;
        }
        if matches.is_empty() {
            ui.weak("No matches in the catalog.");
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for work in matches {
                    ui.horizontal(|ui| {
                        if ui
                            .button("⬇ Import")
                            .on_hover_text("Download and add to the library")
                            .clicked()
                        {
                            *import = Some(SourceImport::Aozora(work.clone()));
                        }
                        ui.label(egui::RichText::new(&work.title).strong());
                        ui.weak(format!(
                            "{} · {}",
                            work.author,
                            work.orthography
                        ));
                    });
                    ui.add_space(2.0);
                }
            });
    }

    fn wikisource_results(&mut self, ui: &mut egui::Ui, import: &mut Option<SourceImport>) {
        if self.sources.ws_results.is_empty() {
            ui.weak(
                "Search the Japanese Wikisource — classic literature, \
                 historical documents, speeches, law texts.",
            );
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for hit in &self.sources.ws_results {
                    ui.horizontal(|ui| {
                        if ui
                            .button("⬇ Import")
                            .on_hover_text("Download and add to the library")
                            .clicked()
                        {
                            *import = Some(SourceImport::Wikisource(hit.title.clone()));
                        }
                        ui.label(egui::RichText::new(&hit.title).strong());
                        ui.weak(format!("{} words", hit.wordcount));
                    });
                    if !hit.snippet.is_empty() {
                        ui.weak(&hit.snippet);
                    }
                    ui.add_space(4.0);
                }
            });
    }
}
