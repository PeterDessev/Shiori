//! Application shell: state, background tasks, frame loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use jrc_app::{App, DataStatus, DocStats, MiningCandidate, ReviewItem};
use jrc_core::{Document, DocumentId, WordId};
use jrc_db::{DocumentSummary, TokenRow, WordRow};
use jrc_dict::DictEntry;
use jrc_llm::Explainer;

use crate::settings::{Settings, Theme};

/// Messages posted back from background threads.
pub enum Msg {
    AppOpened(Result<Box<App>, String>),
    Progress(String),
    DownloadDone(Result<DataStatus, String>),
    ImportDone(Result<DocumentId, String>),
    Explained(Result<String, String>),
    Feedback(Result<String, String>),
}

/// Startup/data lifecycle.
#[derive(PartialEq)]
pub enum Phase {
    Starting,
    NeedsData,
    Downloading,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Welcome,
    Library,
    Reader,
    Mining,
    Review,
    Stats,
    Production,
    Settings,
}

/// One sentence of the open document, with its tokens.
pub struct SentenceView {
    pub sentence: jrc_core::Sentence,
    pub tokens: Vec<TokenRow>,
}

/// Dictionary panel contents for the selected token.
pub struct WordPanel {
    pub word: WordRow,
    pub entry: Option<DictEntry>,
    pub rank: Option<u32>,
    /// Surface of the whole selected phrase (e.g. 読んでいる).
    pub phrase: String,
    /// Grammar of the phrase tail, when conjugated.
    pub inflection: jrc_nlp::Inflection,
    /// Dictionary entry for the whole phrase when the analyzer split a
    /// word JMdict knows as one (低声 = 低＋声).
    pub compound: Option<DictEntry>,
}

pub struct ReaderState {
    pub doc: Document,
    pub sentences: Vec<SentenceView>,
    /// Per sentence: phrase groups as (start, end) token ranges.
    pub groups: Vec<Vec<(usize, usize)>>,
    /// (sentence index, group index) of the selected phrase.
    pub selected: Option<(usize, usize)>,
    pub panel: Option<WordPanel>,
    pub explanation: Option<String>,
    pub explaining: bool,
    /// Sentence-index ranges forming each paragraph, in order.
    pub para_ranges: Vec<(usize, usize)>,
    /// Paragraph index of each sentence.
    pub para_of_sentence: Vec<usize>,
    /// Page boundaries as indices into `para_ranges`; page `i` covers
    /// `para_ranges[page_starts[i]..page_starts[i+1]]`. Rebuilt lazily
    /// whenever the layout size changes.
    pub page_starts: Vec<usize>,
    pub current_page: usize,
    /// (width, height) the pages were computed for.
    pub page_layout: (f32, f32),
    /// Rendered width of every token (lazy; pagination simulates the real
    /// whole-token wrap with these).
    pub token_widths: Vec<Vec<f32>>,
    /// Sentence index to jump to once pages are computed (restores the
    /// saved reading position).
    pub pending_restore: Option<usize>,
}

impl ReaderState {
    /// Index of the group containing token `t_idx` of sentence `s_idx`.
    pub fn group_of(&self, s_idx: usize, t_idx: usize) -> Option<usize> {
        self.groups
            .get(s_idx)?
            .iter()
            .position(|(s, e)| (*s..*e).contains(&t_idx))
    }

    pub fn page_count(&self) -> usize {
        self.page_starts.len().max(1)
    }

    /// Page containing the given paragraph.
    pub fn page_of_paragraph(&self, para: usize) -> usize {
        self.page_starts
            .iter()
            .rposition(|&p| p <= para)
            .unwrap_or_default()
    }
}

/// Group consecutive same-paragraph sentences into ranges.
fn paragraph_structure(sentences: &[SentenceView]) -> (Vec<(usize, usize)>, Vec<usize>) {
    let mut ranges = Vec::new();
    let mut of_sentence = Vec::with_capacity(sentences.len());
    let mut start = 0;
    for i in 0..sentences.len() {
        let boundary = i + 1 == sentences.len()
            || sentences[i + 1].sentence.paragraph != sentences[i].sentence.paragraph;
        if boundary {
            ranges.push((start, i + 1));
            for _ in start..=i {
                of_sentence.push(ranges.len() - 1);
            }
            start = i + 1;
        }
    }
    (ranges, of_sentence)
}

/// Compute phrase groups for each sentence.
fn compute_groups(sentences: &[SentenceView]) -> Vec<Vec<(usize, usize)>> {
    sentences
        .iter()
        .map(|view| {
            let tokens: Vec<jrc_core::Token> =
                view.tokens.iter().map(|r| r.token.clone()).collect();
            jrc_nlp::phrase_groups(&tokens)
        })
        .collect()
}

#[derive(Default)]
pub struct MiningState {
    pub doc_id: Option<DocumentId>,
    pub doc_title: String,
    pub candidates: Vec<MiningCandidate>,
}

#[derive(Default)]
pub struct ReviewState {
    pub queue: Vec<ReviewItem>,
    pub revealed: bool,
}

#[derive(Default)]
pub struct ProductionState {
    pub prompt_idx: usize,
    pub text: String,
    pub feedback: Option<String>,
    pub waiting: bool,
}

/// Per-document metadata being edited in the library dialog.
pub struct MetaEdit {
    pub id: DocumentId,
    pub meta: jrc_core::DocumentMeta,
}

/// Library column to sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Added,
    Title,
    Author,
    Published,
    Size,
    Progress,
    Known,
    Difficulty,
    NewWords,
}

pub struct JrcGui {
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub app: Option<Arc<Mutex<App>>>,
    pub explainer: Arc<dyn Explainer>,
    pub phase: Phase,
    pub progress: Vec<String>,
    pub error: Option<String>,
    /// Number of background import jobs in flight.
    pub import_jobs: usize,
    pub view: View,

    // Cached queries, refreshed on events rather than per-frame.
    pub library: Vec<DocumentSummary>,
    pub doc_stats: HashMap<i64, DocStats>,
    pub due_count: u64,

    pub meta_edit: Option<MetaEdit>,
    pub reader: Option<ReaderState>,
    pub mining: MiningState,
    pub review: ReviewState,
    pub production: ProductionState,
    pub data_status: Option<DataStatus>,
    pub data_dir: PathBuf,
    pub settings: Settings,
    /// Editable copy shown in the settings view (saved explicitly).
    pub settings_draft: Settings,
    /// Which settings category page is open.
    pub settings_category: crate::views::SettingsCategory,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    /// Theme applied to the egui context (to detect setting changes).
    applied_theme: Option<Theme>,
    /// Where to return when the getting-started page is closed.
    pub welcome_return: Option<View>,
}

pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("japanese-reading-companion")
}

impl JrcGui {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        let data_dir = default_data_dir();
        let settings = Settings::load(&data_dir);

        // Opening the app deserializes the embedded NLP dictionary — keep
        // it off the first frame.
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            let data_dir = data_dir.clone();
            std::thread::spawn(move || {
                let result = App::open(&data_dir)
                    .map(Box::new)
                    .map_err(|e| e.to_string());
                let _ = tx.send(Msg::AppOpened(result));
                ctx.request_repaint();
            });
        }

        Self {
            tx,
            rx,
            app: None,
            explainer: settings.build_explainer(),
            phase: Phase::Starting,
            progress: Vec::new(),
            error: None,
            import_jobs: 0,
            view: if settings.onboarded {
                View::Library
            } else {
                View::Welcome
            },
            library: Vec::new(),
            doc_stats: HashMap::new(),
            due_count: 0,
            meta_edit: None,
            reader: None,
            mining: MiningState::default(),
            review: ReviewState::default(),
            production: ProductionState::default(),
            data_status: None,
            data_dir,
            settings_draft: settings.clone(),
            settings,
            settings_category: Default::default(),
            sort_key: SortKey::default(),
            sort_asc: true,
            applied_theme: None,
            welcome_return: None,
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.theme) {
            ctx.set_visuals(match self.settings.theme {
                Theme::Dark => egui::Visuals::dark(),
                Theme::Light => egui::Visuals::light(),
            });
            self.applied_theme = Some(self.settings.theme);
        }
    }

    /// Persist the settings draft and apply it (rebuilds the LLM backend).
    pub fn apply_settings(&mut self) {
        self.settings = self.settings_draft.clone();
        if let Err(e) = self.settings.save(&self.data_dir) {
            self.error = Some(format!("could not save settings: {e}"));
        }
        self.explainer = self.settings.build_explainer();
    }

    /// Dismiss the getting-started page, returning to wherever the user
    /// was before opening it.
    pub fn finish_onboarding(&mut self) {
        self.settings.onboarded = true;
        self.settings_draft.onboarded = true;
        if let Err(e) = self.settings.save(&self.data_dir) {
            self.error = Some(format!("could not save settings: {e}"));
        }
        self.view = self.welcome_return.take().unwrap_or(View::Library);
    }

    /// Open the getting-started page, remembering where to come back to.
    pub fn open_welcome(&mut self) {
        if self.view != View::Welcome {
            self.welcome_return = Some(self.view);
        }
        self.view = View::Welcome;
    }

    /// Run a closure against the app, routing failures to the error toast.
    pub fn with_app<T>(&mut self, f: impl FnOnce(&App) -> Result<T, jrc_app::AppError>) -> Option<T> {
        let app = self.app.clone()?;
        let guard = match app.lock() {
            Ok(g) => g,
            Err(_) => {
                self.error = Some("internal error: app lock poisoned".into());
                return None;
            }
        };
        match f(&guard) {
            Ok(v) => Some(v),
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }

    /// Refresh the library/stat caches from the database.
    pub fn refresh_caches(&mut self) {
        if let Some(recs) = self.with_app(|app| {
            let mut stats = HashMap::new();
            let docs = app.db().list_documents()?;
            for d in &docs {
                stats.insert(d.document.id.0, app.document_stats(d.document.id)?);
            }
            let due = app.due_count()?;
            Ok((docs, stats, due))
        }) {
            (self.library, self.doc_stats, self.due_count) = recs;
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::AppOpened(Ok(app)) => {
                    let status = app.data_status().ok();
                    self.app = Some(Arc::new(Mutex::new(*app)));
                    self.data_status = status;
                    self.phase = match &self.data_status {
                        Some(s) if s.is_ready() => Phase::Ready,
                        _ => Phase::NeedsData,
                    };
                    if self.phase == Phase::Ready {
                        self.refresh_caches();
                    }
                }
                Msg::AppOpened(Err(e)) => {
                    self.error = Some(format!("failed to start: {e}"));
                }
                Msg::Progress(line) => {
                    self.progress.push(line);
                }
                Msg::DownloadDone(Ok(status)) => {
                    self.data_status = Some(status);
                    self.phase = Phase::Ready;
                    self.refresh_caches();
                }
                Msg::DownloadDone(Err(e)) => {
                    self.phase = Phase::NeedsData;
                    self.error = Some(format!("download failed: {e}"));
                }
                Msg::ImportDone(result) => {
                    self.import_jobs = self.import_jobs.saturating_sub(1);
                    match result {
                        Ok(_) => self.refresh_caches(),
                        Err(e) => self.error = Some(format!("import failed: {e}")),
                    }
                }
                Msg::Explained(result) => {
                    if let Some(reader) = &mut self.reader {
                        reader.explaining = false;
                        match result {
                            Ok(text) => reader.explanation = Some(text),
                            Err(e) => self.error = Some(e),
                        }
                    }
                }
                Msg::Feedback(result) => {
                    self.production.waiting = false;
                    match result {
                        Ok(text) => self.production.feedback = Some(text),
                        Err(e) => self.error = Some(e),
                    }
                }
            }
        }
    }

    /// Kick off the first-run dictionary/frequency download.
    pub fn start_download(&mut self, ctx: &egui::Context) {
        let Some(app) = self.app.clone() else { return };
        self.phase = Phase::Downloading;
        self.progress.clear();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(guard) => {
                    let tx2 = tx.clone();
                    let ctx2 = ctx.clone();
                    guard
                        .download_and_import_data(move |line| {
                            let _ = tx2.send(Msg::Progress(line.to_string()));
                            ctx2.request_repaint();
                        })
                        .map_err(|e| e.to_string())
                }
                Err(_) => Err("app lock poisoned".to_string()),
            };
            let _ = tx.send(Msg::DownloadDone(result));
            ctx.request_repaint();
        });
    }

    /// Import files in the background, one job per file. Extraction,
    /// metadata, and the archival copy into `<data>/books` all happen in
    /// `App::import_file`.
    pub fn start_import_files(&mut self, ctx: &egui::Context, paths: Vec<std::path::PathBuf>) {
        let Some(app) = self.app.clone() else { return };
        for path in paths {
            self.import_jobs += 1;
            let app = app.clone();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = match app.lock() {
                    Ok(guard) => guard.import_file(&path).map_err(|e| e.to_string()),
                    Err(_) => Err("app lock poisoned".to_string()),
                };
                let _ = tx.send(Msg::ImportDone(result));
                ctx.request_repaint();
            });
        }
    }

    /// Open a document in the reader.
    pub fn open_reader(&mut self, doc_id: DocumentId) {
        if let Some(state) = self.with_app(|app| {
            let doc = app.db().document(doc_id)?;
            let mut sentences = Vec::new();
            for sentence in app.db().sentences(doc_id)? {
                let tokens = app.db().sentence_tokens(sentence.id)?;
                sentences.push(SentenceView { sentence, tokens });
            }
            let groups = compute_groups(&sentences);
            let (para_ranges, para_of_sentence) = paragraph_structure(&sentences);
            let pending_restore =
                (doc.last_sentence > 0).then_some(doc.last_sentence as usize);
            Ok(ReaderState {
                doc,
                sentences,
                groups,
                selected: None,
                panel: None,
                explanation: None,
                explaining: false,
                para_ranges,
                para_of_sentence,
                page_starts: Vec::new(),
                current_page: 0,
                page_layout: (0.0, 0.0),
                token_widths: Vec::new(),
                pending_restore,
            })
        }) {
            self.reader = Some(state);
            self.view = View::Reader;
        }
    }

    /// Reload token statuses of the open document (after SRS actions).
    pub fn refresh_reader_tokens(&mut self) {
        let Some(doc_id) = self.reader.as_ref().map(|r| r.doc.id) else {
            return;
        };
        let refreshed = self.with_app(|app| {
            let mut out = Vec::new();
            for sentence in app.db().sentences(doc_id)? {
                let tokens = app.db().sentence_tokens(sentence.id)?;
                out.push(SentenceView { sentence, tokens });
            }
            Ok(out)
        });
        if let (Some(reader), Some(sentences)) = (self.reader.as_mut(), refreshed) {
            reader.groups = compute_groups(&sentences);
            reader.sentences = sentences;
        }
    }

    /// Load the dictionary panel for a word within its phrase context.
    /// `try_compound` additionally looks the whole phrase up as one
    /// dictionary word (for analyzer-split compounds).
    pub fn load_word_panel(
        &mut self,
        word_id: WordId,
        phrase: String,
        inflection: jrc_nlp::Inflection,
        try_compound: bool,
    ) -> Option<WordPanel> {
        self.with_app(|app| {
            let word = app.db().word(word_id)?;
            let entry = app.dictionary_entry_for(&word)?;
            let rank = app.db().frequency_rank(&word.key.lemma)?;
            let compound = if try_compound && phrase != word.key.lemma {
                app.lookup_compound(&phrase)?
            } else {
                None
            };
            Ok(WordPanel {
                word,
                entry,
                rank,
                phrase,
                inflection,
                compound,
            })
        })
    }

    /// Remember the first sentence of the current reader page as the
    /// user's position in the open document.
    pub fn persist_reading_position(&mut self) {
        let Some(reader) = self.reader.as_ref() else { return };
        let page = reader.current_page.min(reader.page_count() - 1);
        let para = reader.page_starts.get(page).copied().unwrap_or(0);
        let Some(&(s0, _)) = reader.para_ranges.get(para) else { return };
        let doc_id = reader.doc.id;
        self.with_app(|app| Ok(app.db().set_reading_position(doc_id, s0 as u32)?));
        if let Some(reader) = self.reader.as_mut() {
            reader.doc.last_sentence = s0 as u32;
        }
    }

    pub fn open_mining(&mut self, doc_id: DocumentId, title: String) {
        if let Some(candidates) = self.with_app(|app| app.mining_candidates(doc_id)) {
            self.mining = MiningState {
                doc_id: Some(doc_id),
                doc_title: title,
                candidates,
            };
            self.view = View::Mining;
        }
    }

    pub fn reload_mining(&mut self) {
        if let (Some(doc_id), title) = (self.mining.doc_id, self.mining.doc_title.clone()) {
            self.open_mining(doc_id, title);
        }
    }

    pub fn load_review_queue(&mut self) {
        if let Some(queue) = self.with_app(|app| app.due_reviews(100)) {
            self.review = ReviewState {
                queue,
                revealed: false,
            };
        }
    }

    /// Request an LLM explanation of the selected sentence.
    pub fn request_explanation(&mut self, ctx: &egui::Context) {
        let Some(reader) = &mut self.reader else { return };
        let Some((s_idx, g_idx)) = reader.selected else { return };
        let Some(view) = reader.sentences.get(s_idx) else { return };
        let sentence = view.sentence.text.clone();
        let focus = reader
            .groups
            .get(s_idx)
            .and_then(|g| g.get(g_idx))
            .and_then(|(start, _)| view.tokens.get(*start))
            .map(|t| t.token.lemma.clone());

        reader.explaining = true;
        reader.explanation = None;
        let explainer = self.explainer.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut context = jrc_llm::SentenceContext::new(sentence);
            if let Some(word) = focus {
                context = context.with_focus(word);
            }
            let result = explainer
                .explain_sentence(&context)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Explained(result));
            ctx.request_repaint();
        });
    }

    /// Request LLM feedback on production-mode writing.
    pub fn request_feedback(&mut self, ctx: &egui::Context) {
        let prompts = jrc_llm::writing_prompts();
        let prompt = prompts[self.production.prompt_idx % prompts.len()].to_string();
        let text = self.production.text.clone();
        self.production.waiting = true;
        self.production.feedback = None;
        let explainer = self.explainer.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = explainer
                .production_feedback(&prompt, &text)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Feedback(result));
            ctx.request_repaint();
        });
    }
}

impl eframe::App for JrcGui {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Flips persist as they happen; this catches a page reached by a
        // resize-induced repagination right before quitting.
        self.persist_reading_position();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        self.handle_messages();

        match self.phase {
            Phase::Starting => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(ui.available_height() * 0.4);
                            ui.spinner();
                            ui.label("Starting up (loading morphological dictionary)…");
                        });
                    });
                });
                return;
            }
            Phase::NeedsData | Phase::Downloading => {
                self.show_setup(ctx);
                return;
            }
            Phase::Ready => {}
        }

        // Drag-and-drop import: accepted on the library page.
        if self.view == View::Library {
            let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect()
            });
            if !dropped.is_empty() {
                self.start_import_files(ctx, dropped);
            }
        }

        self.show_nav_rail(ctx);

        if let Some(error) = self.error.clone() {
            egui::TopBottomPanel::bottom("errorbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), &error);
                    if ui.small_button("dismiss").clicked() {
                        self.error = None;
                    }
                });
            });
        }

        match self.view {
            View::Welcome => self.show_welcome(ctx),
            View::Library => self.show_library(ctx),
            View::Reader => self.show_reader(ctx),
            View::Mining => self.show_mining(ctx),
            View::Review => self.show_review(ctx),
            View::Stats => self.show_stats(ctx),
            View::Production => self.show_production(ctx),
            View::Settings => self.show_settings(ctx),
        }
    }
}

impl JrcGui {
    /// VS-Code-style icon rail on the left edge.
    fn show_nav_rail(&mut self, ctx: &egui::Context) {
        fn item(
            ui: &mut egui::Ui,
            selected: bool,
            icon: &str,
            tip: String,
            enabled: bool,
        ) -> bool {
            ui.add_enabled(
                enabled,
                egui::SelectableLabel::new(selected, egui::RichText::new(icon).size(22.0)),
            )
            .on_hover_text(tip)
            .clicked()
        }

        let mut nav: Option<View> = None;
        egui::SidePanel::left("nav-rail")
            .resizable(false)
            .exact_width(46.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    if item(ui, self.view == View::Library, "📚", "Library".into(), true) {
                        nav = Some(View::Library);
                    }
                    let reader_tip = match &self.reader {
                        Some(r) => format!("Reading: {}", truncate_title(&r.doc.title, 24)),
                        None => "Reader (open a document from the library)".into(),
                    };
                    if item(
                        ui,
                        self.view == View::Reader,
                        "📖",
                        reader_tip,
                        self.reader.is_some(),
                    ) {
                        nav = Some(View::Reader);
                    }
                    if item(
                        ui,
                        self.view == View::Mining,
                        "⛏",
                        "Vocabulary mining".into(),
                        self.mining.doc_id.is_some(),
                    ) {
                        nav = Some(View::Mining);
                    }
                    let review_tip = if self.due_count > 0 {
                        format!("Review — {} due", self.due_count)
                    } else {
                        "Review — nothing due".into()
                    };
                    if item(ui, self.view == View::Review, "🔁", review_tip, true) {
                        nav = Some(View::Review);
                    }
                    if self.due_count > 0 {
                        ui.label(
                            egui::RichText::new(self.due_count.to_string())
                                .small()
                                .color(egui::Color32::from_rgb(80, 160, 220)),
                        );
                    }
                    if item(ui, self.view == View::Stats, "📊", "Statistics".into(), true) {
                        nav = Some(View::Stats);
                    }
                    if item(
                        ui,
                        self.view == View::Production,
                        "✏",
                        "Production practice".into(),
                        true,
                    ) {
                        nav = Some(View::Production);
                    }
                    if item(ui, self.view == View::Settings, "⚙", "Settings".into(), true) {
                        nav = Some(View::Settings);
                    }
                });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if item(
                        ui,
                        self.view == View::Welcome,
                        "❓",
                        "Getting started guide".into(),
                        true,
                    ) {
                        nav = Some(View::Welcome);
                    }
                    if self.import_jobs > 0 {
                        ui.spinner();
                        ui.label(egui::RichText::new(self.import_jobs.to_string()).small())
                            .on_hover_text("imports in progress");
                    }
                });
            });

        if let Some(view) = nav {
            match view {
                View::Welcome => self.open_welcome(),
                View::Review => {
                    self.load_review_queue();
                    self.view = view;
                }
                // The progress column reflects reading done since the last
                // refresh, so returning to the library re-reads it.
                View::Library => {
                    self.refresh_caches();
                    self.view = view;
                }
                _ => self.view = view,
            }
        }
    }
}

pub fn truncate_title(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}
