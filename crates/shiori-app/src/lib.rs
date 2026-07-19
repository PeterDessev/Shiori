//! Application services: everything the GUI needs, with no GUI in it.
//!
//! [`App`] owns the database, the morphological analyzer, and the SRS
//! scheduler, and exposes the use-cases of the program: importing text,
//! mining vocabulary, reviewing cards, and computing reading-difficulty
//! statistics.

mod chat;
mod data;
mod dictionary;
pub mod extract;
mod finish;
mod home;
mod ingest;
mod mining;
mod packs;
mod review;
mod sessions;
mod sources;
mod stats;
mod transfer;
mod web_packs;

pub use chat::{ChatSentence, ChatTokenRow};
pub use data::DataStatus;
pub use dictionary::{DictExample, DictSearchHit, DictSearchResults, QueryAnalysis};
pub use finish::{SweepCandidate, SweepPlan};
pub use home::ContinueReading;
pub use mining::MiningCandidate;
pub use packs::{
    download_pack_zip, fetch_pack_catalog, parse_pack_catalog, LanguageInfo, PackCatalogEntry,
    PackDetails, DEFAULT_PACK_CATALOG_URL,
};
pub use review::ReviewItem;
pub use sources::{AozoraWork, WikisourceHit};
pub use stats::{DifficultyBand, DocStats, Recommendation, StatsOverview};
pub use web_packs::{
    build_web_pack, download_web_pack_inputs, web_pack_source, WebPackSource, WEB_PACK_SOURCES,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use shiori_db::Db;
use shiori_lang::LanguageService;
use shiori_srs::Scheduler;

/// Errors surfaced by application services.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Nlp(#[from] shiori_nlp::NlpError),

    #[error(transparent)]
    Lang(#[from] shiori_lang::LangError),

    #[error(transparent)]
    Dict(#[from] shiori_dict::DictError),

    #[error(transparent)]
    Db(#[from] shiori_db::DbError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// The application service layer.
pub struct App {
    db: Db,
    /// Installed language implementations, by language code.
    services: HashMap<String, Arc<dyn LanguageService>>,
    /// Discovered language packs, by language code.
    packs: HashMap<String, shiori_pack::Pack>,
    /// Per-language suffix rewrite rules (loaded from a pack's
    /// `suffix_rules.tsv` on activation) for guessing lemmas of
    /// regular inflections missing from the full-form table.
    suffix_rules: HashMap<String, Vec<(String, String)>>,
    /// Language the whole app currently operates in.
    active: String,
    scheduler: Scheduler,
    data_dir: PathBuf,
}

impl App {
    /// Database filename inside the data directory.
    pub const DB_FILENAME: &'static str = "jrc.sqlite3";
    /// A backup staged for restore; swapped in on the next startup
    /// (the live database file can't be replaced while open).
    pub const RESTORE_PENDING_FILENAME: &'static str = "jrc.sqlite3.restore-pending";

    /// Open the application with its data directory (created if missing).
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join(Self::DB_FILENAME);
        // Complete a staged restore before the database opens.
        let pending = data_dir.join(Self::RESTORE_PENDING_FILENAME);
        if pending.exists() {
            if db_path.exists() {
                let aside = data_dir.join("jrc.sqlite3.pre-restore");
                std::fs::remove_file(&aside).ok();
                std::fs::rename(&db_path, &aside)?;
                // WAL/SHM of the old database must not leak into the new one.
                std::fs::remove_file(data_dir.join("jrc.sqlite3-wal")).ok();
                std::fs::remove_file(data_dir.join("jrc.sqlite3-shm")).ok();
            }
            std::fs::rename(&pending, &db_path)?;
        }
        let db = Db::open(&db_path)?;
        Self::with_db(db, data_dir.to_path_buf())
    }

    /// Stage a backup file to replace the database on next launch.
    pub fn stage_restore(&self, backup: &Path) -> Result<()> {
        std::fs::copy(backup, self.data_dir.join(Self::RESTORE_PENDING_FILENAME))?;
        Ok(())
    }

    /// Open over an existing database handle (tests use an in-memory one).
    pub fn with_db(db: Db, data_dir: PathBuf) -> Result<Self> {
        let japanese: Arc<dyn LanguageService> = Arc::new(shiori_nlp::Japanese::new()?);
        let mut services: HashMap<String, Arc<dyn LanguageService>> = HashMap::new();
        services.insert(japanese.lang().to_string(), japanese);

        // Clean up anything an interrupted pack install/build/removal
        // left behind before discovery can trip over it.
        packs::sweep_pack_leftovers(&data_dir);

        // Every pack under <data_dir>/packs/ becomes a language, no
        // recompile needed.
        let mut packs = HashMap::new();
        for pack in shiori_pack::discover_packs(&data_dir) {
            let lang = pack.manifest.lang.clone();
            services.insert(
                lang.clone(),
                Arc::new(shiori_pack::PackLanguage::new(&pack.manifest)),
            );
            packs.insert(lang, pack);
        }

        Ok(Self {
            db,
            services,
            packs,
            active: "ja".to_string(),
            suffix_rules: HashMap::new(),
            scheduler: Scheduler::default(),
            data_dir,
        })
    }

    /// Languages the app can operate in: (code, display name), Japanese
    /// first, then packs alphabetically.
    pub fn available_languages(&self) -> Vec<(String, String)> {
        let mut out = vec![("ja".to_string(), "Japanese".to_string())];
        let mut langs: Vec<_> = self.packs.values().collect();
        langs.sort_by(|a, b| a.manifest.lang.cmp(&b.manifest.lang));
        for pack in langs {
            out.push((pack.manifest.lang.clone(), pack.manifest.name.clone()));
        }
        out
    }

    /// Switch the active language, installing the pack's reference data
    /// on first use.
    pub fn set_active_lang(&mut self, lang: &str) -> Result<()> {
        if !self.services.contains_key(lang) {
            return Err(AppError::Invalid(format!(
                "no language '{lang}' is installed"
            )));
        }
        self.ensure_pack_data(lang)?;
        // Suffix rules live in memory, loaded once per pack activation.
        if let Some(pack) = self.packs.get(lang) {
            if !self.suffix_rules.contains_key(lang) {
                let rules = packs::load_suffix_rules(&pack.dir.join("suffix_rules.tsv"));
                self.suffix_rules.insert(lang.to_string(), rules);
            }
        }
        self.active = lang.to_string();
        Ok(())
    }

    /// The active language's pack, when it is pack-driven.
    pub fn active_pack(&self) -> Option<&shiori_pack::Pack> {
        self.packs.get(&self.active)
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Language the whole app currently operates in; every
    /// language-scoped database call routes through this.
    pub fn active_lang(&self) -> &str {
        &self.active
    }

    /// Dictionary source backing the active language.
    pub fn active_dict_source(&self) -> &str {
        self.service().dict_source()
    }

    /// The active language implementation.
    pub(crate) fn service(&self) -> &dyn LanguageService {
        self.services
            .get(&self.active)
            .expect("active language always has a service")
            .as_ref()
    }

    /// Shared handle to the active language implementation, for callers
    /// (the GUI) that need it without holding the app lock.
    pub fn lang_service(&self) -> Arc<dyn LanguageService> {
        Arc::clone(
            self.services
                .get(&self.active)
                .expect("active language always has a service"),
        )
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
