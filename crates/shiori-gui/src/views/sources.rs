//! Book search — per language.
//!
//! The active language decides where to look: Aozora Bunko (Japanese
//! only, cached and searched locally), the matching Wikisource wiki, and
//! Project Gutenberg through Gutendex. Users can also add OPDS
//! distributors per language and search them here. The "Libraries" tab
//! lists the free, legal collections the bundled catalog knows for the
//! language. Switching the language switch at the top re-scopes every
//! tab (and where imports land).

use eframe::egui;

use crate::app::{ShioriGui, SourceImport, SourceTab};
use crate::settings::OpdsCatalog;

/// Anonymous, multilingual OPDS feeds offered as one-click additions.
const OPDS_SUGGESTIONS: &[(&str, &str)] = &[
    ("Project Gutenberg", "https://www.gutenberg.org/ebooks.opds/"),
    ("Open Library", "https://openlibrary.org/opds"),
];

impl ShioriGui {
    pub fn show_sources(&mut self, ctx: &egui::Context) {
        let code = self.settings.active_language.clone();
        let profile = shiori_app::book_lang_profile(&code);
        let is_japanese = self.active_lang_is_japanese();
        let lang_name = self.active_lang_name();

        // Which tabs this language offers, and keep the selection valid.
        let mut tabs: Vec<SourceTab> = Vec::new();
        if is_japanese {
            tabs.push(SourceTab::Aozora);
        }
        if profile.wikisource_subdomain.is_some() {
            tabs.push(SourceTab::Wikisource);
        }
        if profile.gutendex_lang.is_some() {
            tabs.push(SourceTab::Gutenberg);
        }
        tabs.push(SourceTab::Opds);
        tabs.push(SourceTab::Libraries);
        if !tabs.contains(&self.sources.tab) {
            self.sources.tab = tabs[0];
        }

        let languages: Vec<(String, String)> = self
            .lang_infos
            .iter()
            .map(|i| (i.lang.clone(), i.name.clone()))
            .collect();
        let active_name = languages
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| code.clone());
        let catalogs: Vec<OpdsCatalog> = self.settings.opds_for(&code).to_vec();

        // Deferred actions (applied after the immutable UI borrow ends).
        let mut switch_to: Option<String> = None;
        let mut import: Option<SourceImport> = None;
        let mut do_search = false;
        let mut reload_catalog = false;
        let mut opds_add: Option<(String, String)> = None;
        let mut opds_remove: Option<usize> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("Find books online");
                ui.add_space(12.0);
                ui.label("Language:");
                egui::ComboBox::from_id_salt("sources-language")
                    .selected_text(active_name)
                    .show_ui(ui, |ui| {
                        for (c, n) in &languages {
                            if ui.selectable_label(*c == code, n).clicked() && *c != code {
                                switch_to = Some(c.clone());
                            }
                        }
                    });
                if self.import_jobs > 0 {
                    ui.spinner();
                    ui.weak(format!("importing {}…", self.import_jobs));
                }
            });
            ui.add_space(6.0);

            // Tab bar.
            ui.horizontal(|ui| {
                for tab in &tabs {
                    ui.selectable_value(&mut self.sources.tab, *tab, tab_label(*tab));
                }
            });
            ui.add_space(6.0);

            // Search row (every tab except the static Libraries list).
            if self.sources.tab != SourceTab::Libraries {
                let hint = match self.sources.tab {
                    SourceTab::Aozora if is_japanese => "title, reading, or author — 坊っちゃん, なつめ…",
                    SourceTab::Aozora => "title, reading, or author…",
                    SourceTab::Wikisource => "search this language's Wikisource…",
                    SourceTab::Gutenberg => "search Project Gutenberg…",
                    SourceTab::Opds => "search this catalog…",
                    SourceTab::Libraries => "",
                };
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [(ui.available_width() - 180.0).clamp(220.0, 460.0), 24.0],
                        egui::TextEdit::singleline(&mut self.sources.query).hint_text(hint),
                    );
                    let enter =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
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
                        _ => {
                            if ui.button("Search").clicked() || enter {
                                do_search = true;
                            }
                            let busy = match self.sources.tab {
                                SourceTab::Wikisource => self.sources.ws_searching,
                                SourceTab::Gutenberg => self.sources.gutendex_searching,
                                SourceTab::Opds => self.sources.opds_searching,
                                _ => false,
                            };
                            if busy {
                                ui.spinner();
                            }
                        }
                    }
                });
                ui.add_space(8.0);
            }

            match self.sources.tab {
                SourceTab::Aozora => self.aozora_results(ui, &mut import),
                SourceTab::Wikisource => self.wikisource_results(ui, &lang_name, &mut import),
                SourceTab::Gutenberg => self.gutendex_results(ui, &mut import),
                SourceTab::Opds => {
                    self.opds_ui(ui, &catalogs, &mut import, &mut opds_add, &mut opds_remove)
                }
                SourceTab::Libraries => self.libraries_list(ui, &code, &lang_name),
            }
        });

        // ── Apply deferred actions ──────────────────────────────────
        if let Some(new_code) = switch_to {
            self.switch_language(ctx, &new_code);
            return;
        }
        if reload_catalog {
            self.sources.catalog = None;
            self.start_catalog_load(ctx, true);
        }
        if let Some((name, url)) = opds_add {
            if self.settings.add_opds(&code, &name, &url) {
                let _ = self.settings.save(&self.data_dir);
                self.sources.new_opds_name.clear();
                self.sources.new_opds_url.clear();
                // Select the newly added distributor; its results aren't
                // whatever the previously-selected one returned.
                self.sources.opds_selected = self.settings.opds_for(&code).len().saturating_sub(1);
                self.sources.opds_results.clear();
            }
        }
        if let Some(idx) = opds_remove {
            self.settings.remove_opds(&code, idx);
            let _ = self.settings.save(&self.data_dir);
            self.sources.opds_selected = 0;
            self.sources.opds_results.clear();
        }
        if do_search {
            match self.sources.tab {
                SourceTab::Wikisource => self.start_wikisource_search(ctx),
                SourceTab::Gutenberg => self.start_gutendex_search(ctx),
                SourceTab::Opds => {
                    let catalogs = self.settings.opds_for(&code);
                    if let Some(cat) = catalogs.get(self.sources.opds_selected) {
                        let url = cat.url.clone();
                        self.start_opds_search(ctx, url);
                    }
                }
                _ => {}
            }
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
                        ui.weak(format!("{} · {}", work.author, work.orthography));
                    });
                    ui.add_space(2.0);
                }
            });
    }

    fn wikisource_results(
        &mut self,
        ui: &mut egui::Ui,
        lang_name: &str,
        import: &mut Option<SourceImport>,
    ) {
        if self.sources.ws_results.is_empty() {
            ui.weak(format!(
                "Search the {lang_name} Wikisource — classic literature, \
                 historical documents, speeches, law texts."
            ));
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

    fn gutendex_results(&mut self, ui: &mut egui::Ui, import: &mut Option<SourceImport>) {
        if self.sources.gutendex_results.is_empty() {
            ui.weak(
                "Search Project Gutenberg — tens of thousands of public-domain \
                 books. Results are filtered to this language.",
            );
            return;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for hit in &self.sources.gutendex_results {
                    ui.horizontal(|ui| {
                        let importable = hit.is_importable();
                        if ui
                            .add_enabled(importable, egui::Button::new("⬇ Import"))
                            .on_hover_text(if importable {
                                "Download and add to the library"
                            } else {
                                "No importable text format for this book"
                            })
                            .clicked()
                        {
                            *import = Some(SourceImport::Gutendex(hit.clone()));
                        }
                        ui.label(egui::RichText::new(&hit.title).strong());
                        if !hit.author.is_empty() {
                            ui.weak(&hit.author);
                        }
                    });
                    ui.weak(format!("{} downloads", hit.download_count));
                    ui.add_space(4.0);
                }
            });
    }

    fn opds_ui(
        &mut self,
        ui: &mut egui::Ui,
        catalogs: &[OpdsCatalog],
        import: &mut Option<SourceImport>,
        opds_add: &mut Option<(String, String)>,
        opds_remove: &mut Option<usize>,
    ) {
        // Distributor selector + management.
        if catalogs.is_empty() {
            ui.weak(
                "No OPDS distributors yet for this language. Add a catalog feed \
                 URL below, or pick a suggestion.",
            );
        } else {
            if self.sources.opds_selected >= catalogs.len() {
                self.sources.opds_selected = 0;
            }
            let prev_selected = self.sources.opds_selected;
            ui.horizontal(|ui| {
                ui.label("Distributor:");
                let selected = catalogs[self.sources.opds_selected].name.clone();
                egui::ComboBox::from_id_salt("opds-distributor")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (i, cat) in catalogs.iter().enumerate() {
                            ui.selectable_value(&mut self.sources.opds_selected, i, &cat.name);
                        }
                    });
                if ui
                    .button("🗑")
                    .on_hover_text("Remove this distributor")
                    .clicked()
                {
                    *opds_remove = Some(self.sources.opds_selected);
                }
            });
            // A different distributor is selected: its results aren't these.
            if self.sources.opds_selected != prev_selected {
                self.sources.opds_results.clear();
            }
            ui.add_space(6.0);

            if self.sources.opds_results.is_empty() {
                ui.weak("Enter a search above to query this distributor.");
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for hit in &self.sources.opds_results {
                            let best = hit.best_link().cloned();
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(best.is_some(), egui::Button::new("⬇ Import"))
                                    .clicked()
                                {
                                    if let Some((mime, url)) = &best {
                                        *import = Some(SourceImport::Opds {
                                            url: url.clone(),
                                            mime: mime.clone(),
                                            title: hit.title.clone(),
                                            author: hit.author.clone(),
                                        });
                                    }
                                }
                                ui.label(egui::RichText::new(&hit.title).strong());
                                if !hit.author.is_empty() {
                                    ui.weak(&hit.author);
                                }
                            });
                            if !hit.summary.is_empty() {
                                ui.weak(truncate(&hit.summary, 240));
                            }
                            ui.add_space(4.0);
                        }
                    });
            }
        }

        // Add-distributor form.
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Add a distributor").strong());
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.sources.new_opds_name)
                    .hint_text("name")
                    .desired_width(140.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.sources.new_opds_url)
                    .hint_text("OPDS feed URL (https://…)")
                    .desired_width(300.0),
            );
            let url_ok = self.sources.new_opds_url.trim().starts_with("http");
            if ui
                .add_enabled(url_ok, egui::Button::new("Add"))
                .clicked()
            {
                *opds_add = Some((
                    self.sources.new_opds_name.clone(),
                    self.sources.new_opds_url.clone(),
                ));
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.weak("Suggestions:");
            for (name, url) in OPDS_SUGGESTIONS {
                let already = catalogs.iter().any(|c| c.url.eq_ignore_ascii_case(url));
                if ui
                    .add_enabled(!already, egui::Button::new(*name))
                    .on_hover_text(*url)
                    .clicked()
                {
                    *opds_add = Some((name.to_string(), url.to_string()));
                }
            }
        });
    }

    fn libraries_list(&mut self, ui: &mut egui::Ui, code: &str, lang_name: &str) {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if let Some(note) = shiori_app::cross_reference(code) {
                    ui.weak(note);
                    ui.add_space(6.0);
                }
                let dedicated = shiori_app::suggested_libraries(code);
                if !dedicated.is_empty() {
                    ui.label(egui::RichText::new(format!("{lang_name} collections")).strong());
                    ui.add_space(4.0);
                    for lib in dedicated {
                        library_row(ui, lib);
                    }
                    ui.add_space(10.0);
                }
                ui.label(egui::RichText::new("Multilingual libraries").strong());
                ui.add_space(4.0);
                for lib in shiori_app::multilingual_libraries() {
                    library_row(ui, lib);
                }
                ui.add_space(8.0);
                ui.weak(
                    "These are external sites. Open one in your browser to \
                     download a book, then import the file from the Library page \
                     — or add its OPDS feed under the OPDS tab to search it here.",
                );
            });
    }
}

fn tab_label(tab: SourceTab) -> &'static str {
    match tab {
        SourceTab::Aozora => "青空文庫",
        SourceTab::Wikisource => "Wikisource",
        SourceTab::Gutenberg => "Project Gutenberg",
        SourceTab::Opds => "OPDS",
        SourceTab::Libraries => "Libraries",
    }
}

fn library_row(ui: &mut egui::Ui, lib: &shiori_app::Library) {
    ui.horizontal_wrapped(|ui| {
        ui.hyperlink_to(egui::RichText::new(&lib.name).strong(), &lib.url);
        if !lib.access.is_empty() {
            ui.weak(format!("· {}", lib.access.replace('_', " ")));
        }
    });
    if !lib.description.is_empty() {
        ui.weak(&lib.description);
    }
    ui.add_space(6.0);
}

/// Trim a string to `max` chars on a char boundary, adding an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}
