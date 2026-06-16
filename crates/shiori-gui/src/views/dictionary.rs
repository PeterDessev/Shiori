//! Dictionary view: one search box answering with word entries and kanji
//! cards (readings, meanings, grade, and stroke-order diagrams drawn
//! from KanjiVG path data).
//!
//! The box accepts kanji, kana, and rōmaji, and understands conjugated
//! input: a query like 食べました or `tabemashita` leads with its root
//! 食べる and a banner explaining the form. Each word entry shows its part
//! of speech and transitivity, its JLPT level, and a toggle for example
//! sentences drawn from the user's library (the SRS material).

use eframe::egui;
use shiori_core::{KnowledgeStatus, PartOfSpeech, WordId};

use crate::app::ShioriGui;

/// How many library sentences to pull per word. The card shows the four
/// shortest of these; the "more info" modal shows them all.
const EXAMPLE_POOL: u32 = 50;

impl ShioriGui {
    pub fn show_dictionary(&mut self, ctx: &egui::Context) {
        // Re-search whenever the query no longer matches the results.
        if self.dictionary.query != self.dictionary.searched_for {
            let query = self.dictionary.query.clone();
            if let Some(results) = self.with_app(|app| app.search_dictionary(&query)) {
                self.dictionary.results = results;
                self.dictionary.searched_for = query;
                // Collapse example panels carried over from the last query.
                self.dictionary.examples_open.clear();
            }
        }

        let mut learn_headword: Option<String> = None;
        let mut toggle_examples: Option<i64> = None;
        let mut open_info: Option<usize> = None;
        let central = egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("Dictionary");
                ui.add_space(8.0);
                let response = ui.add_sized(
                    [(ui.available_width() - 80.0).clamp(220.0, 420.0), 24.0],
                    egui::TextEdit::singleline(&mut self.dictionary.query)
                        .hint_text("猫, ねこ, neko, 食べました, tabemashita…"),
                );
                if ui.button("✕").clicked() {
                    self.dictionary.query.clear();
                    response.request_focus();
                }
            });
            ui.add_space(8.0);

            if !self.dict_ready() {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 160, 60),
                    "No dictionary installed — retry the download from the banner.",
                );
                return;
            }
            if self.dictionary.query.trim().is_empty() {
                ui.weak(
                    "Search by kanji, kana, or rōmaji (Neko → ネコ). Conjugated forms \
                     find their dictionary root. Prefix matches included.",
                );
                return;
            }

            let results = &self.dictionary.results;
            if results.words.is_empty() && results.kanji.is_empty() {
                ui.weak("No matches.");
                return;
            }

            // Banner explaining a conjugated/compounded query.
            if let Some(analysis) = &results.analysis {
                form_banner(ui, analysis, &results.words);
                ui.add_space(8.0);
            }

            ui.columns(2, |columns| {
                // Left: word entries.
                egui::ScrollArea::vertical()
                    .id_salt("dict-words")
                    .auto_shrink([false; 2])
                    .show(&mut columns[0], |ui| {
                        for (i, hit) in results.words.iter().enumerate() {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                // Title row: headword and chips on the left, a
                                // "more info" button pinned to the top-right.
                                ui.horizontal(|ui| {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button("🔎")
                                                .on_hover_text("More info")
                                                .clicked()
                                            {
                                                open_info = Some(i);
                                            }
                                            ui.with_layout(
                                                egui::Layout::left_to_right(egui::Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(hit.entry.headword())
                                                            .size(22.0)
                                                            .strong(),
                                                    );
                                                    let kana = hit
                                                        .entry
                                                        .kana
                                                        .first()
                                                        .map(|k| k.text.as_str())
                                                        .unwrap_or("");
                                                    if !kana.is_empty()
                                                        && kana != hit.entry.headword()
                                                    {
                                                        ui.label(format!("（{kana}）"));
                                                    }
                                                    if let Some(level) = hit.jlpt {
                                                        jlpt_chip(ui, level);
                                                    }
                                                    if let Some(word) = &hit.word {
                                                        ui.weak(format!(
                                                            "· {}",
                                                            word.status.as_str()
                                                        ));
                                                    }
                                                },
                                            );
                                        },
                                    );
                                });

                                // Part of speech / transitivity chips.
                                let pos = hit.entry.pos_labels();
                                if !pos.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        for label in &pos {
                                            pos_chip(ui, label);
                                        }
                                    });
                                }

                                for (i, sense) in hit.entry.senses.iter().take(3).enumerate() {
                                    let glosses: Vec<&str> =
                                        sense.gloss.iter().map(|g| g.text.as_str()).collect();
                                    if !glosses.is_empty() {
                                        ui.label(format!("{}. {}", i + 1, glosses.join("; ")));
                                    }
                                }

                                ui.horizontal_wrapped(|ui| {
                                    let learnable = match &hit.word {
                                        Some(w) => w.status != KnowledgeStatus::Learning,
                                        None => true,
                                    };
                                    if learnable && ui.small_button("➕ Learn (SRS)").clicked() {
                                        learn_headword = Some(hit.entry.headword().to_string());
                                    }
                                    if let Some(word) = &hit.word {
                                        let open =
                                            self.dictionary.examples_open.contains(&word.id.0);
                                        let label = if open {
                                            "▼ Example sentences"
                                        } else {
                                            "▶ Example sentences"
                                        };
                                        if ui.small_button(label).clicked() {
                                            toggle_examples = Some(word.id.0);
                                        }
                                    }
                                });

                                // Expanded example sentences from the library.
                                if let Some(word) = &hit.word {
                                    if self.dictionary.examples_open.contains(&word.id.0) {
                                        example_panel(ui, self.dictionary.examples.get(&word.id.0));
                                    }
                                }
                            });
                            ui.add_space(6.0);
                        }
                    });

                // Right: kanji cards.
                egui::ScrollArea::vertical()
                    .id_salt("dict-kanji")
                    .auto_shrink([false; 2])
                    .show(&mut columns[1], |ui| {
                        for kanji in &results.kanji {
                            kanji_card(ui, kanji);
                            ui.add_space(8.0);
                        }
                    });
            });
        });

        if let Some(word_id) = toggle_examples {
            self.toggle_dictionary_examples(word_id);
        }
        if let Some(headword) = learn_headword {
            self.learn_from_dictionary(&headword);
        }
        if let Some(index) = open_info {
            self.open_info_modal(index);
        }
        // The central panel's rect is exactly the dictionary area (right of
        // the nav rail, below any banners) — center the modal within it.
        self.show_info_modal(ctx, central.response.rect);
    }

    /// Expand or collapse the example-sentence panel for a word, fetching
    /// its library sentences from the database the first time.
    fn toggle_dictionary_examples(&mut self, word_id: i64) {
        if self.dictionary.examples_open.remove(&word_id) {
            return; // was open — collapse
        }
        self.dictionary.examples_open.insert(word_id);
        self.ensure_examples(word_id);
    }

    /// Fetch and cache a word's library example sentences (with highlight
    /// ranges) if not already loaded. One fetch feeds both the card's
    /// shortest-four list and the modal's full list.
    fn ensure_examples(&mut self, word_id: i64) {
        if self.dictionary.examples.contains_key(&word_id) {
            return;
        }
        if let Some(list) = self.with_app(|app| app.word_examples(WordId(word_id), EXAMPLE_POOL)) {
            self.dictionary.examples.insert(word_id, list);
        }
    }

    /// Capture the card at `index` into the "more info" modal: snapshot its
    /// textual content, ensure its example sentences are cached, and resolve
    /// a kanji card for each distinct kanji in the headword.
    fn open_info_modal(&mut self, index: usize) {
        let (word_id, headword, reading, jlpt, status, pos, senses, kanji_chars) = {
            let Some(hit) = self.dictionary.results.words.get(index) else {
                return;
            };
            let headword = hit.entry.headword().to_string();
            let kana = hit
                .entry
                .kana
                .first()
                .map(|k| k.text.clone())
                .unwrap_or_default();
            let reading = if !kana.is_empty() && kana != headword {
                kana
            } else {
                String::new()
            };
            let senses: Vec<String> = hit
                .entry
                .senses
                .iter()
                .take(3)
                .enumerate()
                .filter_map(|(i, sense)| {
                    let glosses: Vec<&str> = sense.gloss.iter().map(|g| g.text.as_str()).collect();
                    (!glosses.is_empty()).then(|| format!("{}. {}", i + 1, glosses.join("; ")))
                })
                .collect();
            // Distinct kanji of the headword, in order.
            let mut kanji_chars: Vec<char> = Vec::new();
            for c in headword.chars() {
                if is_kanji(c) && !kanji_chars.contains(&c) {
                    kanji_chars.push(c);
                }
            }
            (
                hit.word.as_ref().map(|w| w.id.0),
                headword,
                reading,
                hit.jlpt,
                hit.word.as_ref().map(|w| w.status.as_str().to_string()),
                hit.entry.pos_labels(),
                senses,
                kanji_chars,
            )
        };

        if let Some(id) = word_id {
            self.ensure_examples(id);
        }
        let kanji = self
            .with_app(|app| {
                let mut rows = Vec::new();
                for c in &kanji_chars {
                    if let Some(row) = app.db().kanji(&c.to_string())? {
                        rows.push(row);
                    }
                }
                Ok(rows)
            })
            .unwrap_or_default();

        self.dictionary.info = Some(crate::app::DictInfoModal {
            word_id,
            headword,
            reading,
            jlpt,
            status,
            pos,
            senses,
            kanji,
        });
        self.dictionary.info_just_opened = true;
    }

    /// Render the "more info" modal, if open: a borderless window filling
    /// most of `area` (the dictionary panel, minus the nav rail) with a
    /// compact header, the card content and every example sentence on the
    /// left, and the word's kanji cards on the right. Clicking outside it
    /// (but not on the nav rail) or pressing Escape closes it.
    fn show_info_modal(&mut self, ctx: &egui::Context, area: egui::Rect) {
        if self.dictionary.info.is_none() {
            return;
        }
        // The click that opened the modal must not count as a click-away.
        let just_opened = std::mem::take(&mut self.dictionary.info_just_opened);

        // Center the modal in the dictionary area with an even margin on
        // every side (measured from the nav rail, not the screen edge).
        let margin = 44.0;
        let size = egui::vec2(
            (area.width() - 2.0 * margin).max(240.0),
            (area.height() - 2.0 * margin).max(240.0),
        );
        // Anchor the window's centre to the centre of `area` — the screen
        // centre shifted by whatever offset the nav rail (and banners)
        // introduce — so it stays centred whatever its size.
        let anchor_offset = area.center() - ctx.screen_rect().center();

        let mut close = false;
        let win = {
            let info = self.dictionary.info.as_ref().expect("checked above");
            let examples = info
                .word_id
                .and_then(|id| self.dictionary.examples.get(&id));
            egui::Window::new("Word details")
                .title_bar(false)
                .resizable(false)
                .collapsible(false)
                .movable(false)
                .anchor(egui::Align2::CENTER_CENTER, anchor_offset)
                .fixed_size(size)
                .show(ctx, |ui| {
                    // Compact header standing in for a title bar.
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("✕").on_hover_text("Close (Esc)").clicked() {
                                close = true;
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(&info.headword).size(20.0).strong(),
                                    );
                                    if !info.reading.is_empty() {
                                        ui.label(format!("（{}）", info.reading));
                                    }
                                    if let Some(level) = info.jlpt {
                                        jlpt_chip(ui, level);
                                    }
                                    if let Some(status) = &info.status {
                                        ui.weak(format!("· {status}"));
                                    }
                                },
                            );
                        });
                    });
                    ui.separator();
                    info_modal_body(ui, info, examples);
                })
        };

        // Close on a click outside the window that lands in the dictionary
        // area (never the nav rail, so the user can switch tabs and come
        // back to a still-open modal), or on Escape.
        if let (false, Some(win)) = (just_opened, &win) {
            let rect = win.response.rect;
            let clicked_away = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .press_origin()
                        .is_some_and(|p| area.contains(p) && !rect.contains(p))
            });
            if clicked_away {
                close = true;
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if close {
            self.dictionary.info = None;
        }
    }

    /// Re-run the current dictionary search and reload cached example
    /// sentences. Called when the Dictionary tab is re-entered so words
    /// freshly added to the SRS (and new sentences) appear, while keeping
    /// the query, expanded panels, and any open "more info" modal intact.
    pub(crate) fn reenter_dictionary(&mut self) {
        let query = self.dictionary.query.clone();
        if !query.trim().is_empty() {
            if let Some(results) = self.with_app(|app| app.search_dictionary(&query)) {
                self.dictionary.results = results;
                self.dictionary.searched_for = query;
            }
        }
        let ids: Vec<i64> = self.dictionary.examples.keys().copied().collect();
        for id in ids {
            if let Some(list) = self.with_app(|app| app.word_examples(WordId(id), EXAMPLE_POOL)) {
                self.dictionary.examples.insert(id, list);
            }
        }
    }

    /// Put a dictionary search hit into the SRS: derive the canonical
    /// word key by running the headword through the analyzer, create the
    /// word if it has never been seen, and start a context-free card.
    fn learn_from_dictionary(&mut self, headword: &str) {
        let key = self.with_app(|app| {
            let analyzed = app.analyze_chat_text(headword)?;
            let key = analyzed
                .first()
                .and_then(|(tokens, _)| tokens.first())
                .map(|row| shiori_core::WordKey {
                    lemma: row.token.lemma.clone(),
                    reading: row.token.reading.clone(),
                    pos: row.token.pos,
                })
                .unwrap_or_else(|| shiori_core::WordKey {
                    lemma: headword.to_string(),
                    reading: String::new(),
                    pos: shiori_core::PartOfSpeech::Noun,
                });
            Ok(key)
        });
        let Some(key) = key else { return };
        let Some(word) = self.with_app(|app| app.ensure_word(&key)) else {
            return;
        };
        if self
            .with_app(|app| app.start_learning_uncontexted(word.id))
            .is_some()
        {
            // Refresh the hit's status chip.
            self.dictionary.searched_for.clear();
            self.refresh_caches();
        }
    }
}

/// Banner above the results explaining a conjugated/compounded query:
/// the typed form, its dictionary root, what kind of word it is, and the
/// grammar of its tail.
fn form_banner(
    ui: &mut egui::Ui,
    analysis: &shiori_app::QueryAnalysis,
    words: &[shiori_app::DictSearchHit],
) {
    let accent = egui::Color32::from_rgb(80, 140, 240);
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgba_unmultiplied(80, 140, 240, 18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&analysis.surface).size(20.0).strong());
                ui.label(egui::RichText::new("→").size(18.0).color(accent));
                ui.label(
                    egui::RichText::new(&analysis.lemma)
                        .size(20.0)
                        .strong()
                        .color(accent),
                );
                if !analysis.reading.is_empty() && analysis.reading != analysis.lemma {
                    ui.weak(format!("（{}）", analysis.reading));
                }
            });

            // Part of speech of the root: prefer the resolved entry's
            // detailed labels (with transitivity), else the coarse class.
            // An all-kana lemma (たべる) matches the entry by reading even
            // though its headword is the kanji spelling (食べる).
            let root_pos = words
                .iter()
                .find(|h| {
                    h.entry.headword() == analysis.lemma || h.entry.reading() == analysis.lemma
                })
                .map(|h| h.entry.pos_labels())
                .filter(|p| !p.is_empty());
            ui.horizontal_wrapped(|ui| match root_pos {
                Some(labels) => {
                    for label in &labels {
                        pos_chip(ui, label);
                    }
                }
                None => pos_chip(ui, coarse_pos_label(analysis.pos)),
            });

            // The grammar of the conjugation.
            if let Some(summary) = &analysis.inflection.summary {
                ui.label(summary);
            }
            for part in &analysis.inflection.parts {
                ui.weak(part);
            }
        });
}

/// Expanded list of library sentences using the word — only the four
/// shortest, the word highlighted in each — or a hint when the word has not
/// turned up in anything imported yet.
fn example_panel(ui: &mut egui::Ui, examples: Option<&Vec<shiori_app::DictExample>>) {
    ui.add_space(4.0);
    match examples {
        Some(list) if !list.is_empty() => {
            // The shortest four are the easiest to read.
            let mut order: Vec<&shiori_app::DictExample> = list.iter().collect();
            order.sort_by_key(|ex| ex.sentence.text.chars().count());
            for ex in order.into_iter().take(4) {
                example_line(ui, ex, 16.0);
                ui.add_space(2.0);
            }
        }
        Some(_) => {
            ui.weak("Not in your library yet — examples appear once the word turns up in a book you've imported.");
        }
        None => {
            ui.weak("Loading…");
        }
    }
}

/// One example sentence with its source title, the looked-up word coloured.
fn example_line(ui: &mut egui::Ui, ex: &shiori_app::DictExample, size: f32) {
    highlighted_sentence(ui, &ex.sentence.text, &ex.highlights, size);
    ui.weak(format!("— {}", ex.title));
}

/// Render a sentence as a single wrapping label, painting the byte ranges
/// in `highlights` in the accent colour and the rest in the normal text
/// colour. Out-of-bounds, mis-aligned, or overlapping ranges are skipped.
fn highlighted_sentence(ui: &mut egui::Ui, text: &str, highlights: &[(usize, usize)], size: f32) {
    let accent = egui::Color32::from_rgb(80, 140, 240);
    let base = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();

    let mut spans: Vec<(usize, usize)> = highlights
        .iter()
        .copied()
        .filter(|&(s, e)| {
            s < e && e <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(e)
        })
        .collect();
    spans.sort_by_key(|&(s, _)| s);

    let mut pos = 0usize;
    for (s, e) in spans {
        if s < pos {
            continue; // overlaps the previous span
        }
        if s > pos {
            append_run(&mut job, &text[pos..s], base, size);
        }
        append_run(&mut job, &text[s..e], accent, size);
        pos = e;
    }
    if pos < text.len() {
        append_run(&mut job, &text[pos..], base, size);
    }
    ui.label(job);
}

/// Append one coloured run to a layout job.
fn append_run(job: &mut egui::text::LayoutJob, text: &str, color: egui::Color32, size: f32) {
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
}

/// Body of the "more info" modal: a split view — part of speech, senses,
/// and every example sentence on the left, the word's kanji cards on the
/// right. The headword itself sits in the modal's compact header.
fn info_modal_body(
    ui: &mut egui::Ui,
    info: &crate::app::DictInfoModal,
    examples: Option<&Vec<shiori_app::DictExample>>,
) {
    ui.columns(2, |columns| {
        // Left: the card's text content, plus every example sentence.
        egui::ScrollArea::vertical()
            .id_salt("dict-info-left")
            .auto_shrink([false; 2])
            .show(&mut columns[0], |ui| {
                // Keep the text off the divider down the middle of the modal.
                ui.set_width((ui.available_width() - 14.0).max(80.0));
                if !info.pos.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for label in &info.pos {
                            pos_chip(ui, label);
                        }
                    });
                }
                for sense in &info.senses {
                    ui.label(sense);
                }

                ui.add_space(8.0);
                ui.label(egui::RichText::new("Example sentences").small().strong());
                ui.add_space(2.0);
                match examples {
                    Some(list) if !list.is_empty() => {
                        for ex in list {
                            example_line(ui, ex, 15.0);
                            ui.add_space(4.0);
                        }
                    }
                    Some(_) => {
                        ui.weak("Not in your library yet — examples appear once the word turns up in a book you've imported.");
                    }
                    None => {
                        ui.weak("No example sentences — this word isn't being tracked yet.");
                    }
                }
            });

        // Right: a kanji card for each kanji in the word.
        egui::ScrollArea::vertical()
            .id_salt("dict-info-kanji")
            .auto_shrink([false; 2])
            .show(&mut columns[1], |ui| {
                if info.kanji.is_empty() {
                    ui.weak("No kanji in this word.");
                } else {
                    for kanji in &info.kanji {
                        kanji_card(ui, kanji);
                        ui.add_space(8.0);
                    }
                }
            });
    });
}

/// Coarse part-of-speech name for a form's analyzed head, used when no
/// dictionary entry refines it.
fn coarse_pos_label(pos: PartOfSpeech) -> &'static str {
    match pos {
        PartOfSpeech::Noun => "noun",
        PartOfSpeech::ProperNoun => "proper noun",
        PartOfSpeech::Pronoun => "pronoun",
        PartOfSpeech::DependentNoun => "dependent noun",
        PartOfSpeech::Verb => "verb",
        PartOfSpeech::Adjective => "i-adjective",
        PartOfSpeech::AdjectivalNoun => "na-adjective",
        PartOfSpeech::Adverb => "adverb",
        PartOfSpeech::Particle => "particle",
        PartOfSpeech::AuxiliaryVerb => "auxiliary verb",
        PartOfSpeech::Conjunction => "conjunction",
        PartOfSpeech::Prenominal => "pre-noun adjectival",
        PartOfSpeech::Interjection => "interjection",
        PartOfSpeech::Number => "numeric",
        PartOfSpeech::Prefix => "prefix",
        PartOfSpeech::Suffix => "suffix",
        PartOfSpeech::Symbol => "symbol",
        PartOfSpeech::Unknown => "unknown",
    }
}

/// A small blue chip naming a part of speech / transitivity.
fn pos_chip(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .small()
            .background_color(egui::Color32::from_rgba_unmultiplied(80, 140, 240, 45)),
    );
}

/// Whether a character is a CJK ideograph (kanji), matching the analyzer's
/// `kana::contains_kanji` so a word's kanji cards line up with its tokens.
fn is_kanji(c: char) -> bool {
    let u = c as u32;
    (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u) || u == 0x3005
}

/// A green chip for the word's JLPT level (5 = N5 … 1 = N1).
fn jlpt_chip(ui: &mut egui::Ui, level: u8) {
    ui.label(
        egui::RichText::new(format!("JLPT N{level}"))
            .small()
            .background_color(egui::Color32::from_rgba_unmultiplied(110, 180, 110, 70)),
    );
}

/// One kanji card: stroke diagram, readings, meanings, classifications.
fn kanji_card(ui: &mut egui::Ui, kanji: &shiori_db::KanjiRow) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if kanji.strokes.is_empty() {
                ui.label(egui::RichText::new(&kanji.literal).size(72.0));
            } else {
                draw_kanji_strokes(ui, &kanji.strokes, 96.0);
            }
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&kanji.literal).size(26.0).strong());
                if !kanji.on_readings.is_empty() {
                    ui.label(format!("音: {}", kanji.on_readings.join("、")));
                }
                if !kanji.kun_readings.is_empty() {
                    ui.label(format!("訓: {}", kanji.kun_readings.join("、")));
                }
                if !kanji.nanori.is_empty() {
                    ui.weak(format!("名乗り: {}", kanji.nanori.join("、")));
                }
            });
        });
        if !kanji.meanings.is_empty() {
            ui.label(kanji.meanings.join("; "));
        }
        ui.horizontal_wrapped(|ui| {
            ui.weak(format!("{} strokes", kanji.stroke_count));
            if let Some(grade) = kanji.grade {
                ui.weak(match grade {
                    1..=6 => format!("· grade {grade}"),
                    8 => "· jōyō".to_string(),
                    9 | 10 => "· jinmeiyō".to_string(),
                    g => format!("· grade {g}"),
                });
            }
            if let Some(jlpt) = kanji.jlpt {
                ui.weak(format!("· old JLPT {jlpt}"));
            }
            if let Some(freq) = kanji.freq {
                ui.weak(format!("· freq #{freq}"));
            }
        });
        if !kanji.variants.is_empty() {
            ui.weak(format!(
                "variant/archaic forms: {}",
                kanji.variants.join("、")
            ));
        }
    });
}

/// Paint KanjiVG strokes into a square. Path data lives in a 0–109
/// coordinate space; strokes shade from accent (first) to gray (last)
/// and carry their stroke number at the starting point.
fn draw_kanji_strokes(ui: &mut egui::Ui, strokes: &[String], size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.4)),
        egui::StrokeKind::Inside,
    );
    let scale = size / 109.0;
    let to_screen = |p: kurbo::Point| {
        egui::pos2(
            rect.left() + p.x as f32 * scale,
            rect.top() + p.y as f32 * scale,
        )
    };
    let accent = egui::Color32::from_rgb(80, 140, 240);
    let done = ui.visuals().weak_text_color();
    let n = strokes.len().max(1) as f32;

    for (i, d) in strokes.iter().enumerate() {
        let Ok(path) = kurbo::BezPath::from_svg(d) else {
            continue;
        };
        let t = i as f32 / n;
        let color = egui::Color32::from_rgb(
            (accent.r() as f32 * (1.0 - t) + done.r() as f32 * t) as u8,
            (accent.g() as f32 * (1.0 - t) + done.g() as f32 * t) as u8,
            (accent.b() as f32 * (1.0 - t) + done.b() as f32 * t) as u8,
        );
        let stroke = egui::Stroke::new((2.6 * scale).max(1.4), color);

        let mut points: Vec<egui::Pos2> = Vec::new();
        let mut start: Option<kurbo::Point> = None;
        kurbo::flatten(path.iter(), 0.2, |el| match el {
            kurbo::PathEl::MoveTo(p) => {
                if points.len() > 1 {
                    painter.add(egui::Shape::line(points.clone(), stroke));
                }
                points = vec![to_screen(p)];
                start.get_or_insert(p);
            }
            kurbo::PathEl::LineTo(p) => points.push(to_screen(p)),
            _ => {}
        });
        if points.len() > 1 {
            painter.add(egui::Shape::line(points, stroke));
        }
        // Stroke number near the start of the stroke.
        if size >= 64.0 {
            if let Some(p) = start {
                painter.text(
                    to_screen(p) + egui::vec2(-2.0, -2.0),
                    egui::Align2::RIGHT_BOTTOM,
                    (i + 1).to_string(),
                    egui::FontId::proportional(9.0),
                    color,
                );
            }
        }
    }
}
