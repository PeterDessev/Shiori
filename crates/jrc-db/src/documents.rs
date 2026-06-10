//! Document, sentence, and token storage.

use chrono::{DateTime, Utc};
use jrc_core::{
    Document, DocumentId, DocumentMeta, KnowledgeStatus, PartOfSpeech, Sentence, SentenceId,
    Token, WordId,
};
use rusqlite::params;

use crate::{Db, DbError, Result};

/// A token as produced by the analyzer, ready for import.
#[derive(Debug, Clone)]
pub struct NewToken {
    pub surface: String,
    pub lemma: String,
    pub reading: String,
    pub pos: PartOfSpeech,
    pub start: usize,
    pub end: usize,
}

/// A sentence ready for import.
#[derive(Debug, Clone)]
pub struct NewSentence {
    pub paragraph: u32,
    pub text: String,
    pub tokens: Vec<NewToken>,
}

/// A document row plus aggregate counts for library display.
#[derive(Debug, Clone)]
pub struct DocumentSummary {
    pub document: Document,
    pub sentence_count: u32,
    pub token_count: u32,
}

/// A stored token joined with its word's identity and knowledge status.
#[derive(Debug, Clone)]
pub struct TokenRow {
    pub token: Token,
    pub word_id: WordId,
    pub status: KnowledgeStatus,
}

fn row_to_document(r: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: DocumentId(r.get(0)?),
        title: r.get(1)?,
        author: r.get(2)?,
        publisher: r.get(3)?,
        published: r.get(4)?,
        added_at: r.get(5)?,
    })
}

impl Db {
    /// Id of the document with this content hash, if already imported.
    pub fn find_document_by_hash(&self, hash: &str) -> Result<Option<DocumentId>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id FROM documents WHERE content_hash = ?1")?;
        let id = stmt
            .query_row([hash], |r| r.get::<_, i64>(0))
            .map(DocumentId);
        match id {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Import a fully analyzed document in one transaction.
    ///
    /// Words are upserted by their (lemma, reading, pos) identity; tokens
    /// reference them.
    pub fn import_document(
        &self,
        meta: &DocumentMeta,
        content_hash: &str,
        added_at: DateTime<Utc>,
        sentences: &[NewSentence],
    ) -> Result<DocumentId> {
        let tx = self.conn().unchecked_transaction()?;

        tx.execute(
            "INSERT INTO documents(title, author, publisher, published, added_at, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                meta.title,
                meta.author,
                meta.publisher,
                meta.published,
                added_at,
                content_hash
            ],
        )?;
        let doc_id = tx.last_insert_rowid();

        {
            let mut insert_sentence = tx.prepare(
                "INSERT INTO sentences(document_id, idx, paragraph, text)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut find_word = tx.prepare(
                "SELECT id FROM words WHERE lemma = ?1 AND reading = ?2 AND pos = ?3",
            )?;
            let mut insert_word = tx.prepare(
                "INSERT INTO words(lemma, reading, pos) VALUES (?1, ?2, ?3)",
            )?;
            let mut insert_token = tx.prepare(
                "INSERT INTO tokens(sentence_id, idx, word_id, surface, start, end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;

            for (idx, sentence) in sentences.iter().enumerate() {
                insert_sentence.execute(params![
                    doc_id,
                    idx as i64,
                    sentence.paragraph as i64,
                    sentence.text
                ])?;
                let sentence_id = tx.last_insert_rowid();

                for (t_idx, token) in sentence.tokens.iter().enumerate() {
                    let pos = token.pos.as_str();
                    let word_id: i64 = match find_word
                        .query_row(params![token.lemma, token.reading, pos], |r| r.get(0))
                    {
                        Ok(id) => id,
                        Err(rusqlite::Error::QueryReturnedNoRows) => {
                            insert_word.execute(params![token.lemma, token.reading, pos])?;
                            tx.last_insert_rowid()
                        }
                        Err(e) => return Err(e.into()),
                    };
                    insert_token.execute(params![
                        sentence_id,
                        t_idx as i64,
                        word_id,
                        token.surface,
                        token.start as i64,
                        token.end as i64
                    ])?;
                }
            }
        }

        tx.commit()?;
        Ok(DocumentId(doc_id))
    }

    pub fn document(&self, id: DocumentId) -> Result<Document> {
        self.conn()
            .query_row(
                "SELECT id, title, author, publisher, published, added_at
                 FROM documents WHERE id = ?1",
                [id.0],
                row_to_document,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound("document"),
                e => e.into(),
            })
    }

    /// All documents, newest first, with sentence/token counts.
    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>> {
        let mut stmt = self.conn().prepare(
            "SELECT d.id, d.title, d.author, d.publisher, d.published, d.added_at,
                    (SELECT COUNT(*) FROM sentences s WHERE s.document_id = d.id),
                    (SELECT COUNT(*) FROM tokens t
                       JOIN sentences s ON s.id = t.sentence_id
                      WHERE s.document_id = d.id)
             FROM documents d
             ORDER BY d.added_at DESC, d.id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DocumentSummary {
                document: row_to_document(r)?,
                sentence_count: r.get::<_, i64>(6)? as u32,
                token_count: r.get::<_, i64>(7)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn delete_document(&self, id: DocumentId) -> Result<()> {
        self.conn()
            .execute("DELETE FROM documents WHERE id = ?1", [id.0])?;
        Ok(())
    }

    /// Sentences of a document in reading order.
    pub fn sentences(&self, document: DocumentId) -> Result<Vec<Sentence>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, document_id, idx, paragraph, text
             FROM sentences WHERE document_id = ?1 ORDER BY idx",
        )?;
        let rows = stmt.query_map([document.0], |r| {
            Ok(Sentence {
                id: SentenceId(r.get(0)?),
                document_id: DocumentId(r.get(1)?),
                index: r.get::<_, i64>(2)? as u32,
                paragraph: r.get::<_, i64>(3)? as u32,
                text: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn sentence(&self, id: SentenceId) -> Result<Sentence> {
        self.conn()
            .query_row(
                "SELECT id, document_id, idx, paragraph, text FROM sentences WHERE id = ?1",
                [id.0],
                |r| {
                    Ok(Sentence {
                        id: SentenceId(r.get(0)?),
                        document_id: DocumentId(r.get(1)?),
                        index: r.get::<_, i64>(2)? as u32,
                        paragraph: r.get::<_, i64>(3)? as u32,
                        text: r.get(4)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound("sentence"),
                e => e.into(),
            })
    }

    /// Tokens of a sentence in order, joined with word status.
    pub fn sentence_tokens(&self, sentence: SentenceId) -> Result<Vec<TokenRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT t.surface, w.lemma, w.reading, w.pos, t.start, t.end, w.id, w.status
             FROM tokens t JOIN words w ON w.id = t.word_id
             WHERE t.sentence_id = ?1 ORDER BY t.idx",
        )?;
        let rows = stmt.query_map([sentence.0], |r| {
            Ok(TokenRow {
                token: Token {
                    surface: r.get(0)?,
                    lemma: r.get(1)?,
                    reading: r.get(2)?,
                    pos: PartOfSpeech::from_str_lossy(&r.get::<_, String>(3)?),
                    start: r.get::<_, i64>(4)? as usize,
                    end: r.get::<_, i64>(5)? as usize,
                },
                word_id: WordId(r.get(6)?),
                status: KnowledgeStatus::from_str_lossy(&r.get::<_, String>(7)?),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Two-sentence fixture document; 猫 appears in both sentences.
    pub(crate) fn import_fixture(db: &Db) -> DocumentId {
        let sentences = vec![
            NewSentence {
                paragraph: 0,
                text: "猫が好きだ。".into(),
                tokens: vec![
                    tok("猫", "猫", "ねこ", PartOfSpeech::Noun, 0),
                    tok("が", "が", "が", PartOfSpeech::Particle, 3),
                    tok("好き", "好き", "すき", PartOfSpeech::AdjectivalNoun, 6),
                    tok("だ", "だ", "だ", PartOfSpeech::AuxiliaryVerb, 12),
                ],
            },
            NewSentence {
                paragraph: 1,
                text: "その猫は走る。".into(),
                tokens: vec![
                    tok("その", "その", "その", PartOfSpeech::Prenominal, 0),
                    tok("猫", "猫", "ねこ", PartOfSpeech::Noun, 6),
                    tok("は", "は", "は", PartOfSpeech::Particle, 9),
                    tok("走る", "走る", "はしる", PartOfSpeech::Verb, 12),
                ],
            },
        ];
        db.import_document(
            &DocumentMeta {
                title: "fixture".into(),
                author: "fixture author".into(),
                ..Default::default()
            },
            "hash-1",
            Utc::now(),
            &sentences,
        )
        .unwrap()
    }

    pub(crate) fn tok(
        surface: &str,
        lemma: &str,
        reading: &str,
        pos: PartOfSpeech,
        start: usize,
    ) -> NewToken {
        NewToken {
            surface: surface.into(),
            lemma: lemma.into(),
            reading: reading.into(),
            pos,
            start,
            end: start + surface.len(),
        }
    }

    #[test]
    fn import_and_read_back() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);

        let docs = db.list_documents().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].document.title, "fixture");
        assert_eq!(docs[0].document.author, "fixture author");
        assert_eq!(docs[0].document.publisher, "");
        assert_eq!(docs[0].sentence_count, 2);
        assert_eq!(docs[0].token_count, 8);

        let sentences = db.sentences(doc_id).unwrap();
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0].text, "猫が好きだ。");
        assert_eq!(sentences[1].paragraph, 1);

        let tokens = db.sentence_tokens(sentences[0].id).unwrap();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].token.surface, "猫");
        assert_eq!(tokens[0].status, KnowledgeStatus::Unknown);
        assert_eq!(tokens[0].token.pos, PartOfSpeech::Noun);
    }

    #[test]
    fn words_are_deduplicated_across_sentences() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);
        let sentences = db.sentences(doc_id).unwrap();
        let t0 = db.sentence_tokens(sentences[0].id).unwrap();
        let t1 = db.sentence_tokens(sentences[1].id).unwrap();
        let cat0 = t0.iter().find(|t| t.token.surface == "猫").unwrap();
        let cat1 = t1.iter().find(|t| t.token.surface == "猫").unwrap();
        assert_eq!(cat0.word_id, cat1.word_id);
    }

    #[test]
    fn content_hash_lookup() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);
        assert_eq!(db.find_document_by_hash("hash-1").unwrap(), Some(doc_id));
        assert_eq!(db.find_document_by_hash("nope").unwrap(), None);
    }

    #[test]
    fn delete_document_cascades() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);
        db.delete_document(doc_id).unwrap();
        assert!(db.list_documents().unwrap().is_empty());
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM tokens", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "tokens must cascade with the document");
        // Words survive deletion: knowledge is independent of documents.
        let words: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM words", [], |r| r.get(0))
            .unwrap();
        assert!(words > 0);
    }
}
