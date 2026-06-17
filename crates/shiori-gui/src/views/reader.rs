//! Reader view: flowing paragraphs of clickable text with a dictionary
//! side panel for the selected phrase.
//!
//! Phrases (conjugated verbs with their endings, noun+suffix compounds)
//! are selected as a unit, highlighted in a single color, and the panel
//! explains the conjugation. No permanent per-status tinting — an optional
//! settings toggle can mark unknown words.

use eframe::egui;
use shiori_core::{KnowledgeStatus, WordId};
use shiori_dict::register::UsageProfile;

use crate::app::{ReaderLine, ShioriGui};
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
    Learn(WordId, shiori_core::SentenceId),
    Known(WordId),
    Ignore(WordId),
    Reset(WordId),
    Forgot(WordId, Option<shiori_core::SentenceId>),
}

impl ShioriGui {
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
        let mut open_explanation_modal = false;
        // While the explanation modal floats over the reading view, reader
        // input (word clicks, page flips, shortcuts) is suppressed so it can't
        // disturb the page underneath. The away clock still ticks below —
        // interacting with the modal counts as activity, just like the page.
        let modal_open = self.reader.as_ref().is_some_and(|r| r.explanation_modal);
        // The whole reader content area (everything right of the nav rail,
        // below any banner), captured before this view's own panels carve it
        // up — the explanation modal centres in this so it spans the reading
        // text and the dictionary panel alike, but never the nav rail.
        let content_rect = ctx.available_rect();

        let font_size = self.settings.reader_font_size.clamp(14.0, 40.0);
        let spacing_mult = self.settings.reader_line_spacing.clamp(0.6, 2.0);
        let furigana_mode = self.settings.furigana;
        let furigana_x = self.settings.furigana_first_x.max(1);
        let furi_size = (font_size * 0.45).round();
        // Headroom above every text row for ruby text, when enabled.
        let furi_gap = if furigana_mode == crate::settings::FuriganaMode::None {
            0.0
        } else {
            furi_size + 3.0
        };
        let row_gap = ROW_GAP * spacing_mult + furi_gap;
        let para_gap = PARA_GAP * spacing_mult + furi_gap;

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
        if !input_blocked && !modal_open {
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

        let mut open_kanji: Option<String> = None;
        // Trim the panel's right padding so its vertical scroll bar sits at
        // the window edge instead of floating ~8px inside it.
        let dict_frame = egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin {
            left: 8,
            right: 2,
            top: 2,
            bottom: 2,
        });
        let panel_rect = egui::SidePanel::right("dict-panel")
            .resizable(true)
            .default_width(340.0)
            // Cap the width so a stray wide element (e.g. a Markdown table the
            // tutor slipped in) can't balloon the panel over the whole page;
            // it clips instead, and the 🔎 modal shows it in full.
            .width_range(220.0..=700.0)
            .frame(dict_frame)
            .show(ctx, |ui| {
                action = self.dictionary_panel(
                    ui,
                    &mut explain_requested,
                    &mut open_kanji,
                    &mut open_explanation_modal,
                );
            })
            .response
            .rect;
        if let Some(kanji) = open_kanji {
            self.end_page_visit(crate::session::VisitEnd::Pause);
            self.open_dictionary(kanji);
            return;
        }
        if open_explanation_modal {
            if let Some(reader) = self.reader.as_mut() {
                reader.explanation_modal = true;
                reader.explanation_modal_just_opened = true;
            }
        }

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
                    // Fraction of sentences reached by the *end* of this page:
                    // the first sentence shown on the next page (all of them
                    // once the last page is reached).
                    let next_line = r
                        .page_line_starts
                        .get(page + 1)
                        .copied()
                        .unwrap_or(r.lines.len());
                    let end_sentence = r
                        .lines
                        .get(next_line)
                        .and_then(|l| l.cells.first())
                        .map(|&(s, _)| s)
                        .unwrap_or(r.sentences.len());
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
        // Scrolling over the dictionary panel scrolls that panel instead:
        // it still resets the away timer above, but must not flip the page.
        if !input_blocked && !modal_open {
            let pointer_over_panel = ctx
                .input(|i| i.pointer.hover_pos())
                .is_some_and(|p| panel_rect.contains(p));
            let scroll_y = if pointer_over_panel {
                0.0
            } else {
                ctx.input(|i| i.raw_scroll_delta.y)
            };
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

                // egui's global zoom (Ctrl +/-, Ctrl-scroll) changes
                // pixels_per_point, which shifts the pixel-rounded width of
                // every token. The cached widths — and the pagination
                // greedy-wrapped from them — were measured at the previous
                // zoom; kept as-is they under-count rows, so a page overfills
                // and its last lines spill under the bottom progress bar.
                // Drop the cache and re-flow on a zoom change, holding the
                // reading position.
                let ppp = ui.ctx().pixels_per_point();
                let zoom_changed = {
                    let reader = self.reader.as_ref().unwrap();
                    !reader.token_widths.is_empty() && (reader.layout_ppp - ppp).abs() > 1e-3
                };
                if zoom_changed {
                    self.invalidate_reader_layout();
                }

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
                    let reader = self.reader.as_mut().unwrap();
                    reader.token_widths = widths;
                    reader.layout_ppp = ppp;
                }

                let avail = ui.available_size();
                let reader = self.reader.as_mut().unwrap();
                let stale = reader.page_line_starts.is_empty()
                    || (reader.page_layout.0 - avail.x).abs() > 1.0
                    || (reader.page_layout.1 - avail.y).abs() > 1.0;
                if stale {
                    // Sentence at the top of the current page, to land back on
                    // after re-flowing.
                    let anchor = reader
                        .page_line_starts
                        .get(reader.current_page)
                        .and_then(|&l| reader.lines.get(l))
                        .and_then(|line| line.cells.first())
                        .map(|&(s, _)| s)
                        .unwrap_or(0);

                    // Wrap whole tokens into display lines, greedily, exactly
                    // as the renderer lays them out — but break across
                    // paragraphs, recording where each sentence begins.
                    let wrap = (avail.x - 4.0).max(120.0);
                    let mut lines: Vec<ReaderLine> = Vec::new();
                    let mut line_of_sentence = vec![0usize; reader.sentences.len()];
                    for &(s0, s1) in &reader.para_ranges {
                        let mut cells: Vec<(usize, usize)> = Vec::new();
                        let mut line_w = 0.0f32;
                        let mut para_start = true;
                        for si in s0..s1 {
                            for (ti, w) in reader.token_widths[si].iter().enumerate() {
                                if !cells.is_empty() && line_w + w > wrap {
                                    lines.push(ReaderLine {
                                        para_start,
                                        cells: std::mem::take(&mut cells),
                                    });
                                    para_start = false;
                                    line_w = 0.0;
                                }
                                if ti == 0 {
                                    line_of_sentence[si] = lines.len();
                                }
                                cells.push((si, ti));
                                line_w += w;
                            }
                            if reader.token_widths[si].is_empty() {
                                line_of_sentence[si] = lines.len();
                            }
                        }
                        if !cells.is_empty() {
                            lines.push(ReaderLine { para_start, cells });
                        }
                    }

                    // Pack lines into pages up to the height budget. The page's
                    // first line carries the ruby headroom (counted once in the
                    // budget); later lines take a row or paragraph gap. A line
                    // can never exceed the budget, so no page overflows.
                    let budget = (avail.y - 8.0 - furi_gap).max(140.0);
                    let mut starts = vec![0usize];
                    let mut acc = 0.0f32;
                    for (li, line) in lines.iter().enumerate() {
                        let page_top = li == *starts.last().unwrap();
                        let gap = if page_top {
                            0.0
                        } else if line.para_start {
                            para_gap
                        } else {
                            row_gap
                        };
                        let h = gap + row_height;
                        if !page_top && acc + h > budget {
                            starts.push(li);
                            acc = row_height;
                        } else {
                            acc += h;
                        }
                    }

                    reader.lines = lines;
                    reader.line_of_sentence = line_of_sentence;
                    reader.page_line_starts = starts;
                    reader.page_layout = (avail.x, avail.y);
                    reader.current_page = reader.page_of_sentence(anchor);
                }

                // Jump to the saved reading position once pages exist. A
                // finished book stores one-past-the-end; clamp onto the
                // last page.
                if let Some(sentence) = reader.pending_restore.take() {
                    let idx = sentence.min(reader.sentences.len().saturating_sub(1));
                    reader.current_page = reader.page_of_sentence(idx);
                }
            }

            let reader = self.reader.as_ref().unwrap();
            let page = reader.current_page.min(reader.page_count() - 1);
            let line_begin = reader.page_line_starts.get(page).copied().unwrap_or(0);
            let line_end = reader
                .page_line_starts
                .get(page + 1)
                .copied()
                .unwrap_or(reader.lines.len());

            // Drive every vertical gap explicitly so the rendered page height
            // matches what pagination budgeted; egui would otherwise insert
            // its own item spacing between the rows.
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(furi_gap);
            for li in line_begin..line_end {
                let line = &reader.lines[li];
                if li != line_begin {
                    ui.add_space(if line.para_start { para_gap } else { row_gap });
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let selection_fill = ui.visuals().selection.bg_fill;
                    let unknown_fill = unknown_fill(ui.visuals());
                    for &(si, ti) in &line.cells {
                        let view = &reader.sentences[si];
                        let row = &view.tokens[ti];
                        let group = reader.group_of(si, ti);
                        let selected = match (reader.selected, group) {
                            (Some((ss, sg)), Some(g)) => ss == si && sg == g,
                            _ => false,
                        };
                        let japanese = shiori_nlp::kana::is_japanese(&row.token.surface);
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
                        let text = egui::RichText::new(&row.token.surface).size(font_size);
                        // Tokens never wrap internally: a label that breaks
                        // across lines reports a full-width union rect, which
                        // painted highlights as page-wide bars.
                        let label = egui::Label::new(text)
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .sense(if japanese {
                                egui::Sense::click()
                            } else {
                                egui::Sense::hover()
                            });
                        // Reserve a paint slot *under* the text, fill it once
                        // the rect is known — tight around the glyphs, opaque
                        // so adjacent tokens in one phrase never double up.
                        let bg_slot = ui.painter().add(egui::Shape::Noop);
                        let response = ui.add(label);
                        if let Some(fill) = fill {
                            let rect = tight_highlight_rect(response.rect, font_size);
                            ui.painter()
                                .set(bg_slot, egui::Shape::rect_filled(rect, 0.0, fill));
                        }
                        let show_ruby = match furigana_mode {
                            crate::settings::FuriganaMode::None => false,
                            crate::settings::FuriganaMode::All => true,
                            crate::settings::FuriganaMode::Unknown => {
                                row.status == KnowledgeStatus::Unknown
                            }
                            crate::settings::FuriganaMode::UnknownFirstX => {
                                row.status == KnowledgeStatus::Unknown
                                    && reader
                                        .word_occurrence
                                        .get(si)
                                        .and_then(|s| s.get(ti))
                                        .is_some_and(|&n| n <= furigana_x)
                            }
                        };
                        if show_ruby {
                            if let Some(ruby) = token_furigana(
                                &row.token.surface,
                                &row.token.lemma,
                                &row.token.reading,
                            ) {
                                ui.painter().text(
                                    egui::pos2(response.rect.center().x, response.rect.top() - 1.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    ruby,
                                    egui::FontId::proportional(furi_size),
                                    ui.visuals().weak_text_color(),
                                );
                            }
                        }
                        if japanese {
                            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                            if response.clicked() {
                                clicked = Some((si, ti));
                            }
                        }
                    }
                });
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
        if self
            .reader
            .as_ref()
            .is_some_and(|r| r.session.away.is_some())
        {
            let screen = ctx.screen_rect();
            egui::Area::new(egui::Id::new("away-overlay"))
                .order(egui::Order::Foreground)
                .fixed_pos(screen.min)
                .show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
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
            if !input_blocked && !modal_open {
                self.select_token(s_idx, t_idx);
            }
        }
        if explain_requested {
            self.request_explanation(ctx);
        }
        if let Some(action) = action {
            self.apply_word_action(action);
        }
        self.show_explanation_modal(ctx, content_rect);
    }

    /// Move the selection to the next/previous selectable phrase group.
    fn navigate_selection(&mut self, delta: isize) {
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        // Ordered list of (sentence, group, first-token) for every group
        // that contains Japanese.
        let mut selectable: Vec<(usize, usize, usize)> = Vec::new();
        for (s, groups) in reader.groups.iter().enumerate() {
            for (g, (start, end)) in groups.iter().enumerate() {
                let has_japanese = reader.sentences[s].tokens[*start..*end]
                    .iter()
                    .any(|r| shiori_nlp::kana::is_japanese(&r.token.surface));
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
            let page = reader.page_of_sentence(s);
            (page != reader.current_page).then_some(page)
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
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        let Some(g_idx) = reader.group_of(s_idx, t_idx) else {
            return;
        };
        let (start, end) = reader.groups[s_idx][g_idx];
        let view = &reader.sentences[s_idx];

        let group_tokens: Vec<shiori_core::Token> = view.tokens[start..end]
            .iter()
            .map(|r| r.token.clone())
            .collect();
        let phrase: String = group_tokens.iter().map(|t| t.surface.as_str()).collect();
        let inflection = shiori_nlp::analyze_inflection(&group_tokens);
        let word_id = view.tokens[t_idx].word_id;
        // Nominal multi-token groups (低＋声, 日本語＋版) may exist in the
        // dictionary as one word; verb chains never do.
        let try_compound = group_tokens.len() > 1
            && matches!(
                group_tokens[0].pos,
                shiori_core::PartOfSpeech::Noun
                    | shiori_core::PartOfSpeech::ProperNoun
                    | shiori_core::PartOfSpeech::AdjectivalNoun
                    | shiori_core::PartOfSpeech::Prefix
            );

        let panel = self.load_word_panel(word_id, phrase, inflection, try_compound);
        if let Some(reader) = self.reader.as_mut() {
            reader.selected = Some((s_idx, g_idx));
            reader.panel = panel;
            reader.explanation = None;
            reader.explanation_modal = false;
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
        open_kanji: &mut Option<String>,
        open_explanation_modal: &mut bool,
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
                        egui::RichText::new(format!("component: {}", panel.word.key.lemma))
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

                // Kanji chips: expand any kanji of the headword into its
                // card in the dictionary view.
                let kanji_chars: Vec<char> = {
                    let mut seen = std::collections::HashSet::new();
                    panel
                        .word
                        .key
                        .lemma
                        .chars()
                        .filter(|c| {
                            shiori_nlp::kana::contains_kanji(&c.to_string()) && seen.insert(*c)
                        })
                        .collect()
                };
                if !kanji_chars.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.weak("kanji:");
                        for c in kanji_chars {
                            if ui
                                .small_button(c.to_string())
                                .on_hover_text("Readings and stroke order")
                                .clicked()
                            {
                                *open_kanji = Some(c.to_string());
                            }
                        }
                    });
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
                                            egui::Color32::from_rgba_unmultiplied(200, 120, 60, 70),
                                        ),
                                    );
                                }
                                for note in &profile.notes {
                                    ui.label(egui::RichText::new(note).small().background_color(
                                        egui::Color32::from_rgba_unmultiplied(100, 140, 100, 60),
                                    ));
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
                        if self.dict_ready() {
                            ui.weak("No dictionary entry found for this word.");
                        } else {
                            ui.weak(
                                "No dictionary installed — definitions are \
                                 unavailable. Retry the download from the \
                                 banner above.",
                            );
                        }
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
                    if panel.word.status != KnowledgeStatus::Unknown && ui.button("Reset").clicked()
                    {
                        action = Some(WordAction::Reset(panel.word.id));
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.label(egui::RichText::new("Sentence explanation").strong());
                if self.explainer.is_available() {
                    ui.horizontal(|ui| {
                        if reader.explaining {
                            ui.spinner();
                            ui.label("asking the tutor…");
                        } else if ui.button("Explain this sentence").clicked() {
                            *explain_requested = true;
                        }
                        // Expand-to-modal, pinned to the right at the same
                        // level — the magnifier matches the dictionary card.
                        if reader.explanation.is_some() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("🔎")
                                        .on_hover_text("Read the explanation in a larger window")
                                        .clicked()
                                    {
                                        *open_explanation_modal = true;
                                    }
                                },
                            );
                        }
                    });
                    if let Some(explanation) = &reader.explanation {
                        ui.add_space(4.0);
                        // The tutor answers in Markdown; render it formatted
                        // rather than dumping the raw source. Wide content
                        // (tables) scrolls sideways instead of stretching the
                        // panel — see render_markdown — and the 🔎 opens the
                        // same Markdown in a roomy modal.
                        render_markdown(ui, "reader-explanation-inline", explanation, false);
                    }
                } else {
                    ui.weak("Add an API key in Settings to enable LLM explanations.");
                }
            });

        action
    }

    /// Render the sentence-explanation modal, if open, via the shared
    /// [`super::modal::centered_modal`] shell. It is centred in the reading
    /// area (not the whole screen), so the nav rail and dictionary panel stay
    /// put; clicks inside the modal stay in the modal, and a click on the
    /// reading area behind it (or Escape) dismisses it. The away clock is
    /// driven from raw input above, so reading the explanation still counts
    /// as activity.
    fn show_explanation_modal(&mut self, ctx: &egui::Context, area: egui::Rect) {
        let (explanation, just_opened) = {
            let Some(reader) = self.reader.as_mut() else {
                return;
            };
            if !reader.explanation_modal {
                return;
            }
            let Some(explanation) = reader.explanation.clone() else {
                return;
            };
            (
                explanation,
                std::mem::take(&mut reader.explanation_modal_just_opened),
            )
        };

        let close = super::modal::centered_modal(
            ctx,
            area,
            "reader-explanation-modal",
            just_opened,
            |ui| {
                ui.label(egui::RichText::new("Sentence explanation").strong());
            },
            |ui| {
                render_markdown(ui, "reader-explanation-modal-body", &explanation, true);
            },
        );

        if close {
            if let Some(reader) = self.reader.as_mut() {
                reader.explanation_modal = false;
            }
        }
    }
}

/// Render the tutor's Markdown with a real heading hierarchy. When
/// `scrollable` it gets its own vertical scroll area (the modal, which owns
/// its scrolling); otherwise it renders inline and the caller's scroll area
/// handles height (the side panel, which wraps everything to its width).
///
/// Tables are discouraged upstream (the prompt asks for lists) because
/// egui_commonmark lays each table cell out in a non-wrapping row, so cells
/// cannot wrap; the panel's width cap keeps a stray one from ballooning it.
fn render_markdown(ui: &mut egui::Ui, id: &str, markdown: &str, scrollable: bool) {
    let draw = |ui: &mut egui::Ui| {
        // egui_commonmark scales headings between the Body and Heading text
        // styles, and egui's default Heading sits barely above Body — bump it
        // (this scope only) so #/##/### gain a real size hierarchy.
        let body = ui
            .style()
            .text_styles
            .get(&egui::TextStyle::Body)
            .map_or(14.0, |f| f.size);
        if let Some(heading) = ui
            .style_mut()
            .text_styles
            .get_mut(&egui::TextStyle::Heading)
        {
            heading.size = body * 2.0;
        }
        // A fresh cache each frame is fine — explanations are short and
        // image-free, so there is nothing worth persisting.
        let mut cache = egui_commonmark::CommonMarkCache::default();
        egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, markdown);
    };

    if scrollable {
        // Reserve the (solid) scroll bar's gutter so text wraps clear of it
        // rather than tripping a spurious horizontal bar.
        let wrap = (ui.available_width() - ui.spacing().scroll.allocated_width()).max(0.0);
        egui::ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(wrap);
                draw(ui);
            });
    } else {
        ui.scope(draw);
    }
}

/// The ruby text to draw over a token, if it deserves any.
///
/// Tokens store their word's lemma reading, not a surface reading. For
/// unconjugated words those coincide. For a conjugated stem (走っ from
/// 走る・はしる) the lemma's ruby segments are walked for as long as the
/// surface still matches them, keeping the kanji-run furigana and
/// dropping the okurigana the surface no longer has.
fn token_furigana(surface: &str, lemma: &str, reading: &str) -> Option<String> {
    if !shiori_nlp::kana::contains_kanji(surface) || reading.is_empty() {
        return None;
    }
    let hira = shiori_nlp::kana::katakana_to_hiragana(reading);
    if surface == lemma {
        return Some(hira);
    }
    let surf: Vec<char> = surface.chars().collect();
    let mut out = String::new();
    let mut consumed = 0;
    for seg in shiori_nlp::ruby_segments(lemma, &hira) {
        let seg_chars: Vec<char> = seg.text.chars().collect();
        if surf[consumed..].starts_with(&seg_chars[..]) {
            consumed += seg_chars.len();
            match &seg.furigana {
                Some(f) => out.push_str(f),
                None => out.push_str(&seg.text),
            }
            if consumed == surf.len() {
                break;
            }
        } else {
            break;
        }
    }
    if out.is_empty() {
        Some(hira)
    } else {
        Some(out)
    }
}

/// Draw a headword with furigana positioned over the kanji run it reads,
/// not clumped over the whole word: 食(た)べる, 引(ひ)き出(だ)し.
fn ruby_headword(ui: &mut egui::Ui, lemma: &str, reading: &str) {
    let segments = shiori_nlp::ruby_segments(lemma, reading);
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
        .map(|(b, f)| {
            b.size()
                .x
                .max(f.as_ref().map(|f| f.size().x).unwrap_or(0.0))
        })
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

#[cfg(test)]
mod tests {
    use super::token_furigana;

    #[test]
    fn plain_word_gets_its_whole_reading() {
        assert_eq!(token_furigana("猫", "猫", "ネコ"), Some("ねこ".into()));
        assert_eq!(
            token_furigana("日本語", "日本語", "ニホンゴ"),
            Some("にほんご".into())
        );
    }

    #[test]
    fn conjugated_stem_keeps_kanji_reading_only() {
        // 走っ (from 走る・はしる): ruby segments are 走(はし) + る; the
        // surface diverges after the kanji, so only はし survives.
        assert_eq!(
            token_furigana("走っ", "走る", "ハシル"),
            Some("はし".into())
        );
    }

    #[test]
    fn kana_tokens_get_nothing() {
        assert_eq!(token_furigana("する", "する", "スル"), None);
        assert_eq!(token_furigana("は", "は", "ハ"), None);
    }

    #[test]
    fn missing_reading_gets_nothing() {
        assert_eq!(token_furigana("猫", "猫", ""), None);
    }
}
