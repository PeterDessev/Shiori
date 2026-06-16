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
mod ingest;
mod mining;
mod review;
mod sessions;
mod sources;
mod stats;
mod transfer;

pub use chat::{ChatSentence, ChatTokenRow};
pub use data::DataStatus;
pub use dictionary::{DictExample, DictSearchHit, DictSearchResults, QueryAnalysis};
pub use finish::{SweepCandidate, SweepPlan};
pub use mining::MiningCandidate;
pub use review::ReviewItem;
pub use sources::{AozoraWork, WikisourceHit};
pub use stats::{DifficultyBand, DocStats, Recommendation, StatsOverview};

use std::path::{Path, PathBuf};

use shiori_db::Db;
use shiori_nlp::Analyzer;
use shiori_srs::Scheduler;

/// Errors surfaced by application services.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Nlp(#[from] shiori_nlp::NlpError),

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
    analyzer: Analyzer,
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
        Ok(Self {
            db,
            analyzer: Analyzer::new()?,
            scheduler: Scheduler::default(),
            data_dir,
        })
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    pub fn analyzer(&self) -> &Analyzer {
        &self.analyzer
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}
