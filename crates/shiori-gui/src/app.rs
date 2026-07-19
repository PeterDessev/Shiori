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

/// Default UI zoom factor: two Ctrl+Plus steps (0.1 each) above egui's
/// 1.0 baseline. Applied at startup and used as the Ctrl+0 reset target.
const DEFAULT_ZOOM_FACTOR: f32 = 1.2;

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
    /// The hosted pack catalog arrived (or failed to).
    PackCatalog(Result<Vec<shiori_app::PackCatalogEntry>, String>),
    /// Export/import/backup finished: Ok(summary) or Err(reason).
    TransferDone(Result<String, String>),
    /// A pack install/download/removal finished. Separate from
    /// [`Msg::TransferDone`] so unrelated transfers can never clear the
    /// installing flag while a pack job is still running.
    PackJobDone(Result<String, String>),
    /// Status line from a long pack job (web builds report download and
    /// scan progress).
    PackJobProgress(String),
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
    Home,
    Library,
    Reader,
    Review,
    Dictionary,
    Sources,
    Stats,
    Production,
    Settings,
}

/// Cached contents of the home page. Recomputed when the page is
/// (re)entered and dropped whenever the caches refresh, so it always
/// reflects the latest imports, reviews, and language switches.
pub struct HomeData {
    /// Credited reading seconds per day, for the activity heatmap.
    pub reading_by_day: Vec<(String, f64)>,
    /// Cards due by the end of today (includes overdue).
    pub due_today: u64,
    /// Measured seconds per review card, when enough history exists.
    pub pace_seconds: Option<f64>,
    /// The book to pick back up, if one is in progress.
    pub cont: Option<shiori_app::ContinueReading>,
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
    /// Decoded parse of *this occurrence* for pre-annotated texts
    /// ("verb · present active indicative · 3rd person singular").
    pub morph: Option<String>,
    /// Known dictionary words an unknown compound splits into
    /// (Germanic packs): Kaffeemaschine → kaffee + maschine.
    pub split_parts: Option<Vec<String>>,
}

/// One wrapped display line of the reader: the (sentence, token) cells laid
/// out on it and whether it opens a paragraph. Lines — not whole paragraphs —
/// are the unit of pagination, so a paragraph taller than a page flows onto
/// the next instead of spilling over the page edge.
pub struct ReaderLine {
    /// First wrapped line of a paragraph: gets a paragraph gap above it
    /// rather than a row gap (the page's first line gets neither).
    pub para_start: bool,
    /// (sentence, token) cells on this line, in reading order.
    pub cells: Vec<(usize, usize)>,
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
    /// The explanation is open in the centered modal for full-width reading.
    pub explanation_modal: bool,
    /// Set the frame the modal opens, so the opening click is not read as a
    /// click-away that immediately closes it.
    pub explanation_modal_just_opened: bool,
    /// Sentence-index ranges forming each paragraph, in order.
    pub para_ranges: Vec<(usize, usize)>,
    /// Wrapped display lines for the whole document at the current layout
    /// width. Rebuilt lazily whenever the layout size, font, or zoom changes.
    pub lines: Vec<ReaderLine>,
    /// First `lines` index containing each sentence (drives position restore
    /// and selection-follow across pages).
    pub line_of_sentence: Vec<usize>,
    /// Page boundaries as indices into `lines`; page `i` covers
    /// `lines[page_line_starts[i]..page_line_starts[i+1]]`.
    pub page_line_starts: Vec<usize>,
    pub current_page: usize,
    /// (width, height) the pages were computed for.
    pub page_layout: (f32, f32),
    /// Rendered width of every token (lazy; the line breaker measures the
    /// whole-token wrap with these).
    pub token_widths: Vec<Vec<f32>>,
    /// `pixels_per_point` the token widths were measured at. egui's global
    /// zoom changes it, shifting every token's pixel-rounded width, so the
    /// cache is re-measured and the pages re-flowed when it moves.
    pub layout_ppp: f32,
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
        self.page_line_starts.len().max(1)
    }

    /// Page containing the given display line.
    pub fn page_of_line(&self, line: usize) -> usize {
        self.page_line_starts
            .iter()
            .rposition(|&l| l <= line)
            .unwrap_or_default()
    }

    /// Page that first shows the given sentence.
    pub fn page_of_sentence(&self, sentence: usize) -> usize {
        let line = self.line_of_sentence.get(sentence).copied().unwrap_or(0);
        self.page_of_line(line)
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
fn compute_groups(
    service: &dyn shiori_lang::LanguageService,
    sentences: &[SentenceView],
) -> Vec<Vec<(usize, usize)>> {
    sentences
        .iter()
        .map(|view| {
            let tokens: Vec<shiori_core::Token> =
                view.tokens.iter().map(|r| r.token.clone()).collect();
            service.phrase_groups(&tokens)
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
    /// Active language implementation, shared out of the app so views
    /// can call it per-token without taking the app lock.
    pub lang: Option<Arc<dyn shiori_lang::LanguageService>>,
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
    /// Home-page aggregates; `None` = stale, recomputed on next show.
    pub home: Option<HomeData>,
    /// Cached language registry for per-frame UI (language combos and
    /// the Languages settings page) — reading it live would take the
    /// app lock and scan pack directories every frame.
    pub lang_infos: Vec<shiori_app::LanguageInfo>,

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
    /// URL typed into the pack-download box (Settings → Languages).
    pub pack_url_input: String,
    /// Optional SHA-256 typed next to the pack URL.
    pub pack_sha_input: String,
    /// Language code awaiting removal confirmation, with its name.
    pub pack_remove_confirm: Option<(String, String)>,
    /// A pack install/download is running in the background.
    pub pack_installing: bool,
    /// Latest status line of the running pack job (web builds).
    pub pack_job_status: Option<String>,
    /// Filter box over the build-from-Wiktionary language list.
    pub web_pack_filter: String,
    /// Browsable hosted pack catalog, once fetched.
    pub pack_catalog: Option<Vec<shiori_app::PackCatalogEntry>>,
    pub pack_catalog_loading: bool,
    /// Why the catalog is unavailable, when it is.
    pub pack_catalog_error: Option<String>,
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

        // Start larger than egui's 1.0 baseline. The user can still zoom
        // up or down from here with Ctrl+Plus/Minus; we don't persist egui
        // memory, so each launch returns to this default.
        cc.egui_ctx.set_zoom_factor(DEFAULT_ZOOM_FACTOR);

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
            lang: None,
            explainer: settings.build_explainer(),
            phase: Phase::Starting,
            progress: Vec::new(),
            error: None,
            notice: None,
            import_jobs: 0,
            view: if settings.onboarded {
                View::Home
            } else {
                View::Welcome
            },
            library: Vec::new(),
            doc_stats: HashMap::new(),
            due_count: 0,
            home: None,
            lang_infos: Vec::new(),
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
            pack_url_input: String::new(),
            pack_sha_input: String::new(),
            pack_remove_confirm: None,
            pack_installing: false,
            pack_job_status: None,
            web_pack_filter: String::new(),
            pack_catalog: None,
            pack_catalog_loading: false,
            pack_catalog_error: None,
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
            // Solid scroll bars reserve a gutter at the edge, so content wraps
            // clear of them instead of sliding under a floating overlay bar.
            ctx.style_mut(|style| {
                style.spacing.scroll = egui::style::ScrollStyle::solid();
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
            let sentence = reader
                .page_line_starts
                .get(reader.current_page)
                .and_then(|&l| reader.lines.get(l))
                .and_then(|line| line.cells.first())
                .map(|&(s, _)| s)
                .unwrap_or(0);
            reader.token_widths.clear();
            reader.lines.clear();
            reader.line_of_sentence.clear();
            reader.page_line_starts.clear();
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
        self.view = self.welcome_return.take().unwrap_or(View::Home);
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

    /// Run a closure against the app mutably (language switching).
    pub fn with_app_mut<T>(
        &mut self,
        f: impl FnOnce(&mut App) -> Result<T, shiori_app::AppError>,
    ) -> Option<T> {
        let app = self.app.clone()?;
        let mut guard = match app.lock() {
            Ok(g) => g,
            Err(_) => {
                self.error = Some("internal error: app lock poisoned".into());
                return None;
            }
        };
        match f(&mut guard) {
            Ok(v) => Some(v),
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }

    /// Switch the active language: everything scoped — library, stats,
    /// dictionary, chat — re-reads under the new language.
    pub fn switch_language(&mut self, code: &str) {
        let code = code.to_string();
        if self
            .with_app_mut(|app| app.set_active_lang(&code))
            .is_none()
        {
            return;
        }
        self.settings.active_language = code.clone();
        self.settings_draft.active_language = code;
        let _ = self.settings.save(&self.data_dir);
        self.lang = self.with_app(|app| Ok(app.lang_service()));
        // The new language may pin its own model (a local model fine for
        // Japanese can be hopeless at Koine).
        self.explainer = self.settings.build_explainer();
        // Language-scoped view state resets; the new language's data
        // loads through the usual caches.
        self.reader = None;
        self.book_info = None;
        self.sweep = None;
        self.dictionary = DictionaryState::default();
        self.production = ProductionState::default();
        self.review = ReviewState::default();
        self.data_status = self.with_app(|app| app.data_status());
        self.phase = match &self.data_status {
            Some(s) if s.is_ready() => Phase::Ready,
            _ => Phase::NeedsData,
        };
        self.refresh_caches();
        self.load_conversations();
    }

    /// Refresh the library/stat caches from the database (active
    /// language only).
    pub fn refresh_caches(&mut self) {
        if let Some(recs) = self.with_app(|app| {
            let mut stats = HashMap::new();
            let mut docs = app.db().list_documents()?;
            docs.retain(|d| d.document.lang == app.active_lang());
            for d in &docs {
                stats.insert(d.document.id.0, app.document_stats(d.document.id)?);
            }
            let due = app.due_count()?;
            Ok((docs, stats, due))
        }) {
            (self.library, self.doc_stats, self.due_count) = recs;
        }
        self.lang_infos = self
            .with_app(|app| Ok(app.language_infos()))
            .unwrap_or_default();
        // Anything that moves these caches also moves the home page.
        self.home = None;
    }

    /// Recompute the home-page aggregates. Always fills `home` — a
    /// failed query shows an empty page (with its error toast) instead
    /// of being retried every frame.
    pub fn refresh_home(&mut self) {
        let data = self.with_app(|app| {
            Ok(HomeData {
                reading_by_day: app.db().reading_seconds_by_day(app.active_lang())?,
                due_today: app.due_today()?,
                pace_seconds: app.review_pace_seconds()?,
                cont: app.continue_reading()?,
            })
        });
        self.home = Some(data.unwrap_or(HomeData {
            reading_by_day: Vec::new(),
            due_today: 0,
            pace_seconds: None,
            cont: None,
        }));
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::AppOpened(Ok(app)) => {
                    let mut app = app;
                    // Restore the persisted language before anything
                    // reads through the service.
                    let desired = self.settings.active_language.clone();
                    if !desired.is_empty() && desired != app.active_lang() {
                        if let Err(e) = app.set_active_lang(&desired) {
                            self.error =
                                Some(format!("could not activate language '{desired}': {e}"));
                        }
                    }
                    let status = app.data_status().ok();
                    self.lang = Some(app.lang_service());
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
                Msg::PackCatalog(result) => {
                    self.pack_catalog_loading = false;
                    match result {
                        Ok(packs) => {
                            self.pack_catalog = Some(packs);
                            self.pack_catalog_error = None;
                        }
                        // Not an error toast: the browse section itself
                        // explains and offers a retry, and a previously
                        // fetched list keeps showing.
                        Err(e) => self.pack_catalog_error = Some(e),
                    }
                }
                Msg::TransferDone(result) => match result {
                    Ok(summary) => {
                        self.notice = Some(summary);
                        self.refresh_caches();
                    }
                    Err(e) => self.error = Some(e),
                },
                Msg::PackJobDone(result) => {
                    self.pack_installing = false;
                    self.pack_job_status = None;
                    match result {
                        Ok(summary) => {
                            self.notice = Some(summary);
                            self.refresh_caches();
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::PackJobProgress(line) => {
                    self.pack_job_status = Some(line);
                }
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
            let groups = compute_groups(app.lang_service().as_ref(), &sentences);
            let (para_ranges, _) = paragraph_structure(&sentences);
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
                explanation_modal: false,
                explanation_modal_just_opened: false,
                para_ranges,
                lines: Vec::new(),
                line_of_sentence: Vec::new(),
                page_line_starts: Vec::new(),
                current_page: 0,
                page_layout: (0.0, 0.0),
                token_widths: Vec::new(),
                layout_ppp: 1.0,
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
        if let (Some(reader), Some(sentences), Some(lang)) =
            (self.reader.as_mut(), refreshed, self.lang.as_ref())
        {
            reader.groups = compute_groups(lang.as_ref(), &sentences);
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
            // Frequency lists key by the language's lookup forms (folded
            // lemma for packs; lemma-then-reading for Japanese).
            let mut rank = None;
            for form in app
                .lang_service()
                .frequency_forms(&word.key.lemma, &word.key.reading)
            {
                rank = app.db().frequency_rank(app.active_lang(), &form)?;
                if rank.is_some() {
                    break;
                }
            }
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
                morph: None,
                split_parts: None,
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
        if reader.page_line_starts.is_empty() {
            // Not laid out yet; nothing meaningful to save.
            return;
        }
        let page = reader.current_page.min(reader.page_count() - 1);
        let s0 = if page + 1 == reader.page_count() {
            reader.sentences.len() as u32
        } else {
            let line = reader.page_line_starts.get(page).copied().unwrap_or(0);
            let Some(&(s0, _)) = reader.lines.get(line).and_then(|l| l.cells.first()) else {
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

    /// Run a pack install/removal against the app mutably on a worker
    /// thread. Owns the busy flag: refuses to start while another pack
    /// job runs, and only [`Msg::PackJobDone`] clears it.
    pub fn run_pack_job<F>(&mut self, ctx: &egui::Context, job: F)
    where
        F: FnOnce(&mut App) -> Result<String, shiori_app::AppError> + Send + 'static,
    {
        if self.pack_installing {
            return;
        }
        let Some(app) = self.app.clone() else { return };
        self.pack_installing = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = match app.lock() {
                Ok(mut guard) => job(&mut guard).map_err(|e| e.to_string()),
                Err(_) => Err("app lock poisoned".into()),
            };
            let _ = tx.send(Msg::PackJobDone(result));
            ctx.request_repaint();
        });
    }

    /// Download and install a language pack in the background. The
    /// download itself runs without the app lock, so a slow network
    /// never freezes the interface; only the quick unpack-and-register
    /// step locks the app.
    pub fn start_pack_download(&mut self, ctx: &egui::Context, url: String, sha256: String) {
        if self.pack_installing {
            return;
        }
        let Some(app) = self.app.clone() else { return };
        self.pack_installing = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let expected = (!sha256.trim().is_empty()).then_some(sha256.as_str());
            let result = shiori_app::download_pack_zip(&url, expected)
                .map_err(|e| e.to_string())
                .and_then(|bytes| match app.lock() {
                    Ok(mut guard) => guard
                        .install_pack_from_zip_bytes(&bytes)
                        .map(|lang| format!("language pack '{lang}' downloaded and installed"))
                        .map_err(|e| e.to_string()),
                    Err(_) => Err("app lock poisoned".into()),
                });
            let _ = tx.send(Msg::PackJobDone(result));
            ctx.request_repaint();
        });
    }

    /// Build a language pack from public web sources (kaikki.org +
    /// hermitdave) in the background: download and build run without
    /// the app lock; only the final install step locks. Progress lands
    /// in [`Msg::PackJobProgress`].
    pub fn start_web_pack_build(&mut self, ctx: &egui::Context, lang: String) {
        if self.pack_installing {
            return;
        }
        let Some(source) = shiori_app::web_pack_source(&lang).copied() else {
            return;
        };
        let Some(app) = self.app.clone() else { return };
        self.pack_installing = true;
        self.pack_job_status = Some(format!("preparing {}…", source.name));
        let data_dir = self.data_dir.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let progress_ctx = ctx.clone();
            let mut on_progress = move |line: &str| {
                let _ = progress_tx.send(Msg::PackJobProgress(line.to_string()));
                progress_ctx.request_repaint();
            };
            let staging = data_dir.join(format!(".pack-build-{}", std::process::id()));
            let result = (|| {
                let (kaikki, freq) =
                    shiori_app::download_web_pack_inputs(&data_dir, &source, &mut on_progress)
                        .map_err(|e| e.to_string())?;
                let report = shiori_app::build_web_pack(
                    &source,
                    &kaikki,
                    freq.as_deref(),
                    &staging,
                    &mut on_progress,
                )
                .map_err(|e| e.to_string())?;
                on_progress("installing…");
                match app.lock() {
                    Ok(mut guard) => {
                        guard
                            .install_pack_from_dir(&staging)
                            .map_err(|e| e.to_string())?;
                    }
                    Err(_) => return Err("app lock poisoned".to_string()),
                }
                // Reclaim the gigabyte-class download once the pack is in;
                // a failed run keeps it so a retry needn't re-download.
                std::fs::remove_file(&kaikki).ok();
                let mut summary = format!(
                    "{} pack built and installed: {} words, {} inflected forms",
                    source.name, report.entries, report.forms
                );
                if report.frequency > 0 {
                    summary.push_str(&format!(", {}-word frequency list", report.frequency));
                }
                Ok(summary)
            })();
            std::fs::remove_dir_all(&staging).ok();
            let _ = tx.send(Msg::PackJobDone(result));
            ctx.request_repaint();
        });
    }

    /// Fetch the browsable pack catalog in the background (no app lock:
    /// only the data directory and the configured URL are needed).
    /// `force` re-downloads; otherwise a cached copy serves.
    ///
    /// Currently dormant: the Browse-packs UI was removed until a
    /// hosted catalog exists (build-from-Wiktionary covers discovery).
    /// The full pipeline — this loader, [`Msg::PackCatalog`], the
    /// app-layer fetch/parse, and `shiori-packc catalog` — stays wired
    /// and tested so the section can come back with one UI call.
    #[allow(dead_code)]
    pub fn start_pack_catalog_load(&mut self, ctx: &egui::Context, force: bool) {
        if self.pack_catalog_loading {
            return;
        }
        self.pack_catalog_loading = true;
        let data_dir = self.data_dir.clone();
        let url = self.settings_draft.pack_catalog_url.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                shiori_app::fetch_pack_catalog(&data_dir, &url, force).map_err(|e| e.to_string());
            let _ = tx.send(Msg::PackCatalog(result));
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
        let profile = self.lang.as_ref().map(|l| l.prompt_profile().clone());
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut context = shiori_llm::SentenceContext::new(sentence);
            if let Some(profile) = profile {
                context = context.with_profile(profile);
            }
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
        if let Some(list) =
            self.with_app(|app| Ok(app.db().list_conversations(app.active_lang())?))
        {
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
                    Ok(app.db().create_conversation(
                        app.active_lang(),
                        chrono::Utc::now(),
                        &title,
                    )?)
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
        let profile = self
            .lang
            .as_ref()
            .map(|l| l.prompt_profile().clone())
            .unwrap_or_else(shiori_llm::PromptProfile::japanese);
        let system = shiori_llm::chat_system_prompt(
            &profile,
            &level_hint,
            self.settings.chat_challenge.to_llm(),
        );

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
        // Redirect the built-in zoom-reset shortcut (Ctrl/Cmd+0) to our
        // larger default instead of egui's hard-coded 1.0. Consuming it
        // here, before egui's end-of-pass zoom handler runs, stops that
        // handler from also firing and overriding us. Ctrl+Plus/Minus keep
        // egui's default behavior.
        let zoom_reset = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Num0);
        if ctx.input_mut(|i| i.consume_shortcut(&zoom_reset)) {
            ctx.set_zoom_factor(DEFAULT_ZOOM_FACTOR);
        }

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
            View::Home => self.show_home(ctx),
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
                    if item(ui, self.view == View::Home, "🏠", "Home".into(), true) {
                        nav = Some(View::Home);
                    }
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
                // Due counts and reading progress move while away, so
                // returning to the home page re-reads everything.
                View::Home => {
                    self.refresh_caches();
                    self.view = view;
                }
                // Picks up packs dropped into the data dir by hand while
                // the app runs (the page itself reads the cached list).
                View::Settings => {
                    self.refresh_caches();
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
