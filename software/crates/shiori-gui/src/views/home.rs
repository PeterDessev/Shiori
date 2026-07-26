//! Home view: the landing page — active language at a glance, today's
//! review load, pick-up-where-you-left-off, and the reading calendar.

use eframe::egui;
use shiori_core::DocumentId;

use crate::app::{ShioriGui, View};

/// Seconds per card assumed until enough reviews exist to measure the
/// user's real pace.
const DEFAULT_SECONDS_PER_CARD: f64 = 10.0;

/// What the continue-reading card shows, copied out of the home cache so
/// the UI closure doesn't hold a borrow of `self`.
struct ContinueView {
    id: DocumentId,
    title: String,
    author: String,
    progress: f32,
    remaining_line: String,
    unknown_words: u32,
    band: shiori_app::DifficultyBand,
    time_line: Option<String>,
}

impl ShioriGui {
    pub fn show_home(&mut self, ctx: &egui::Context) {
        if self.home.is_none() {
            self.refresh_home();
        }

        let languages: Vec<(String, String)> = self
            .lang_infos
            .iter()
            .map(|i| (i.lang.clone(), i.name.clone()))
            .collect();
        let active = self.settings.active_language.clone();
        let active_name = languages
            .iter()
            .find(|(code, _)| *code == active)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| active.clone());

        // Copy everything the frame shows out of the cache.
        let (due_today, pace_seconds, reading_by_day, cont) = match &self.home {
            Some(h) => (
                h.due_today,
                h.pace_seconds,
                h.reading_by_day.clone(),
                h.cont.as_ref().map(|c| ContinueView {
                    id: c.summary.document.id,
                    title: c.summary.document.title.clone(),
                    author: c.summary.document.author.clone(),
                    progress: c.progress as f32,
                    remaining_line: remaining_line(c),
                    unknown_words: c.remaining_unknown_words,
                    band: c.stats.band,
                    time_line: (c.reading.seconds > 0.0).then(|| {
                        format!(
                            "{} in this book so far",
                            crate::views::human_duration(chrono::Duration::seconds(
                                c.reading.seconds as i64
                            ))
                        )
                    }),
                }),
            ),
            None => (0, None, Vec::new(), None),
        };
        let due_now = self.due_count;
        let library_empty = self.library.is_empty();

        let mut switch_to: Option<String> = None;
        let mut open_languages = false;
        let mut start_review = false;
        let mut open_library = false;
        let mut open_doc: Option<DocumentId> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.heading("Home");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("🌐 Manage languages")
                                .on_hover_text("Install, download, and remove language packs")
                                .clicked()
                            {
                                open_languages = true;
                            }
                            egui::ComboBox::from_id_salt("home-language")
                                .selected_text(active_name)
                                .show_ui(ui, |ui| {
                                    for (code, name) in &languages {
                                        if ui.selectable_label(*code == active, name).clicked()
                                            && *code != active
                                        {
                                            switch_to = Some(code.clone());
                                        }
                                    }
                                });
                            ui.label("Language:");
                        });
                    });
                    ui.add_space(10.0);

                    // ── Continue reading ─────────────────────────────
                    ui.heading("Continue reading");
                    ui.add_space(4.0);
                    match &cont {
                        Some(c) => {
                            ui.group(|ui| {
                                ui.set_width(ui.available_width().min(560.0));
                                ui.horizontal(|ui| {
                                    ui.strong(&c.title);
                                    if !c.author.is_empty() {
                                        ui.weak(format!("— {}", c.author));
                                    }
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::ProgressBar::new(c.progress)
                                        .desired_width(320.0)
                                        .text(format!("{:.0}% read", c.progress * 100.0)),
                                );
                                ui.add_space(4.0);
                                ui.label(&c.remaining_line);
                                ui.horizontal(|ui| {
                                    ui.label(format!("≈ {} unknown words ahead", c.unknown_words));
                                    ui.weak("·");
                                    ui.colored_label(
                                        crate::views::band_color(c.band),
                                        c.band.label(),
                                    );
                                });
                                if let Some(line) = &c.time_line {
                                    ui.weak(line);
                                }
                                ui.add_space(6.0);
                                if ui.button("📖 Continue reading").clicked() {
                                    open_doc = Some(c.id);
                                }
                            });
                        }
                        None => {
                            ui.group(|ui| {
                                ui.set_width(ui.available_width().min(560.0));
                                if library_empty {
                                    ui.label(
                                        "Your library is empty — import a book or find \
                                         one online, then your current read lives here.",
                                    );
                                } else {
                                    ui.label(
                                        "Nothing in progress — open something from the \
                                         library and it will be waiting here.",
                                    );
                                }
                                ui.add_space(4.0);
                                if ui.button("📚 Open library").clicked() {
                                    open_library = true;
                                }
                            });
                        }
                    }

                    // ── Reviews ──────────────────────────────────────
                    ui.add_space(14.0);
                    ui.heading("Reviews");
                    ui.add_space(4.0);
                    ui.group(|ui| {
                        ui.set_width(ui.available_width().min(560.0));
                        if due_today == 0 {
                            ui.label("Nothing due today — all caught up 🎉");
                        } else {
                            let est =
                                due_today as f64 * pace_seconds.unwrap_or(DEFAULT_SECONDS_PER_CARD);
                            let est_text =
                                crate::views::human_duration(chrono::Duration::seconds(est as i64));
                            ui.horizontal(|ui| {
                                ui.strong(format!(
                                    "{due_today} card{} due today",
                                    if due_today == 1 { "" } else { "s" }
                                ));
                                ui.weak(match pace_seconds {
                                    Some(_) => format!("· ≈ {est_text} at your usual pace"),
                                    None => format!("· ≈ {est_text} (rough estimate)"),
                                });
                            });
                            if due_now == 0 {
                                ui.weak("They become due later today.");
                            } else if due_now != due_today {
                                ui.weak(format!("{due_now} ready now, the rest later today."));
                            }
                            ui.add_space(4.0);
                            if ui
                                .add_enabled(due_now > 0, egui::Button::new("🔁 Review now"))
                                .clicked()
                            {
                                start_review = true;
                            }
                        }
                    });

                    // ── Reading activity ─────────────────────────────
                    ui.add_space(14.0);
                    ui.heading("Reading activity");
                    ui.add_space(4.0);
                    if reading_by_day.is_empty() {
                        ui.weak(
                            "No reading time recorded yet — the clock runs while a \
                             book is open in the reader.",
                        );
                    } else {
                        crate::views::reading_heatmap(ui, &reading_by_day);
                    }
                    ui.add_space(8.0);
                });
        });

        if let Some(code) = switch_to {
            self.switch_language(ctx, &code);
        }
        if open_languages {
            self.settings_category = crate::views::SettingsCategory::Languages;
            self.view = View::Settings;
        }
        if start_review {
            self.load_review_queue();
            self.view = View::Review;
        }
        if open_library {
            self.refresh_caches();
            self.view = View::Library;
        }
        if let Some(id) = open_doc {
            self.open_reader(id);
        }
    }
}

/// The "what's left" sentence of the continue-reading card.
fn remaining_line(c: &shiori_app::ContinueReading) -> String {
    match c.est_remaining_seconds {
        Some(secs) => format!(
            "About {} of reading left ({} characters)",
            crate::views::human_duration(chrono::Duration::seconds(secs as i64)),
            c.remaining_chars
        ),
        None => format!(
            "{} characters to go — a time estimate appears after ~10 \
             minutes of reading",
            c.remaining_chars
        ),
    }
}
