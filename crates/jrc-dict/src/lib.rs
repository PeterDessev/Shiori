//! JMdict dictionary and frequency list: download, parsing, lookup.
//!
//! Dictionary data comes from the
//! [jmdict-simplified](https://github.com/scriptin/jmdict-simplified) JSON
//! distribution of JMdict (© EDRDG, used under the EDRDG licence). Word
//! frequency ranks come from a Leeds-corpus-derived list. Both are fetched
//! on first run by [`download`]; everything else in the crate is pure.

pub mod download;
pub mod frequency;
pub mod kanji;
mod lookup;
pub mod register;
mod types;

pub use frequency::FrequencyList;
pub use kanji::KanjiEntry;
pub use lookup::{pick_best_entry, Dictionary};
pub use types::{DictEntry, Form, JmdictFile, Sense};

/// Errors produced while acquiring or reading dictionary data.
#[derive(Debug, thiserror::Error)]
pub enum DictError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("network error: {0}")]
    Network(String),

    #[error("malformed dictionary data: {0}")]
    Parse(String),

    #[error("no suitable asset found in the latest jmdict-simplified release")]
    NoAsset,
}

impl From<serde_json::Error> for DictError {
    fn from(e: serde_json::Error) -> Self {
        DictError::Parse(e.to_string())
    }
}

impl From<ureq::Error> for DictError {
    fn from(e: ureq::Error) -> Self {
        DictError::Network(e.to_string())
    }
}
