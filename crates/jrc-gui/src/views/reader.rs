//! Reader view: flowing paragraphs of clickable text with a dictionary
//! side panel for the selected phrase.
//!
//! Phrases (conjugated verbs with their endings, noun+suffix compounds)
//! are selected as a unit, highlighted in a single color, and the panel
//! explains the conjugation. No permanent per-status tinting — an optional
//! settings toggle can mark unknown words.

use eframe::egui;
use jrc_core::{KnowledgeStatus, WordId};
use jrc_dict::register::UsageProfile;

use crate::app::JrcGui;
use crate::settings::shortcut_pressed;
use crate::views::{tight_highlight_rect, unknown_fill};

/// Base vertical gap after each paragraph (scaled by the line-spacing
/// setting).
const PARA_GAP: f32 = 14.0;
/// Base spacing horizontal_wrapped inserts between wrapped rows (scaled
/// by the line-spacing setting).
const ROW_GAP: f32 = 8.0;

/// Action chosen in the dictionary panel.
enum WordAction {
    Learn(WordId, jrc_core::SentenceId),
    Known(WordId),
    Ignore(WordId),
    Reset(WordId),
    Forgot(WordId, Option<jrc_core::SentenceId>),
}

impl JrcGui {
    pub fn show_reader(&mut self, ctx: &egui::Context) {
        if self.reader.is_none() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.weak("Open a document from the library to start reading.");
            });
            return;
        }

        let mut action: Option<WordAction> = None;
        let mut clicked: Option<(usize, usize)> = None; // (sentence, token)
        let mut explain_requested = false;

        let font_size = self.settings.reader_font_size.clamp(14.0, 40.0);
        let spacing_mult = self.settings.reader_line_spacing.clamp(0.6, 2.0);
        let row_gap = ROW_GAP * spacing_mult;
        let para_gap = PARA_GAP * spacing_mult;

        // ---- reading clock & away handling ----
        // While paused (or just resuming), all reader input is swallowed.
        let mut input_blocked = false;
        let mut manual_away = false;
        {
            use crate::session::{current_page_chars, Away, VisitEnd, GRACE_SECS};
            let now = std::time::Instant::now();
            let interacted = ctx.input(|i| {
                i.pointer.any_pressed()
                    || i.raw_scroll_delta.y != 0.0
                    || i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
            });
            match self.reader.as_ref().and_then(|r| r.session.away) {
                Some(Away::Manual) | Some(Away::Auto) => {
                    input_blocked = true;
                    if interacted {
                        // Resume; the resuming click/key does nothing else.
                        self.enter_page();
                    }
                }
                Some(Away::Grace { shown }) => {
                    input_blocked = true;
                    if interacted {
                        // A hard page, not an absence: keep the clock
                        // running with full credit.
                        if let Some(reader) = self.reader.as_mut() {
                            reader.session.away = None;
                            reader.session.last_interaction = now;
                        }
                    } else if shown.elapsed().as_secs_f64() >= GRACE_SECS {
                        self.end_page_visit(VisitEnd::AutoAway);
                        if let Some(reader) = self.reader.as_mut() {
                            reader.session.away = Some(Away::Auto);
                        }
                    }
                }
                None => {
                    if interacted {
                        if let Some(reader) = self.reader.as_mut() {
                            reader.session.last_interaction = now;
                        }
                    } else if let Some(reader) = self.reader.as_mut() {
                        let chars = current_page_chars(reader);
                        if chars > 0 {
                            if let Some(entered) = reader.session.page_entered {
                                let idle_start = reader.session.last_interaction.max(entered);
                                if idle_start.elapsed().as_secs_f64()
                                    >= reader.session.away_threshold(chars)
                                {
                                    reader.session.away = Some(Away::Grace { shown: now });
                                }
                            }
                        }
                    }
                }
            }
            // The away clock must tick even without input events.
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // Keyboard shortcuts (ignored while a text field has focus).
        let shortcuts = self.settings.shortcuts.clone();
        if !input_blocked {
            if shortcut_pressed(ctx, &shortcuts.reader_away) {
                manual_away = true;
            }
            if shortcut_pressed(ctx, &shortcuts.reader_next) {
                self.navigate_selection(1);
            } else if shortcut_pressed(ctx, &shortcuts.reader_prev) {
                self.navigate_selection(-1);
            }
            if let Some(reader) = self.reader.as_ref() {
                if let Some(panel) = &reader.panel {
                    let sentence_id = reader
                        .selected
                        .and_then(|(s, _)| reader.sentences.get(s))
                        .map(|v| v.sentence.id);
                    if shortcut_pressed(ctx, &shortcuts.reader_learn)
                        && panel.word.status != KnowledgeStatus::Learning
                    {
                        if let Some(sid) = sentence_id {
                            action = Some(WordAction::Learn(panel.word.id, sid));
                        }
                    }
                    if shortcut_pressed(ctx, &shortcuts.reader_known)
                        && panel.word.status != KnowledgeStatus::Known
                    {
                        action = Some(WordAction::Known(panel.word.id));
                    }
                    if shortcut_pressed(ctx, &shortcuts.reader_ignore)
                        && panel.word.status != KnowledgeStatus::Ignored
                    {
                        action = Some(WordAction::Ignore(panel.word.id));
                    }
                    if shortcut_pressed(ctx, &shortcuts.reader_explain)
                        && self.explainer.is_available()
                        && !reader.explaining
                    {
                        explain_requested = true;
                    }
                }
            }
        }

        egui::SidePanel::right("dict-panel")
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                action = self.dictionary_panel(ui, &mut explain_requested);
            });

        // Progress strip + page controls at the bottom. The control
        // cluster is centered on its own; the hint rides along to the
        // right without affecting the centering.
        let mut flip: isize = 0;
        {
            let (page, pages, progress) = self
                .reader
                .as_ref()
                .map(|r| {
                    let page = r.current_page.min(r.page_count() - 1);
                    // Fraction of sentences before the *end* of this page.
                    let end_para = r
                        .page_starts
                        .get(page + 1)
                        .copied()
                        .unwrap_or(r.para_ranges.len());
                    let end_sentence = if end_para >= r.para_ranges.len() {
                        r.sentences.len()
                    } else {
                        r.para_ranges[end_para].0
                    };
                    let frac = end_sentence as f32 / r.sentences.len().max(1) as f32;
                    (page, r.page_count(), frac)
                })
                .unwrap_or((0, 1, 0.0));

            egui::TopBottomPanel::bottom("reader-pages").show(ctx, |ui| {
                ui.add_space(4.0);
                // Thin book-progress indicator.
                let (bar, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 4.0),
                    egui::Sense::hover(),
                );
                let painter = ui.painter();
                painter.rect_filled(bar, 2.0, ui.visuals().faint_bg_color);
                let mut filled = bar;
                filled.set_width(bar.width() * progress.clamp(0.0, 1.0));
                painter.rect_filled(filled, 2.0, ui.visuals().selection.bg_fill);

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let label = format!("page {} / {}", page + 1, pages);
                    let label_width = ui
                        .fonts(|f| {
                            f.layout_no_wrap(
                                label.clone(),
                                egui::TextStyle::Body.resolve(ui.style()),
                                egui::Color32::WHITE,
                            )
                        })
                        .size()
                        .x;
                    let gap = ui.spacing().item_spacing.x;
                    let cluster = 26.0 + gap + label_width + gap + 26.0 + gap + 26.0;
                    ui.add_space((ui.available_width() - cluster).max(0.0) / 2.0);
                    if ui.add_enabled(page > 0, egui::Button::new("◀")).clicked() {
                        flip = -1;
                    }
                    ui.label(label);
                    if ui
                        .add_enabled(page + 1 < pages, egui::Button::new("▶"))
                        .clicked()
                    {
                        flip = 1;
                    }
                    if ui
                        .button("⏸")
                        .on_hover_text(format!(
                            "Pause reading ({})",
                            self.settings.shortcuts.reader_away
                        ))
                        .clicked()
                    {
                        flip = 0;
                        manual_away = true;
                    }
                    ui.weak("scroll or PgUp/PgDn");
                });
                ui.add_space(2.0);
            });
        }

        // Scroll wheel and PageUp/PageDown flip pages, e-reader style.
        if !input_blocked {
            let scroll_y = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll_y < -8.0 || ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
                flip = 1;
            } else if scroll_y > 8.0 || ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
                flip = -1;
            }
        }

        let show_unknown = self.settings.show_unknown_highlights;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(&self.reader.as_ref().unwrap().doc.title);
            ui.add_space(8.0);

            // (Re)compute pagination for the current layout size, by
            // simulating the renderer's whole-token greedy wrap with
            // measured per-token widths (a plain-text estimate breaks
            // lines later than the renderer and cut text off the page).
            {
                let font = egui::FontId::proportional(font_size);
                let row_height = ui.fonts(|f| f.row_height(&font));

                // One-time per-token width measurement.
                if self.reader.as_ref().unwrap().token_widths.is_empty() {
                    let widths: Vec<Vec<f32>> = {
                        let reader = self.reader.as_ref().unwrap();
                        reader
                            .sentences
                            .iter()
                            .map(|view| {
                                view.tokens
                                    .iter()
                                    .map(|row| {
                                        ui.fonts(|f| {
                                            f.layout_no_wrap(
                                                row.token.surface.clone(),
                                                font.clone(),
                                                egui::Color32::WHITE,
                                            )
                                        })
                                        .size()
                                        .x
                                    })
                                    .collect()
                            })
                            .collect()
                    };
                    self.reader.as_mut().unwrap().token_widths = widths;
                }

                let avail = ui.available_size();
                let reader = self.reader.as_mut().unwrap();
                let stale = reader.page_starts.is_empty()
                    || (reader.page_layout.0 - avail.x).abs() > 1.0
                    || (reader.page_layout.1 - avail.y).abs() > 1.0;
                if stale {
                    let current_para = reader
                        .page_starts
                        .get(reader.current_page)
                        .copied()
                        .unwrap_or(0);
                    let wrap = (avail.x - 4.0).max(120.0);
                    let budget = (avail.y - 8.0).max(140.0);
                    let mut starts = vec![0usize];
                    let mut acc = 0.0f32;
                    for (pi, (s0, s1)) in reader.para_ranges.iter().enumerate() {
                        // Greedy wrap, exactly like horizontal_wrapped
                        // places non-wrapping labels.
                        let mut rows = 1u32;
                        let mut line = 0.0f32;
                        for si in *s0..*s1 {
                            for w in &reader.token_widths[si] {
                                if line > 0.0 && line + w > wrap {
                                    rows += 1;
                                    line = *w;
                                } else {
                                    line += w;
                                }
                            }
                        }
                        let rows = rows as f32;
                        let h = rows * row_height + (rows - 1.0) * row_gap + para_gap;
                        if acc + h > budget && pi > *starts.last().unwrap() {
                            starts.push(pi);
                            acc = 0.0;
                        }
                        acc += h;
                    }
                    reader.page_starts = starts;
                    reader.page_layout = (avail.x, avail.y);
                    reader.current_page = reader.page_of_paragraph(current_para);
                }

                // Jump to the saved reading position once pages exist.
                if let Some(sentence) = reader.pending_restore.take() {
                    if let Some(&para) = reader.para_of_sentence.get(sentence) {
                        reader.current_page = reader.page_of_paragraph(para);
                    }
                }
            }

            let reader = self.reader.as_ref().unwrap();
            let page = reader.current_page.min(reader.page_count() - 1);
            let para_begin = reader.page_starts.get(page).copied().unwrap_or(0);
            let para_end = reader
                .page_starts
                .get(page + 1)
                .copied()
                .unwrap_or(reader.para_ranges.len());

            for (s0, s1) in &reader.para_ranges[para_begin..para_end] {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.spacing_mut().item_spacing.y = row_gap;
                    let selection_fill = ui.visuals().selection.bg_fill;
                    let unknown_fill = unknown_fill(ui.visuals());
                    for si in *s0..*s1 {
                        let view = &reader.sentences[si];
                        for (ti, row) in view.tokens.iter().enumerate() {
                            let group = reader.group_of(si, ti);
                            let selected = match (reader.selected, group) {
                                (Some((ss, sg)), Some(g)) => ss == si && sg == g,
                                _ => false,
                            };
                            let japanese = jrc_nlp::kana::is_japanese(&row.token.surface);
                            let fill = if selected {
                                Some(selection_fill)
                            } else if show_unknown
                                && japanese
                                && row.status == KnowledgeStatus::Unknown
                                && row.token.pos.is_lexical()
                            {
                                Some(unknown_fill)
                            } else {
                                None
                            };
                            let text =
                                egui::RichText::new(&row.token.surface).size(font_size);
                            // Tokens never wrap internally: a label that
                            // breaks across lines reports a full-width
                            // union rect, which painted highlights as
                            // page-wide bars.
                            let label = egui::Label::new(text)
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .sense(if japanese {
                                    egui::Sense::click()
                                } else {
                                    egui::Sense::hover()
                                });
                            // Reserve a paint slot *under* the text, fill
                            // it once the rect is known — tight around the
                            // glyphs, opaque so adjacent tokens in one
                            // phrase never double up.
                            let bg_slot = ui.painter().add(egui::Shape::Noop);
                            let response = ui.add(label);
                            if let Some(fill) = fill {
                                let rect =
                                    tight_highlight_rect(response.rect, font_size);
                                ui.painter().set(
                                    bg_slot,
                                    egui::Shape::rect_filled(rect, 0.0, fill),
                                );
                            }
                            if japanese {
                                let response =
                                    response.on_hover_cursor(egui::CursorIcon::PointingHand);
                                if response.clicked() {
                                    clicked = Some((si, ti));
                                }
                            }
                        }
                    }
                });
                ui.add_space(para_gap);
            }
        });

        if flip != 0 && !input_blocked {
            let target = self.reader.as_ref().map(|reader| {
                let pages = reader.page_count() as isize;
                (reader.current_page as isize + flip).clamp(0, pages - 1) as usize
            });
            if let Some(target) = target {
                let moved = self
                    .reader
                    .as_ref()
                    .is_some_and(|r| target != r.current_page);
                if moved {
                    // Credit the page being left before moving off it.
                    self.end_page_visit(crate::session::VisitEnd::Flip);
                    if let Some(reader) = self.reader.as_mut() {
                        reader.current_page = target;
                    }
                    self.enter_page();
                    self.persist_reading_position();
                }
            }
        }

        if manual_away && !input_blocked && self.reader.is_some() {
            self.end_page_visit(crate::session::VisitEnd::Pause);
            if let Some(reader) = self.reader.as_mut() {
                reader.session.away = Some(crate::session::Away::Manual);
            }
        }

        // Pause overlay: dims the page and swallows pointer input.
        if self.reader.as_ref().is_some_and(|r| r.session.away.is_some()) {
            let screen = ctx.screen_rect();
            egui::Area::new(egui::Id::new("away-overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(screen.min)
                .show(ctx, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(screen.size(), egui::Sense::click());
                    let painter = ui.painter();
                    painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(170));
                    painter.text(
                        rect.center() - egui::vec2(0.0, 16.0),
                        egui::Align2::CENTER_CENTER,
                        "⏸  Reading paused",
                        egui::FontId::proportional(30.0),
                        egui::Color32::WHITE,
                    );
                    painter.text(
                        rect.center() + egui::vec2(0.0, 26.0),
                        egui::Align2::CENTER_CENTER,
                        "click anywhere or press any key to resume",
                        egui::FontId::proportional(15.0),
                        egui::Color32::from_gray(200),
                    );
                });
        }

        if let Some((s_idx, t_idx)) = clicked {
            if !input_blocked {
                self.select_token(s_idx, t_idx);
            }
        }
        if explain_requested {
            self.request_explanation(ctx);
        }
        if let Some(action) = action {
            self.apply_word_action(action);
        }
    }

    /// Move the selection to the next/previous selectable phrase group.
    fn navigate_selection(&mut self, delta: isize) {
        let Some(reader) = self.reader.as_ref() else { return };
        // Ordered list of (sentence, group, first-token) for every group
        // that contains Japanese.
        let mut selectable: Vec<(usize, usize, usize)> = Vec::new();
        for (s, groups) in reader.groups.iter().enumerate() {
            for (g, (start, end)) in groups.iter().enumerate() {
                let has_japanese = reader.sentences[s].tokens[*start..*end]
                    .iter()
                    .any(|r| jrc_nlp::kana::is_japanese(&r.token.surface));
                if has_japanese {
                    selectable.push((s, g, *start));
                }
            }
        }
        if selectable.is_empty() {
            return;
        }
        let current = reader
            .selected
            .and_then(|(ss, sg)| selectable.iter().position(|(s, g, _)| *s == ss && *g == sg));
        let next = match current {
            Some(pos) => (pos as isize + delta).rem_euclid(selectable.len() as isize) as usize,
            None => {
                if delta >= 0 {
                    0
                } else {
                    selectable.len() - 1
                }
            }
        };
        let (s, _, t) = selectable[next];
        self.select_token(s, t);
        // Follow the selection across page boundaries.
        let target = self.reader.as_ref().and_then(|reader| {
            reader
                .para_of_sentence
                .get(s)
                .map(|&para| reader.page_of_paragraph(para))
                .filter(|&page| page != reader.current_page)
        });
        if let Some(page) = target {
            self.end_page_visit(crate::session::VisitEnd::Flip);
            if let Some(reader) = self.reader.as_mut() {
                reader.current_page = page;
            }
            self.enter_page();
            self.persist_reading_position();
        }
    }

    /// Select the phrase group containing a clicked token and load the
    /// panel for that token's word.
    fn select_token(&mut self, s_idx: usize, t_idx: usize) {
        let Some(reader) = self.reader.as_ref() else { return };
        let Some(g_idx) = reader.group_of(s_idx, t_idx) else { return };
        let (start, end) = reader.groups[s_idx][g_idx];
        let view = &reader.sentences[s_idx];

        let group_tokens: Vec<jrc_core::Token> = view.tokens[start..end]
            .iter()
            .map(|r| r.token.clone())
            .collect();
        let phrase: String = group_tokens.iter().map(|t| t.surface.as_str()).collect();
        let inflection = jrc_nlp::analyze_inflection(&group_tokens);
        let word_id = view.tokens[t_idx].word_id;
        // Nominal multi-token groups (低＋声, 日本語＋版) may exist in the
        // dictionary as one word; verb chains never do.
        let try_compound = group_tokens.len() > 1
            && matches!(
                group_tokens[0].pos,
                jrc_core::PartOfSpeech::Noun
                    | jrc_core::PartOfSpeech::ProperNoun
                    | jrc_core::PartOfSpeech::AdjectivalNoun
                    | jrc_core::PartOfSpeech::Prefix
            );

        let panel = self.load_word_panel(word_id, phrase, inflection, try_compound);
        if let Some(reader) = self.reader.as_mut() {
            reader.selected = Some((s_idx, g_idx));
            reader.panel = panel;
            reader.explanation = None;
        }
    }

    fn apply_word_action(&mut self, action: WordAction) {
        let result = match action {
            WordAction::Learn(word, sentence) => {
                self.with_app(|app| app.start_learning(word, sentence))
            }
            WordAction::Known(word) => self.with_app(|app| app.mark_known(word)),
            WordAction::Ignore(word) => self.with_app(|app| app.ignore_word(word)),
            WordAction::Reset(word) => self.with_app(|app| app.reset_word(word)),
            WordAction::Forgot(word, sentence) => {
                self.with_app(|app| app.mark_forgotten(word, sentence))
            }
        };
        if result.is_some() {
            self.refresh_reader_tokens();
            self.refresh_caches();
            // Refresh the panel so the status line is current.
            let current = self.reader.as_ref().and_then(|r| {
                r.panel.as_ref().map(|p| {
                    (
                        p.word.id,
                        p.phrase.clone(),
                        p.inflection.clone(),
                        p.compound.is_some(),
                    )
                })
            });
            if let Some((word_id, phrase, inflection, had_compound)) = current {
                let panel = self.load_word_panel(word_id, phrase, inflection, had_compound);
                if let Some(reader) = self.reader.as_mut() {
                    reader.panel = panel;
                }
            }
        }
    }

    fn dictionary_panel(
        &mut self,
        ui: &mut egui::Ui,
        explain_requested: &mut bool,
    ) -> Option<WordAction> {
        let mut action = None;
        let reader = self.reader.as_ref()?;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let Some(panel) = &reader.panel else {
                    ui.add_space(8.0);
                    ui.weak("Click any word in the text to look it up.");
                    ui.add_space(6.0);
                    ui.weak(
                        "Conjugated verbs are selected with their endings, and the \
                         panel explains the form.",
                    );
                    return;
                };

                // Headword with per-segment furigana above the kanji. When
                // the analyzer split a dictionary word (低声), the compound
                // takes the headline and the clicked token becomes a
                // component below.
                ui.add_space(4.0);
                if let Some(compound) = &panel.compound {
                    ruby_headword(ui, compound.headword(), compound.reading());
                    ui.add_space(2.0);
                    for (i, sense) in compound.senses.iter().take(3).enumerate() {
                        let glosses: Vec<&str> =
                            sense.gloss.iter().map(|g| g.text.as_str()).collect();
                        if !glosses.is_empty() {
                            ui.label(format!("{}. {}", i + 1, glosses.join("; ")));
                        }
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "component: {}",
                            panel.word.key.lemma
                        ))
                        .strong(),
                    );
                } else {
                    ruby_headword(ui, &panel.word.key.lemma, &panel.word.key.reading);
                    if panel.phrase != panel.word.key.lemma {
                        ui.label(format!("in text: {}", panel.phrase));
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(panel.word.key.pos.as_str());
                    ui.label("·");
                    ui.label(format!("status: {}", panel.word.status.as_str()));
                });
                if let Some(rank) = panel.rank {
                    ui.label(format!("corpus frequency rank: #{rank}"));
                }

                // Conjugation/form information.
                if !panel.inflection.is_plain() {
                    ui.add_space(8.0);
                    ui.group(|ui| {
                        ui.label(egui::RichText::new("Form").strong());
                        if let Some(summary) = &panel.inflection.summary {
                            ui.label(summary);
                        }
                        for part in &panel.inflection.parts {
                            ui.weak(part);
                        }
                    });
                }
                ui.add_space(6.0);

                match &panel.entry {
                    Some(entry) => {
                        let profile = UsageProfile::from_misc_codes(entry.misc_codes());
                        if !profile.is_neutral() || !profile.notes.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for reg in &profile.registers {
                                    ui.label(
                                        egui::RichText::new(reg.label()).small().background_color(
                                            egui::Color32::from_rgba_unmultiplied(
                                                200, 120, 60, 70,
                                            ),
                                        ),
                                    );
                                }
                                for note in &profile.notes {
                                    ui.label(
                                        egui::RichText::new(note).small().background_color(
                                            egui::Color32::from_rgba_unmultiplied(
                                                100, 140, 100, 60,
                                            ),
                                        ),
                                    );
                                }
                            });
                            ui.add_space(4.0);
                        }

                        for (i, sense) in entry.senses.iter().enumerate() {
                            let glosses: Vec<&str> =
                                sense.gloss.iter().map(|g| g.text.as_str()).collect();
                            if glosses.is_empty() {
                                continue;
                            }
                            ui.label(format!("{}. {}", i + 1, glosses.join("; ")));
                            if !sense.misc.is_empty() {
                                ui.weak(format!("   [{}]", sense.misc.join(", ")));
                            }
                            if !sense.info.is_empty() {
                                ui.weak(format!("   {}", sense.info.join("; ")));
                            }
                        }

                        let related = entry.related_words();
                        if !related.is_empty() {
                            ui.add_space(4.0);
                            ui.label(format!("see also: {}", related.join("、")));
                        }
                        let antonyms = entry.antonyms();
                        if !antonyms.is_empty() {
                            ui.label(format!("antonyms: {}", antonyms.join("、")));
                        }
                    }
                    None => {
                        ui.weak("No dictionary entry found for this word.");
                    }
                }

                ui.add_space(10.0);
                ui.separator();

                let sentence_id = reader
                    .selected
                    .and_then(|(s, _)| reader.sentences.get(s))
                    .map(|v| v.sentence.id);
                ui.horizontal_wrapped(|ui| {
                    if panel.word.status != KnowledgeStatus::Learning {
                        if let Some(sid) = sentence_id {
                            if ui.button("➕ Learn (SRS)").clicked() {
                                action = Some(WordAction::Learn(panel.word.id, sid));
                            }
                        }
                    }
                    if panel.word.status == KnowledgeStatus::Known {
                        if ui
                            .button("↺ Forgot this")
                            .on_hover_text("Put it back into review rotation")
                            .clicked()
                        {
                            action = Some(WordAction::Forgot(panel.word.id, sentence_id));
                        }
                    } else if ui.button("✔ Known").clicked() {
                        action = Some(WordAction::Known(panel.word.id));
                    }
                    if panel.word.status != KnowledgeStatus::Ignored
                        && ui.button("🚫 Ignore").clicked()
                    {
                        action = Some(WordAction::Ignore(panel.word.id));
                    }
                    if panel.word.status != KnowledgeStatus::Unknown
                        && ui.button("Reset").clicked()
                    {
                        action = Some(WordAction::Reset(panel.word.id));
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Sentence explanation").strong());
                if self.explainer.is_available() {
                    if reader.explaining {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("asking the tutor…");
                        });
                    } else if ui.button("Explain this sentence").clicked() {
                        *explain_requested = true;
                    }
                    if let Some(explanation) = &reader.explanation {
                        ui.add_space(4.0);
                        ui.label(explanation);
                    }
                } else {
                    ui.weak("Add an API key in Settings to enable LLM explanations.");
                }
            });

        action
    }
}

/// Draw a headword with furigana positioned over the kanji run it reads,
/// not clumped over the whole word: 食(た)べる, 引(ひ)き出(だ)し.
fn ruby_headword(ui: &mut egui::Ui, lemma: &str, reading: &str) {
    let segments = jrc_nlp::ruby_segments(lemma, reading);
    let big_font = egui::FontId::proportional(30.0);
    let small_font = egui::FontId::proportional(12.0);
    let text_color = ui.visuals().strong_text_color();
    let weak_color = ui.visuals().weak_text_color();

    let big_galleys: Vec<_> = segments
        .iter()
        .map(|s| ui.fonts(|f| f.layout_no_wrap(s.text.clone(), big_font.clone(), text_color)))
        .collect();
    let furi_galleys: Vec<_> = segments
        .iter()
        .map(|s| {
            s.furigana
                .as_ref()
                .map(|t| ui.fonts(|f| f.layout_no_wrap(t.clone(), small_font.clone(), weak_color)))
        })
        .collect();

    let furi_height = 14.0;
    let big_height = big_galleys
        .iter()
        .map(|g| g.size().y)
        .fold(0.0f32, f32::max);
    // Each segment is as wide as the wider of its text and its furigana.
    let seg_widths: Vec<f32> = big_galleys
        .iter()
        .zip(&furi_galleys)
        .map(|(b, f)| b.size().x.max(f.as_ref().map(|f| f.size().x).unwrap_or(0.0)))
        .collect();
    let total_width: f32 = seg_widths.iter().sum();

    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(total_width, furi_height + big_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let mut x = rect.left();
    for ((big, furi), width) in big_galleys.into_iter().zip(furi_galleys).zip(seg_widths) {
        if let Some(furi) = furi {
            let fx = x + (width - furi.size().x) / 2.0;
            painter.galley(egui::pos2(fx, rect.top()), furi, weak_color);
        }
        let bx = x + (width - big.size().x) / 2.0;
        painter.galley(egui::pos2(bx, rect.top() + furi_height), big, text_color);
        x += width;
    }
}
