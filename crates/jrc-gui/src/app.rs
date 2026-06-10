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
}

pub struct ReaderState {
    pub doc: Document,
    pub sentences: Vec<SentenceView>,
    /// (sentence index, token index) of the selected token.
    pub selected: Option<(usize, usize)>,
    pub panel: Option<WordPanel>,
    pub explanation: Option<String>,
    pub explaining: bool,
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

#[derive(Default)]
pub struct ImportState {
    pub title: String,
    pub text: String,
}

pub struct JrcGui {
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub app: Option<Arc<Mutex<App>>>,
    pub explainer: Arc<dyn Explainer>,
    pub phase: Phase,
    pub progress: Vec<String>,
    pub error: Option<String>,
    /// Set while a background import runs; gates db-touching UI.
    pub importing: bool,
    pub view: View,

    // Cached queries, refreshed on events rather than per-frame.
    pub library: Vec<DocumentSummary>,
    pub doc_stats: HashMap<i64, DocStats>,
    pub due_count: u64,

    pub import: ImportState,
    pub reader: Option<ReaderState>,
    pub mining: MiningState,
    pub review: ReviewState,
    pub production: ProductionState,
    pub data_status: Option<DataStatus>,
    pub data_dir: PathBuf,
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
            explainer: Arc::from(jrc_llm::explainer_from_env()),
            phase: Phase::Starting,
            progress: Vec::new(),
            error: None,
            importing: false,
            view: View::Library,
            library: Vec::new(),
            doc_stats: HashMap::new(),
            due_count: 0,
            import: ImportState::default(),
            reader: None,
            mining: MiningState::default(),
            review: ReviewState::default(),
            production: ProductionState::default(),
            data_status: None,
            data_dir,
        }
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
                    self.importing = false;
                    match result {
                        Ok(_) => {
                            self.import = ImportState::default();
                            self.refresh_caches();
                        }
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

    /// Import pasted text (or a file's contents) in the background.
    pub fn start_import(&mut self, ctx: &egui::Context, title: String, text: String) {
        let Some(app) = self.app.clone() else { return };
        self.importing = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(guard) => guard.import_text(&title, &text).map_err(|e| e.to_string()),
                Err(_) => Err("app lock poisoned".to_string()),
            };
            let _ = tx.send(Msg::ImportDone(result));
            ctx.request_repaint();
        });
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
            Ok(ReaderState {
                doc,
                sentences,
                selected: None,
                panel: None,
                explanation: None,
                explaining: false,
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
            reader.sentences = sentences;
        }
    }

    /// Load the dictionary panel for a word.
    pub fn load_word_panel(&mut self, word_id: WordId) -> Option<WordPanel> {
        self.with_app(|app| {
            let word = app.db().word(word_id)?;
            let entry = app.dictionary_entry_for(&word)?;
            let rank = app.db().frequency_rank(&word.key.lemma)?;
            Ok(WordPanel { word, entry, rank })
        })
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
        let Some((s_idx, t_idx)) = reader.selected else { return };
        let Some(view) = reader.sentences.get(s_idx) else { return };
        let sentence = view.sentence.text.clone();
        let focus = view.tokens.get(t_idx).map(|t| t.token.lemma.clone());

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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        egui::TopBottomPanel::top("topbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("読書");
                ui.separator();
                ui.selectable_value(&mut self.view, View::Library, "Library");
                let reader_label = match &self.reader {
                    Some(r) => format!("Reading: {}", truncate_title(&r.doc.title, 18)),
                    None => "Reader".to_string(),
                };
                ui.add_enabled_ui(self.reader.is_some(), |ui| {
                    ui.selectable_value(&mut self.view, View::Reader, reader_label);
                });
                ui.add_enabled_ui(self.mining.doc_id.is_some(), |ui| {
                    ui.selectable_value(&mut self.view, View::Mining, "Mining");
                });
                let review_label = if self.due_count > 0 {
                    format!("Review ({})", self.due_count)
                } else {
                    "Review".to_string()
                };
                if ui
                    .selectable_value(&mut self.view, View::Review, review_label)
                    .clicked()
                {
                    self.load_review_queue();
                }
                ui.selectable_value(&mut self.view, View::Stats, "Stats");
                ui.selectable_value(&mut self.view, View::Production, "Production");
                ui.selectable_value(&mut self.view, View::Settings, "Settings");

                if self.importing {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spinner();
                        ui.label("importing…");
                    });
                }
            });
        });

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

pub fn truncate_title(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}
