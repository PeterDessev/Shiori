//! Application shell: state, background tasks, frame loop.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use shiori_app::{App, DataStatus, DocStats, ReviewItem};
use shiori_core::{Document, DocumentId, WordId};
use shiori_db::{DocumentSummary, TokenRow, WordRow};
use shiori_dict::DictEntry;
use shiori_llm::Explainer;

use crate::settings::{Settings, Theme};

/// Messages posted back from background threads.
pub enum Msg {
    AppOpened(Result<Box<App>, String>),
    Progress(String),
    DownloadDone(Result<DataStatus, String>),
    ImportDone(Result<DocumentId, String>),
    Explained(Result<String, String>),
    /// Outcome of a chat turn: the user message the write-up belongs to,
    /// and the parsed reply + annotations.
    ChatReply(i64, Result<shiori_llm::ChatTurnOutcome, String>),
    FontDownloaded(crate::settings::ReaderFont, Result<(), String>),
    OllamaProbe(Result<(String, Vec<shiori_llm::OllamaModel>), String>),
    OllamaPullProgress(String, Option<f32>),
    OllamaPullDone(Result<(), String>),
    AozoraCatalog(Result<Vec<shiori_app::AozoraWork>, String>),
    WikisourceResults(Result<Vec<shiori_app::WikisourceHit>, String>),
    /// Export/import/backup finished: Ok(summary) or Err(reason).
    TransferDone(Result<String, String>),
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
    Review,
    Dictionary,
    Sources,
    Stats,
    Production,
    Settings,
}

/// One sentence of the open document, with its tokens.
pub struct SentenceView {
    pub sentence: shiori_core::Sentence,
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
    pub inflection: shiori_nlp::Inflection,
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
    /// Reading clock for this sitting.
    pub session: crate::session::SessionTracker,
    /// Per sentence, per token: the 1-based occurrence index of that
    /// token's word within this document, in reading order. Drives the
    /// "first X instances" furigana mode; instance-anchored, so it never
    /// changes however the user flips around.
    pub word_occurrence: Vec<Vec<u32>>,
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

/// Number each token with its word's occurrence index (1-based) in
/// document order.
fn word_occurrences(sentences: &[SentenceView]) -> Vec<Vec<u32>> {
    let mut counts: HashMap<i64, u32> = HashMap::new();
    sentences
        .iter()
        .map(|view| {
            view.tokens
                .iter()
                .map(|row| {
                    let n = counts.entry(row.word_id.0).or_insert(0);
                    *n += 1;
                    *n
                })
                .collect()
        })
        .collect()
}

/// Compute phrase groups for each sentence.
fn compute_groups(sentences: &[SentenceView]) -> Vec<Vec<(usize, usize)>> {
    sentences
        .iter()
        .map(|view| {
            let tokens: Vec<shiori_core::Token> =
                view.tokens.iter().map(|r| r.token.clone()).collect();
            shiori_nlp::phrase_groups(&tokens)
        })
        .collect()
}

#[derive(Default)]
pub struct ReviewState {
    pub queue: Vec<ReviewItem>,
    pub revealed: bool,
}

#[derive(Default)]
pub struct DictionaryState {
    pub query: String,
    /// Query the current results belong to.
    pub searched_for: String,
    pub results: shiori_app::DictSearchResults,
    /// Word ids whose example-sentence panel is expanded.
    pub examples_open: HashSet<i64>,
    /// Lazily fetched library example sentences, keyed by word id.
    pub examples: HashMap<i64, Vec<shiori_app::DictExample>>,
    /// The "more info" modal for a word card, when open.
    pub info: Option<DictInfoModal>,
    /// Set the frame the modal opens, so the opening click is not mistaken
    /// for a click-away that would immediately close it.
    pub info_just_opened: bool,
}

/// Snapshot powering a word card's "more info" modal: the card's own
/// content plus the kanji cards for every kanji in the word, captured when
/// the modal is opened. Example sentences are read live from the
/// [`DictionaryState::examples`] cache via `word_id`.
#[derive(Default)]
pub struct DictInfoModal {
    /// Tracked word id, for reading cached examples; `None` when the word
    /// has never been met in the library.
    pub word_id: Option<i64>,
    pub headword: String,
    /// Kana reading to show in parentheses; empty when same as headword.
    pub reading: String,
    pub jlpt: Option<u8>,
    /// Knowledge status of the tracked word, if any.
    pub status: Option<String>,
    pub pos: Vec<String>,
    /// Numbered gloss lines, mirroring the card.
    pub senses: Vec<String>,
    /// Kanji cards for each distinct kanji in the headword.
    pub kanji: Vec<shiori_db::KanjiRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SourceTab {
    #[default]
    Aozora,
    Wikisource,
}

#[derive(Default)]
pub struct SourcesState {
    pub query: String,
    pub tab: SourceTab,
    /// Parsed Aozora catalog (public-domain, importable works).
    pub catalog: Option<Vec<shiori_app::AozoraWork>>,
    pub catalog_loading: bool,
    pub ws_results: Vec<shiori_app::WikisourceHit>,
    pub ws_searching: bool,
}

/// One chat message prepared for display.
pub struct ChatMessageView {
    pub id: i64,
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
    /// Write-up spans (byte offsets into content); user messages only.
    pub annotations: Vec<shiori_db::ChatAnnotationRow>,
    /// Per sentence: clickable tokens (absolute offsets) + phrase groups.
    pub sentences: Vec<shiori_app::ChatSentence>,
}

#[derive(Default)]
pub struct ProductionState {
    pub conversations: Vec<shiori_db::ConversationRow>,
    pub current: Option<i64>,
    pub messages: Vec<ChatMessageView>,
    pub input: String,
    pub waiting: bool,
    /// Conversation list has been loaded this session.
    pub loaded: bool,
    /// Dictionary panel for the clicked chat word.
    pub panel: Option<WordPanel>,
    /// Write-up note overlapping the clicked word, if any.
    pub panel_note: Option<String>,
}

/// Per-document metadata being edited in the library dialog.
pub struct MetaEdit {
    pub id: DocumentId,
    pub meta: shiori_core::DocumentMeta,
}

/// Contents of the library's book-info side panel.
pub struct BookInfo {
    pub id: DocumentId,
    pub reading: shiori_db::ReadingTotals,
    /// Unknown content words, most useful to learn first.
    pub top_unknown: Vec<shiori_app::MiningCandidate>,
}

/// The finish-sweep confirmation dialog.
pub struct SweepState {
    pub doc: DocumentId,
    pub plan: shiori_app::SweepPlan,
    /// Inclusion checkbox per `plan.to_known` entry; suspicious words
    /// default to excluded.
    pub include: Vec<bool>,
}

/// One queued source-import job.
pub enum SourceImport {
    Aozora(shiori_app::AozoraWork),
    Wikisource(String),
}

/// Press-to-record state for one shortcut binding. The combo is
/// committed when the first held key is released ("burned in on
/// release"); Escape cancels.
pub struct ShortcutRecording {
    pub id: crate::settings::ShortcutId,
    /// (modifiers at press, key) of the last non-modifier key pressed.
    pub captured: Option<(egui::Modifiers, egui::Key)>,
    /// Modifier state last frame, to detect a modifier being released
    /// first (which also commits the combo).
    pub prev_modifiers: egui::Modifiers,
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

pub struct ShioriGui {
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub app: Option<Arc<Mutex<App>>>,
    pub explainer: Arc<dyn Explainer>,
    pub phase: Phase,
    pub progress: Vec<String>,
    pub error: Option<String>,
    /// Green, dismissible success message (exports, backups).
    pub notice: Option<String>,
    /// Number of background import jobs in flight.
    pub import_jobs: usize,
    pub view: View,

    // Cached queries, refreshed on events rather than per-frame.
    pub library: Vec<DocumentSummary>,
    pub doc_stats: HashMap<i64, DocStats>,
    pub due_count: u64,

    pub meta_edit: Option<MetaEdit>,
    pub book_info: Option<BookInfo>,
    pub sweep: Option<SweepState>,
    pub reader: Option<ReaderState>,
    pub review: ReviewState,
    pub dictionary: DictionaryState,
    pub sources: SourcesState,
    pub production: ProductionState,
    pub data_status: Option<DataStatus>,
    pub data_dir: PathBuf,
    pub settings: Settings,
    /// Editable copy shown in the settings view (saved explicitly).
    pub settings_draft: Settings,
    /// Which settings category page is open.
    pub settings_category: crate::views::SettingsCategory,
    /// In-progress press-to-record shortcut capture.
    pub shortcut_recording: Option<ShortcutRecording>,
    /// Conflict/info notice under the shortcuts grid.
    pub shortcut_notice: Option<String>,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    /// Theme applied to the egui context (to detect setting changes).
    applied_theme: Option<Theme>,
    /// Japanese font currently installed in the egui context.
    applied_font: Option<crate::settings::ReaderFont>,
    /// A font download is in flight.
    pub font_downloading: bool,
    /// Where to return when the getting-started page is closed.
    pub welcome_return: Option<View>,
    /// The no-dictionary banner was dismissed this run.
    pub dict_banner_dismissed: bool,
    /// The "what works without the dictionary" modal is open.
    pub offline_info_open: bool,
    /// Last Ollama probe: server version + installed models, or why not.
    pub ollama_probe: Option<Result<(String, Vec<shiori_llm::OllamaModel>), String>>,
    pub ollama_probing: bool,
    /// Model name typed into the pull box.
    pub ollama_pull_input: String,
    /// In-flight pull: (status line, completed fraction).
    pub ollama_pull: Option<(String, Option<f32>)>,
}

pub fn default_data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let new = base.join("shiori");
    // One-time migration from the pre-Shiori directory name. If the
    // rename is blocked (another instance, antivirus), keep using the
    // old directory rather than starting over with empty data.
    let old = base.join("japanese-reading-companion");
    if !new.exists() && old.exists() && std::fs::rename(&old, &new).is_err() {
        return old;
    }
    new
}

impl ShioriGui {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        let data_dir = default_data_dir();
        let settings = Settings::load(&data_dir);

        // Install whichever Japanese font is usable right now; if the
        // chosen Noto font isn't cached yet, the first frame's
        // apply_fonts kicks off the download and switches on arrival.
        let initial_font = if crate::fonts::font_available(&data_dir, settings.reader_font) {
            settings.reader_font
        } else {
            crate::settings::ReaderFont::System
        };
        crate::fonts::install_japanese_fonts(&cc.egui_ctx, &data_dir, initial_font);

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
            notice: None,
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
            book_info: None,
            sweep: None,
            reader: None,
            review: ReviewState::default(),
            dictionary: DictionaryState::default(),
            sources: SourcesState::default(),
            production: ProductionState::default(),
            data_status: None,
            data_dir,
            settings_draft: settings.clone(),
            settings,
            settings_category: Default::default(),
            shortcut_recording: None,
            shortcut_notice: None,
            sort_key: SortKey::default(),
            sort_asc: true,
            applied_theme: None,
            applied_font: Some(initial_font),
            font_downloading: false,
            welcome_return: None,
            dict_banner_dismissed: false,
            offline_info_open: false,
            ollama_probe: None,
            ollama_probing: false,
            ollama_pull_input: String::new(),
            ollama_pull: None,
        }
    }

    /// Whether the reference data (dictionary + frequency list) is loaded.
    pub fn dict_ready(&self) -> bool {
        self.data_status.as_ref().is_some_and(|s| s.is_ready())
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.theme) {
            ctx.set_visuals(match self.settings.theme {
                Theme::Dark => egui::Visuals::dark(),
                Theme::Light => egui::Visuals::light(),
                Theme::Sepia => sepia_visuals(),
            });
            self.applied_theme = Some(self.settings.theme);
        }
    }

    /// Make the installed Japanese font match the setting, downloading a
    /// Noto font in the background the first time it is chosen.
    fn apply_fonts(&mut self, ctx: &egui::Context) {
        let wanted = self.settings.reader_font;
        if self.applied_font == Some(wanted) {
            return;
        }
        if crate::fonts::font_available(&self.data_dir, wanted) {
            crate::fonts::install_japanese_fonts(ctx, &self.data_dir, wanted);
            self.applied_font = Some(wanted);
            // Different metrics: re-measure and re-paginate the open book.
            self.invalidate_reader_layout();
        } else if !self.font_downloading {
            self.font_downloading = true;
            let data_dir = self.data_dir.clone();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = crate::fonts::download_font(&data_dir, wanted);
                let _ = tx.send(Msg::FontDownloaded(wanted, result));
                ctx.request_repaint();
            });
        }
    }

    /// Persist the settings draft and apply it (rebuilds the LLM backend).
    pub fn apply_settings(&mut self) {
        let layout_changed = self.settings.reader_font_size != self.settings_draft.reader_font_size
            || self.settings.reader_line_spacing != self.settings_draft.reader_line_spacing
            || self.settings.furigana != self.settings_draft.furigana
            || self.settings.furigana_first_x != self.settings_draft.furigana_first_x;
        self.settings = self.settings_draft.clone();
        if let Err(e) = self.settings.save(&self.data_dir) {
            self.error = Some(format!("could not save settings: {e}"));
        }
        self.explainer = self.settings.build_explainer();
        if layout_changed {
            self.invalidate_reader_layout();
        }
    }

    /// Force the reader to re-measure tokens and re-paginate (after a
    /// font, size, or spacing change), keeping the current position.
    pub fn invalidate_reader_layout(&mut self) {
        if let Some(reader) = self.reader.as_mut() {
            let para = reader
                .page_starts
                .get(reader.current_page)
                .copied()
                .unwrap_or(0);
            let sentence = reader.para_ranges.get(para).map(|&(s0, _)| s0).unwrap_or(0);
            reader.token_widths.clear();
            reader.page_starts.clear();
            reader.pending_restore = Some(sentence);
        }
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
    pub fn with_app<T>(
        &mut self,
        f: impl FnOnce(&App) -> Result<T, shiori_app::AppError>,
    ) -> Option<T> {
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

    fn handle_messages(&mut self, ctx: &egui::Context) {
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
                    // Fetch/refresh the Aozora catalog in the background;
                    // offline falls back to the cached copy.
                    self.start_catalog_load(ctx, false);
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
                Msg::ChatReply(user_msg_id, result) => {
                    self.production.waiting = false;
                    match result {
                        Ok(outcome) => self.apply_chat_reply(user_msg_id, outcome),
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::OllamaProbe(result) => {
                    self.ollama_probing = false;
                    self.ollama_probe = Some(result);
                }
                Msg::AozoraCatalog(result) => {
                    self.sources.catalog_loading = false;
                    match result {
                        Ok(works) => self.sources.catalog = Some(works),
                        // Offline or fetch failure: not an error toast —
                        // the sources view explains and offers reload.
                        Err(e) => eprintln!("aozora catalog unavailable: {e}"),
                    }
                }
                Msg::WikisourceResults(result) => {
                    self.sources.ws_searching = false;
                    match result {
                        Ok(hits) => self.sources.ws_results = hits,
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::TransferDone(result) => match result {
                    Ok(summary) => {
                        self.notice = Some(summary);
                        self.refresh_caches();
                    }
                    Err(e) => self.error = Some(e),
                },
                Msg::OllamaPullProgress(status, frac) => {
                    self.ollama_pull = Some((status, frac));
                }
                Msg::OllamaPullDone(result) => {
                    self.ollama_pull = None;
                    match result {
                        // Forget the old probe so the model list refreshes.
                        Ok(()) => self.ollama_probe = None,
                        Err(e) => self.error = Some(format!("model pull failed: {e}")),
                    }
                }
                Msg::FontDownloaded(font, result) => {
                    self.font_downloading = false;
                    if let Err(e) = result {
                        self.error = Some(format!("font download failed: {e}"));
                        // Fall back so apply_fonts stops retrying.
                        if self.settings.reader_font == font {
                            self.settings.reader_font = crate::settings::ReaderFont::System;
                            self.settings_draft.reader_font = self.settings.reader_font;
                            let _ = self.settings.save(&self.data_dir);
                        }
                    }
                    // On success apply_fonts installs it next frame.
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
        // Flush the sitting with the previous document, if any.
        self.end_page_visit(crate::session::VisitEnd::Pause);
        if let Some(state) = self.with_app(|app| {
            let doc = app.db().document(doc_id)?;
            let mut sentences = Vec::new();
            for sentence in app.db().sentences(doc_id)? {
                let tokens = app.db().sentence_tokens(sentence.id)?;
                sentences.push(SentenceView { sentence, tokens });
            }
            let groups = compute_groups(&sentences);
            let (para_ranges, para_of_sentence) = paragraph_structure(&sentences);
            let word_occurrence = word_occurrences(&sentences);
            let pending_restore = (doc.last_sentence > 0).then_some(doc.last_sentence as usize);
            let velocity = app.reading_velocity_cps()?;
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
                session: crate::session::SessionTracker::new(velocity),
                word_occurrence,
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
            reader.word_occurrence = word_occurrences(&sentences);
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
        inflection: shiori_nlp::Inflection,
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
    /// user's position in the open document. Reaching the last page
    /// saves one-past-the-end, which reads as 100% (finished) in the
    /// library and unlocks the finish sweep.
    pub fn persist_reading_position(&mut self) {
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        if reader.page_starts.is_empty() {
            // Not laid out yet; nothing meaningful to save.
            return;
        }
        let page = reader.current_page.min(reader.page_count() - 1);
        let s0 = if page + 1 == reader.page_count() {
            reader.sentences.len() as u32
        } else {
            let para = reader.page_starts.get(page).copied().unwrap_or(0);
            let Some(&(s0, _)) = reader.para_ranges.get(para) else {
                return;
            };
            s0 as u32
        };
        let doc_id = reader.doc.id;
        self.with_app(|app| Ok(app.db().set_reading_position(doc_id, s0)?));
        if let Some(reader) = self.reader.as_mut() {
            reader.doc.last_sentence = s0;
        }
    }

    /// Probe the configured Ollama server for liveness and models.
    pub fn probe_ollama(&mut self, ctx: &egui::Context) {
        if self.ollama_probing {
            return;
        }
        self.ollama_probing = true;
        let url = self.settings_draft.ollama_url.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = shiori_llm::OllamaClient::new(url);
            let result = client
                .version()
                .and_then(|version| Ok((version, client.list_models()?)))
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::OllamaProbe(result));
            ctx.request_repaint();
        });
    }

    /// Pull a model into Ollama with streamed progress.
    pub fn pull_ollama_model(&mut self, ctx: &egui::Context, model: String) {
        if self.ollama_pull.is_some() || model.trim().is_empty() {
            return;
        }
        self.ollama_pull = Some(("starting…".into(), None));
        let url = self.settings_draft.ollama_url.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let client = shiori_llm::OllamaClient::new(url);
            let mut last_sent = -1.0f32;
            let result = client
                .pull(model.trim(), |p| {
                    let frac = match (p.completed, p.total) {
                        (Some(c), Some(t)) if t > 0 => Some(c as f32 / t as f32),
                        _ => None,
                    };
                    // Throttle: a pull emits thousands of lines.
                    let significant = match frac {
                        Some(f) => (f - last_sent).abs() > 0.01,
                        None => true,
                    };
                    if significant {
                        if let Some(f) = frac {
                            last_sent = f;
                        }
                        let status = p.status.clone().unwrap_or_default();
                        let _ = tx.send(Msg::OllamaPullProgress(status, frac));
                        ctx.request_repaint();
                    }
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::OllamaPullDone(result));
            ctx.request_repaint();
        });
    }

    /// Plan a finish sweep and open its confirmation dialog.
    pub fn open_sweep_dialog(&mut self, id: DocumentId) {
        if let Some(plan) = self.with_app(|app| app.finish_sweep_plan(id)) {
            let include = plan.to_known.iter().map(|c| !c.suspicious).collect();
            self.sweep = Some(SweepState {
                doc: id,
                plan,
                include,
            });
        }
    }

    /// Run a blocking export/import job against the app on a worker
    /// thread; the outcome lands in the notice/error bar.
    pub fn run_transfer<F>(&mut self, ctx: &egui::Context, job: F)
    where
        F: FnOnce(&App) -> Result<String, shiori_app::AppError> + Send + 'static,
    {
        let Some(app) = self.app.clone() else { return };
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(guard) => job(&guard).map_err(|e| e.to_string()),
                Err(_) => Err("app lock poisoned".into()),
            };
            let _ = tx.send(Msg::TransferDone(result));
            ctx.request_repaint();
        });
    }

    /// Load (or force-reload) the Aozora catalog in the background.
    pub fn start_catalog_load(&mut self, ctx: &egui::Context, force: bool) {
        if self.sources.catalog_loading {
            return;
        }
        let Some(app) = self.app.clone() else { return };
        self.sources.catalog_loading = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(guard) => guard
                    .ensure_aozora_catalog(force)
                    .and_then(|path| guard.load_aozora_catalog(&path))
                    .map_err(|e| e.to_string()),
                Err(_) => Err("app lock poisoned".into()),
            };
            let _ = tx.send(Msg::AozoraCatalog(result));
            ctx.request_repaint();
        });
    }

    /// Search Japanese Wikisource in the background.
    pub fn start_wikisource_search(&mut self, ctx: &egui::Context) {
        if self.sources.ws_searching || self.sources.query.trim().is_empty() {
            return;
        }
        let Some(app) = self.app.clone() else { return };
        self.sources.ws_searching = true;
        let query = self.sources.query.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(guard) => guard.search_wikisource(&query).map_err(|e| e.to_string()),
                Err(_) => Err("app lock poisoned".into()),
            };
            let _ = tx.send(Msg::WikisourceResults(result));
            ctx.request_repaint();
        });
    }

    /// Download and import one work from a source in the background.
    pub fn start_source_import(&mut self, ctx: &egui::Context, job: SourceImport) {
        let Some(app) = self.app.clone() else { return };
        self.import_jobs += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(guard) => match &job {
                    SourceImport::Aozora(work) => {
                        guard.import_aozora_work(work).map_err(|e| e.to_string())
                    }
                    SourceImport::Wikisource(title) => guard
                        .import_wikisource_page(title)
                        .map_err(|e| e.to_string()),
                },
                Err(_) => Err("app lock poisoned".into()),
            };
            let _ = tx.send(Msg::ImportDone(result));
            ctx.request_repaint();
        });
    }

    /// Jump to the dictionary view with a query (e.g. a kanji chip from
    /// the reader's word panel).
    pub fn open_dictionary(&mut self, query: String) {
        self.dictionary.query = query;
        self.dictionary.searched_for.clear(); // force a re-search
        self.view = View::Dictionary;
    }

    /// Open (or refresh) the library's book-info side panel.
    pub fn open_book_info(&mut self, id: DocumentId) {
        if let Some((reading, top_unknown)) = self.with_app(|app| {
            Ok((
                app.db().document_reading_totals(id)?,
                app.mining_candidates(id)?,
            ))
        }) {
            self.book_info = Some(BookInfo {
                id,
                reading,
                top_unknown,
            });
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
        let Some(reader) = &mut self.reader else {
            return;
        };
        let Some((s_idx, g_idx)) = reader.selected else {
            return;
        };
        let Some(view) = reader.sentences.get(s_idx) else {
            return;
        };
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
            let mut context = shiori_llm::SentenceContext::new(sentence);
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

    /// Load (or reload) the conversation list for the chat sidebar.
    pub fn load_conversations(&mut self) {
        if let Some(list) = self.with_app(|app| Ok(app.db().list_conversations()?)) {
            self.production.conversations = list;
            self.production.loaded = true;
        }
    }

    /// Open a conversation: load its messages, annotations, and analysis.
    pub fn open_conversation(&mut self, id: i64) {
        let loaded = self.with_app(|app| {
            let mut views = Vec::new();
            for row in app.db().conversation_messages(id)? {
                let annotations = if row.role == "user" {
                    app.db().chat_annotations(row.id)?
                } else {
                    Vec::new()
                };
                let sentences = app.analyze_chat_text(&row.content)?;
                views.push(ChatMessageView {
                    id: row.id,
                    role: row.role,
                    content: row.content,
                    annotations,
                    sentences,
                });
            }
            Ok(views)
        });
        if let Some(messages) = loaded {
            self.production.current = Some(id);
            self.production.messages = messages;
            self.production.panel = None;
            self.production.panel_note = None;
        }
    }

    /// Send the input box as a user message and request the partner's
    /// reply + write-up in the background.
    pub fn send_chat_message(&mut self, ctx: &egui::Context) {
        let content = self.production.input.trim().to_string();
        if content.is_empty() || self.production.waiting || !self.explainer.is_available() {
            return;
        }

        // Make sure a conversation exists, titled after the first message.
        let conversation = match self.production.current {
            Some(id) => id,
            None => {
                let title = truncate_title(&content, 24);
                let Some(id) = self.with_app(|app| {
                    Ok(app.db().create_conversation(chrono::Utc::now(), &title)?)
                }) else {
                    return;
                };
                self.production.current = Some(id);
                self.production.messages.clear();
                id
            }
        };

        let Some((msg_id, sentences)) = self.with_app(|app| {
            let id =
                app.db()
                    .add_chat_message(conversation, "user", &content, chrono::Utc::now())?;
            Ok((id, app.analyze_chat_text(&content)?))
        }) else {
            return;
        };
        self.production.messages.push(ChatMessageView {
            id: msg_id,
            role: "user".into(),
            content: content.clone(),
            annotations: Vec::new(),
            sentences,
        });
        self.production.input.clear();
        self.load_conversations();

        // History for the model: the visible transcript, most recent 20.
        let history: Vec<shiori_llm::ChatMessage> = self
            .production
            .messages
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|m| shiori_llm::ChatMessage {
                role: if m.role == "user" {
                    shiori_llm::ChatRole::User
                } else {
                    shiori_llm::ChatRole::Assistant
                },
                content: m.content.clone(),
            })
            .collect();
        let level_hint = self
            .with_app(|app| app.chat_level_hint())
            .unwrap_or_else(|| "Level unknown; infer it from their messages.".into());
        let system =
            shiori_llm::chat_system_prompt(&level_hint, self.settings.chat_challenge.to_llm());

        self.production.waiting = true;
        let explainer = self.explainer.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = explainer
                .chat(&system, &history)
                .and_then(|raw| shiori_llm::parse_chat_response(&raw, &content))
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::ChatReply(msg_id, result));
            ctx.request_repaint();
        });
    }

    /// Store and display a finished chat turn.
    fn apply_chat_reply(&mut self, user_msg_id: i64, outcome: shiori_llm::ChatTurnOutcome) {
        let Some(conversation) = self.production.current else {
            return;
        };

        let annotations: Vec<shiori_db::ChatAnnotationRow> = outcome
            .annotations
            .iter()
            .map(|a| shiori_db::ChatAnnotationRow {
                start: a.start,
                end: a.end,
                severity: a.severity.as_str().to_string(),
                note: a.note.clone(),
            })
            .collect();
        let reply = outcome.reply;
        let stored = self.with_app(|app| {
            app.db().add_chat_annotations(user_msg_id, &annotations)?;
            let id =
                app.db()
                    .add_chat_message(conversation, "assistant", &reply, chrono::Utc::now())?;
            Ok((id, app.analyze_chat_text(&reply)?))
        });
        if let Some(message) = self
            .production
            .messages
            .iter_mut()
            .find(|m| m.id == user_msg_id)
        {
            message.annotations = annotations;
        }
        if let Some((id, sentences)) = stored {
            self.production.messages.push(ChatMessageView {
                id,
                role: "assistant".into(),
                content: reply,
                annotations: Vec::new(),
                sentences,
            });
        }
    }
}

impl eframe::App for ShioriGui {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Flips persist as they happen; this catches a page reached by a
        // resize-induced repagination right before quitting.
        self.persist_reading_position();
        // Credit the reading done on the final page.
        self.end_page_visit(crate::session::VisitEnd::Pause);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        self.apply_fonts(ctx);
        self.handle_messages(ctx);

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
        self.show_dictionary_banner(ctx);

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
        if let Some(notice) = self.notice.clone() {
            egui::TopBottomPanel::bottom("noticebar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(110, 180, 110), &notice);
                    if ui.small_button("dismiss").clicked() {
                        self.notice = None;
                    }
                });
            });
        }

        match self.view {
            View::Welcome => self.show_welcome(ctx),
            View::Library => self.show_library(ctx),
            View::Reader => self.show_reader(ctx),
            View::Review => self.show_review(ctx),
            View::Dictionary => self.show_dictionary(ctx),
            View::Sources => self.show_sources(ctx),
            View::Stats => self.show_stats(ctx),
            View::Production => self.show_production(ctx),
            View::Settings => self.show_settings(ctx),
        }
    }
}

impl ShioriGui {
    /// Banner shown while running without the reference dictionary, with
    /// retry and an explanation modal. This banner (and in-place notices
    /// where lookups break) is the only place the offline path is
    /// explained.
    fn show_dictionary_banner(&mut self, ctx: &egui::Context) {
        if self.dict_ready() || self.dict_banner_dismissed {
            return;
        }
        egui::TopBottomPanel::top("dict-banner").show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 160, 60),
                    "Dictionary not installed — word lookups are unavailable.",
                );
                if ui.button("Retry download").clicked() {
                    self.start_download(ctx);
                }
                if ui
                    .button("ⓘ")
                    .on_hover_text("What works without it?")
                    .clicked()
                {
                    self.offline_info_open = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text("Dismiss").clicked() {
                        self.dict_banner_dismissed = true;
                    }
                });
            });
            ui.add_space(3.0);
        });

        if self.offline_info_open {
            let mut open = true;
            egui::Window::new("Running without the dictionary")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("Unavailable until the reference data is downloaded:");
                    ui.label("  · dictionary entries and definitions");
                    ui.label("  · compound-word lookups");
                    ui.label("  · corpus frequency ranks");
                    ui.label("  · usage registers (formal, colloquial, …)");
                    ui.add_space(8.0);
                    ui.label("Everything else works normally:");
                    ui.label("  · importing and reading books");
                    ui.label("  · marking words and SRS reviews");
                    ui.label("  · difficulty statistics and recommendations");
                    ui.label("  · LLM features, including local models");
                    ui.add_space(8.0);
                    ui.weak(
                        "Retry the download any time from the banner; it needs \
                         a network connection once.",
                    );
                });
            if !open {
                self.offline_info_open = false;
            }
        }
    }

    /// VS-Code-style icon rail on the left edge.
    fn show_nav_rail(&mut self, ctx: &egui::Context) {
        fn item(ui: &mut egui::Ui, selected: bool, icon: &str, tip: String, enabled: bool) -> bool {
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
                    if item(
                        ui,
                        self.view == View::Dictionary,
                        "🔍",
                        "Dictionary & kanji".into(),
                        true,
                    ) {
                        nav = Some(View::Dictionary);
                    }
                    if item(
                        ui,
                        self.view == View::Sources,
                        "🌐",
                        "Find books online".into(),
                        true,
                    ) {
                        nav = Some(View::Sources);
                    }
                    if item(
                        ui,
                        self.view == View::Stats,
                        "📊",
                        "Statistics".into(),
                        true,
                    ) {
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
                    if item(
                        ui,
                        self.view == View::Settings,
                        "⚙",
                        "Settings".into(),
                        true,
                    ) {
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
            // The reading clock only runs while the reader is on screen.
            if self.view == View::Reader && view != View::Reader {
                self.end_page_visit(crate::session::VisitEnd::Pause);
            } else if view == View::Reader && self.view != View::Reader {
                self.enter_page();
            }
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
                // Re-running the search on return surfaces words and example
                // sentences added to the SRS since leaving, while keeping the
                // query and any open word-detail modal.
                View::Dictionary => {
                    self.reenter_dictionary();
                    self.view = view;
                }
                _ => self.view = view,
            }
        }
    }
}

/// Warm paper-toned visuals for long reading sessions.
fn sepia_visuals() -> egui::Visuals {
    use egui::{Color32, Stroke};
    let mut v = egui::Visuals::light();
    let text = Color32::from_rgb(66, 50, 35);
    v.override_text_color = Some(text);
    v.panel_fill = Color32::from_rgb(240, 228, 205);
    v.window_fill = Color32::from_rgb(246, 236, 216);
    v.extreme_bg_color = Color32::from_rgb(250, 242, 226);
    v.faint_bg_color = Color32::from_rgb(232, 218, 192);
    v.code_bg_color = Color32::from_rgb(232, 218, 192);
    v.selection.bg_fill = Color32::from_rgb(213, 178, 122);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(120, 90, 50));
    v.hyperlink_color = Color32::from_rgb(140, 90, 40);
    v.warn_fg_color = Color32::from_rgb(160, 100, 20);
    v.error_fg_color = Color32::from_rgb(170, 50, 40);
    v.widgets.noninteractive.bg_fill = v.panel_fill;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.inactive.bg_fill = Color32::from_rgb(228, 213, 185);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(228, 213, 185);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.hovered.bg_fill = Color32::from_rgb(219, 201, 168);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(219, 201, 168);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, text);
    v.widgets.active.bg_fill = Color32::from_rgb(208, 188, 152);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(208, 188, 152);
    v.widgets.active.fg_stroke = Stroke::new(1.5, text);
    v.widgets.open.bg_fill = Color32::from_rgb(228, 213, 185);
    v.widgets.open.weak_bg_fill = Color32::from_rgb(228, 213, 185);
    v.widgets.open.fg_stroke = Stroke::new(1.0, text);
    v
}

pub fn truncate_title(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars).collect();
        format!("{cut}…")
    }
}
