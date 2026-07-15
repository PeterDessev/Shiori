//! Text ingestion: analyze and store documents.

use chrono::Utc;
use shiori_core::{DocumentId, DocumentMeta};
use shiori_db::{NewSentence, NewToken};

use crate::{App, AppError, Result};

impl App {
    /// Analyze `text` and store it as a document titled `title`.
    ///
    /// Re-importing identical content returns the existing document instead
    /// of duplicating it.
    pub fn import_text(&self, title: &str, text: &str) -> Result<DocumentId> {
        self.import_text_meta(DocumentMeta::titled(title), text)
    }

    /// Like [`import_text`](Self::import_text), with full metadata.
    pub fn import_text_meta(&self, mut meta: DocumentMeta, text: &str) -> Result<DocumentId> {
        meta.title = meta.title.trim().to_string();
        if meta.title.is_empty() {
            return Err(AppError::Invalid("document title must not be empty".into()));
        }
        let hash = content_hash(text);
        if let Some(existing) = self.db.find_document_by_hash(self.active_lang(), &hash)? {
            return Ok(existing);
        }

        let analyzed = self.analyzer.analyze(text)?;
        let mut sentences = Vec::new();
        for (p_idx, paragraph) in analyzed.paragraphs.iter().enumerate() {
            for sentence in &paragraph.sentences {
                sentences.push(NewSentence {
                    paragraph: p_idx as u32,
                    text: sentence.text.clone(),
                    tokens: sentence
                        .tokens
                        .iter()
                        .map(|t| NewToken {
                            surface: t.surface.clone(),
                            lemma: t.lemma.clone(),
                            reading: t.reading.clone(),
                            pos: t.pos,
                            start: t.start,
                            end: t.end,
                            morph: None,
                            gloss: None,
                        })
                        .collect(),
                });
            }
        }
        if sentences.is_empty() {
            return Err(AppError::Invalid(
                "no Japanese sentences found in the text".into(),
            ));
        }

        Ok(self
            .db
            .import_document(self.active_lang(), &meta, &hash, Utc::now(), &sentences)?)
    }

    /// Import a file from disk (txt/md, HTML, EPUB, or PDF — see
    /// [`crate::extract`]), auto-extracting metadata where the format
    /// provides it and falling back to the file stem for the title.
    ///
    /// The original file is copied into `<data_dir>/books/` so the library
    /// survives the source being moved or deleted.
    pub fn import_file(&self, path: &std::path::Path) -> Result<DocumentId> {
        let extracted = crate::extract::extract_document(path)?;
        let mut meta = extracted.meta;
        if meta.title.trim().is_empty() {
            meta.title = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string());
        }
        let id = self.import_text_meta(meta, &extracted.text)?;

        // Best-effort archival copy; the text itself is already in the
        // database, so a failed copy must not fail the import.
        let books = self.data_dir().join("books");
        if std::fs::create_dir_all(&books).is_ok() {
            if let Some(name) = path.file_name() {
                let dest = books.join(format!("{}-{}", id.0, name.to_string_lossy()));
                if !dest.exists() {
                    let _ = std::fs::copy(path, &dest);
                }
            }
        }
        Ok(id)
    }
}

/// FNV-1a 64-bit content hash, hex-encoded. Deterministic across runs and
/// Rust versions (used for import dedup, not security).
fn content_hash(text: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_stable_and_distinguishes() {
        assert_eq!(content_hash("猫"), content_hash("猫"));
        assert_ne!(content_hash("猫"), content_hash("犬"));
        // Known FNV-1a 64 test vector.
        assert_eq!(content_hash(""), "cbf29ce484222325");
    }
}
