//! Data-driven language packs.
//!
//! A pack is a directory under `<data_dir>/packs/{lang}/` holding
//! everything a language needs at runtime, as data:
//!
//! - `manifest.toml` — identity, script ranges, segmentation rules,
//!   prompt/extraction profiles, level schemes, license metadata.
//! - `dictionary.jsonl` — one entry per line: `{"key": …, "entry": …}`
//!   where `entry` is a jmdict-simplified-shaped word object (the shape
//!   the whole app already renders).
//! - `frequency.tsv` — `form<TAB>rank` per line.
//! - `tags.tsv` — `code<TAB>label` per line, decoding parse codes
//!   (`V` → "verb", `PAI` → "present active indicative").
//! - `graded.tsv` — `level_ord<TAB>level_label<TAB>form<TAB>alt_form`.
//! - `texts/*.siat.jsonl` — pre-annotated texts (see [`siat`]).
//!
//! [`PackLanguage`] implements `LanguageService` over this data: for
//! dead languages the primary reading path is pre-annotated texts (no
//! runtime analyzer at all); plain-text imports fall back to a
//! rule-based tokenizer driven by the manifest.

pub mod betacode;
pub mod catalog;
pub mod kaikki;
mod language;
pub mod manifest;
pub mod siat;

pub use catalog::is_safe_lang_code;
pub use language::{fold_lookup, normalize_nfc, PackLanguage};
pub use manifest::Manifest;

use std::path::{Path, PathBuf};

/// Errors from pack loading and parsing.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("pack I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bad pack manifest: {0}")]
    Manifest(String),

    #[error("bad SIAT file: {0}")]
    Siat(String),

    #[error("pack data error: {0}")]
    Data(String),
}

pub type Result<T, E = PackError> = std::result::Result<T, E>;

/// A discovered pack: its directory and parsed manifest.
#[derive(Debug, Clone)]
pub struct Pack {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Pack {
    /// Load the pack rooted at `dir` (must contain `manifest.toml`).
    pub fn load(dir: &Path) -> Result<Self> {
        let manifest = Manifest::load(&dir.join("manifest.toml"))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            manifest,
        })
    }

    pub fn dictionary_path(&self) -> PathBuf {
        self.dir.join("dictionary.jsonl")
    }

    pub fn frequency_path(&self) -> PathBuf {
        self.dir.join("frequency.tsv")
    }

    pub fn tags_path(&self) -> PathBuf {
        self.dir.join("tags.tsv")
    }

    pub fn graded_path(&self) -> PathBuf {
        self.dir.join("graded.tsv")
    }

    /// Bundled pre-annotated texts, sorted by filename for a stable
    /// shelf order.
    pub fn text_paths(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.dir.join("texts")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with(".siat.jsonl"))
                {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }
}

/// Discover every pack under `<data_dir>/packs/`.
pub fn discover_packs(data_dir: &Path) -> Vec<Pack> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(data_dir.join("packs")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() && dir.join("manifest.toml").exists() {
                match Pack::load(&dir) {
                    Ok(pack) => out.push(pack),
                    Err(e) => eprintln!("skipping pack {}: {e}", dir.display()),
                }
            }
        }
    }
    out.sort_by(|a, b| a.manifest.lang.cmp(&b.manifest.lang));
    out
}
