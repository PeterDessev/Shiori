//! Text ingestion: analyze and store documents.

use chrono::Utc;
use jrc_core::DocumentId;
use jrc_db::{NewSentence, NewToken};

use crate::{App, AppError, Result};

impl App {
    /// Analyze `text` and store it as a document titled `title`.
    ///
    /// Re-importing identical content returns the existing document instead
    /// of duplicating it.
    pub fn import_text(&self, title: &str, text: &str) -> Result<DocumentId> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::Invalid("document title must not be empty".into()));
        }
        let hash = content_hash(text);
        if let Some(existing) = self.db.find_document_by_hash(&hash)? {
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

        Ok(self.db.import_document(title, &hash, Utc::now(), &sentences)?)
    }

    /// Import a text file (UTF-8) from disk, using the file stem as title.
    pub fn import_file(&self, path: &std::path::Path) -> Result<DocumentId> {
        let text = std::fs::read_to_string(path)?;
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        self.import_text(&title, &text)
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
