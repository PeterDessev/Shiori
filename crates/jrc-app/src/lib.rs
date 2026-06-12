//! Application services: everything the GUI needs, with no GUI in it.
//!
//! [`App`] owns the database, the morphological analyzer, and the SRS
//! scheduler, and exposes the use-cases of the program: importing text,
//! mining vocabulary, reviewing cards, and computing reading-difficulty
//! statistics.

mod chat;
mod data;
pub mod extract;
mod finish;
mod ingest;
mod mining;
mod review;
mod sessions;
mod stats;

pub use chat::ChatTokenRow;
pub use data::DataStatus;
pub use finish::{SweepCandidate, SweepPlan};
pub use mining::MiningCandidate;
pub use review::ReviewItem;
pub use stats::{DifficultyBand, DocStats, Recommendation};

use std::path::{Path, PathBuf};

use jrc_db::Db;
use jrc_nlp::Analyzer;
use jrc_srs::Scheduler;

/// Errors surfaced by application services.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Nlp(#[from] jrc_nlp::NlpError),

    #[error(transparent)]
    Dict(#[from] jrc_dict::DictError),

    #[error(transparent)]
    Db(#[from] jrc_db::DbError),

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

    /// Open the application with its data directory (created if missing).
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db = Db::open(&data_dir.join(Self::DB_FILENAME))?;
        Self::with_db(db, data_dir.to_path_buf())
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
