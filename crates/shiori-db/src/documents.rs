//! Document, sentence, and token storage.

use chrono::{DateTime, Utc};
use rusqlite::params;
use shiori_core::{
    Document, DocumentId, DocumentMeta, KnowledgeStatus, PartOfSpeech, Sentence, SentenceId, Token,
    WordId,
};

use crate::{Db, DbError, Result};

/// A token as produced by the analyzer (or carried by a pre-annotated
/// text), ready for import.
#[derive(Debug, Clone)]
pub struct NewToken {
    pub surface: String,
    pub lemma: String,
    pub reading: String,
    pub pos: PartOfSpeech,
    pub start: usize,
    pub end: usize,
    /// Language-pack parse code for this occurrence (e.g. "V-AAI-3S");
    /// `None` for analyzer-produced tokens.
    pub morph: Option<String>,
    /// Short per-occurrence gloss from a pre-annotated text.
    pub gloss: Option<String>,
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
    /// Stored parse code of this occurrence, if the text was imported
    /// pre-annotated.
    pub morph: Option<String>,
    /// Stored per-occurrence gloss, if the text was imported pre-annotated.
    pub gloss: Option<String>,
}

fn row_to_document(r: &rusqlite::Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: DocumentId(r.get(0)?),
        lang: r.get(1)?,
        title: r.get(2)?,
        author: r.get(3)?,
        publisher: r.get(4)?,
        published: r.get(5)?,
        last_sentence: r.get::<_, i64>(6)? as u32,
        added_at: r.get(7)?,
    })
}

const DOC_COLS: &str = "id, lang, title, author, publisher, published, last_sentence, added_at";

impl Db {
    /// Id of the document with this content hash in this language, if
    /// already imported.
    pub fn find_document_by_hash(&self, lang: &str, hash: &str) -> Result<Option<DocumentId>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT id FROM documents WHERE lang = ?1 AND content_hash = ?2")?;
        let id = stmt
            .query_row([lang, hash], |r| r.get::<_, i64>(0))
            .map(DocumentId);
        match id {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Import a fully analyzed document in one transaction.
    ///
    /// Words are upserted by their (lang, lemma, reading, pos) identity;
    /// tokens reference them.
    pub fn import_document(
        &self,
        lang: &str,
        meta: &DocumentMeta,
        content_hash: &str,
        added_at: DateTime<Utc>,
        sentences: &[NewSentence],
    ) -> Result<DocumentId> {
        let tx = self.conn().unchecked_transaction()?;

        tx.execute(
            "INSERT INTO documents(lang, title, author, publisher, published, added_at,
                                   content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                lang,
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
                "SELECT id FROM words
                 WHERE lang = ?1 AND lemma = ?2 AND reading = ?3 AND pos = ?4",
            )?;
            let mut insert_word =
                tx.prepare("INSERT INTO words(lang, lemma, reading, pos) VALUES (?1, ?2, ?3, ?4)")?;
            let mut insert_token = tx.prepare(
                "INSERT INTO tokens(sentence_id, idx, word_id, surface, start, end, morph, gloss)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                        .query_row(params![lang, token.lemma, token.reading, pos], |r| r.get(0))
                    {
                        Ok(id) => id,
                        Err(rusqlite::Error::QueryReturnedNoRows) => {
                            insert_word.execute(params![lang, token.lemma, token.reading, pos])?;
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
                        token.end as i64,
                        token.morph,
                        token.gloss
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
                &format!("SELECT {DOC_COLS} FROM documents WHERE id = ?1"),
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
            "SELECT d.id, d.lang, d.title, d.author, d.publisher, d.published,
                    d.last_sentence, d.added_at,
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
                sentence_count: r.get::<_, i64>(8)? as u32,
                token_count: r.get::<_, i64>(9)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Remember where the user is in a document (first sentence of the
    /// current page).
    pub fn set_reading_position(&self, id: DocumentId, sentence_index: u32) -> Result<()> {
        self.conn().execute(
            "UPDATE documents SET last_sentence = ?2 WHERE id = ?1",
            params![id.0, sentence_index as i64],
        )?;
        Ok(())
    }

    /// The sentence at a given position of a document, if any.
    pub fn sentence_at(&self, document: DocumentId, index: i64) -> Result<Option<Sentence>> {
        if index < 0 {
            return Ok(None);
        }
        let result = self.conn().query_row(
            "SELECT id, document_id, idx, paragraph, text
             FROM sentences WHERE document_id = ?1 AND idx = ?2",
            params![document.0, index],
            |r| {
                Ok(Sentence {
                    id: SentenceId(r.get(0)?),
                    document_id: DocumentId(r.get(1)?),
                    index: r.get::<_, i64>(2)? as u32,
                    paragraph: r.get::<_, i64>(3)? as u32,
                    text: r.get(4)?,
                })
            },
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update a document's descriptive metadata.
    pub fn update_document_meta(&self, id: DocumentId, meta: &DocumentMeta) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE documents SET title = ?2, author = ?3, publisher = ?4, published = ?5
             WHERE id = ?1",
            params![
                id.0,
                meta.title,
                meta.author,
                meta.publisher,
                meta.published
            ],
        )?;
        if n == 0 {
            return Err(DbError::NotFound("document"));
        }
        Ok(())
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

    /// Up to `limit` sentences from across the library containing a word,
    /// with their document titles. Sentences from documents other than
    /// `home_document` come first (cross-book context beats repetition),
    /// and the card's own sentence is excluded.
    pub fn word_example_sentences(
        &self,
        word: WordId,
        exclude: Option<SentenceId>,
        home_document: Option<DocumentId>,
        limit: u32,
    ) -> Result<Vec<(Sentence, String)>> {
        let mut stmt = self.conn().prepare(
            "SELECT DISTINCT s.id, s.document_id, s.idx, s.paragraph, s.text, d.title
             FROM tokens t
             JOIN sentences s ON s.id = t.sentence_id
             JOIN documents d ON d.id = s.document_id
             WHERE t.word_id = ?1 AND s.id != COALESCE(?2, -1)
             ORDER BY (s.document_id = COALESCE(?3, -1)), s.document_id, s.idx
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![
                word.0,
                exclude.map(|s| s.0),
                home_document.map(|d| d.0),
                limit
            ],
            |r| {
                Ok((
                    Sentence {
                        id: SentenceId(r.get(0)?),
                        document_id: DocumentId(r.get(1)?),
                        index: r.get::<_, i64>(2)? as u32,
                        paragraph: r.get::<_, i64>(3)? as u32,
                        text: r.get(4)?,
                    },
                    r.get(5)?,
                ))
            },
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A random sentence of one language's library with at least a few
    /// words — drill material drawn from the user's own reading.
    pub fn random_sentence_text(&self, lang: &str) -> Result<Option<String>> {
        let result = self.conn().query_row(
            "SELECT s.text FROM sentences s
             JOIN documents d ON d.id = s.document_id
             WHERE d.lang = ?1 AND LENGTH(s.text) >= 12
             ORDER BY RANDOM() LIMIT 1",
            [lang],
            |r| r.get(0),
        );
        match result {
            Ok(text) => Ok(Some(text)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Tokens of a sentence in order, joined with word status.
    pub fn sentence_tokens(&self, sentence: SentenceId) -> Result<Vec<TokenRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT t.surface, w.lemma, w.reading, w.pos, t.start, t.end, w.id, w.status,
                    t.morph, t.gloss
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
                morph: r.get(8)?,
                gloss: r.get(9)?,
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
            "ja",
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
            morph: None,
            gloss: None,
        }
    }

    #[test]
    fn import_and_read_back() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);

        let docs = db.list_documents().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].document.title, "fixture");
        assert_eq!(docs[0].document.lang, "ja");
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
        assert_eq!(tokens[0].morph, None);
    }

    #[test]
    fn pre_annotated_tokens_round_trip_morph_and_gloss() {
        let db = Db::open_in_memory().unwrap();
        let doc = db
            .import_document(
                "grc",
                &DocumentMeta::titled("John"),
                "hash-grc",
                Utc::now(),
                &[NewSentence {
                    paragraph: 0,
                    text: "Ἐν ἀρχῇ ἦν ὁ λόγος".into(),
                    tokens: vec![
                        NewToken {
                            surface: "Ἐν".into(),
                            lemma: "ἐν".into(),
                            reading: String::new(),
                            pos: PartOfSpeech::Unknown,
                            start: 0,
                            end: "Ἐν".len(),
                            morph: Some("P".into()),
                            gloss: Some("in".into()),
                        },
                        NewToken {
                            surface: "λόγος".into(),
                            lemma: "λόγος".into(),
                            reading: String::new(),
                            pos: PartOfSpeech::Noun,
                            start: "Ἐν ἀρχῇ ἦν ὁ ".len(),
                            end: "Ἐν ἀρχῇ ἦν ὁ λόγος".len(),
                            morph: Some("N-NSM".into()),
                            gloss: Some("word".into()),
                        },
                    ],
                }],
            )
            .unwrap();
        let sentences = db.sentences(doc).unwrap();
        let tokens = db.sentence_tokens(sentences[0].id).unwrap();
        assert_eq!(tokens[0].morph.as_deref(), Some("P"));
        assert_eq!(tokens[0].gloss.as_deref(), Some("in"));
        assert_eq!(tokens[1].morph.as_deref(), Some("N-NSM"));
        // The stored words are scoped to the document's language.
        assert!(db
            .find_word(
                "grc",
                &shiori_core::WordKey::new("λόγος", "", PartOfSpeech::Noun)
            )
            .unwrap()
            .is_some());
        assert!(db
            .find_word(
                "ja",
                &shiori_core::WordKey::new("λόγος", "", PartOfSpeech::Noun)
            )
            .unwrap()
            .is_none());
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
    fn reading_position_roundtrip_and_sentence_at() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);
        assert_eq!(db.document(doc_id).unwrap().last_sentence, 0);

        db.set_reading_position(doc_id, 1).unwrap();
        assert_eq!(db.document(doc_id).unwrap().last_sentence, 1);
        assert_eq!(db.list_documents().unwrap()[0].document.last_sentence, 1);

        let s = db.sentence_at(doc_id, 1).unwrap().unwrap();
        assert_eq!(s.text, "その猫は走る。");
        assert!(db.sentence_at(doc_id, -1).unwrap().is_none());
        assert!(db.sentence_at(doc_id, 99).unwrap().is_none());
    }

    #[test]
    fn metadata_can_be_edited() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);
        db.update_document_meta(
            doc_id,
            &DocumentMeta {
                title: "新タイトル".into(),
                author: "新著者".into(),
                publisher: "出版社".into(),
                published: "2024".into(),
            },
        )
        .unwrap();
        let doc = db.document(doc_id).unwrap();
        assert_eq!(doc.title, "新タイトル");
        assert_eq!(doc.author, "新著者");
        assert_eq!(doc.publisher, "出版社");
        assert_eq!(doc.published, "2024");

        assert!(db
            .update_document_meta(DocumentId(999), &DocumentMeta::default())
            .is_err());
    }

    #[test]
    fn word_examples_prefer_other_documents() {
        let db = Db::open_in_memory().unwrap();
        let doc1 = import_fixture(&db);
        // A second document also containing 猫.
        let doc2 = db
            .import_document(
                "ja",
                &DocumentMeta {
                    title: "second".into(),
                    ..Default::default()
                },
                "hash-2",
                Utc::now(),
                &[NewSentence {
                    paragraph: 0,
                    text: "猫も歩く。".into(),
                    tokens: vec![
                        tok("猫", "猫", "ねこ", PartOfSpeech::Noun, 0),
                        tok("も", "も", "も", PartOfSpeech::Particle, 3),
                        tok("歩く", "歩く", "あるく", PartOfSpeech::Verb, 6),
                    ],
                }],
            )
            .unwrap();

        let s1 = db.sentences(doc1).unwrap();
        let cat = db
            .sentence_tokens(s1[0].id)
            .unwrap()
            .into_iter()
            .find(|t| t.token.surface == "猫")
            .unwrap();

        // Excluding the card's own sentence, the other-document example
        // ranks first even though it was imported later.
        let examples = db
            .word_example_sentences(cat.word_id, Some(s1[0].id), Some(doc1), 5)
            .unwrap();
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].1, "second");
        assert_eq!(examples[0].0.document_id, doc2);
        assert_eq!(examples[1].0.text, "その猫は走る。");
        assert!(examples.iter().all(|(s, _)| s.id != s1[0].id));
    }

    #[test]
    fn content_hash_lookup_is_language_scoped() {
        let db = Db::open_in_memory().unwrap();
        let doc_id = import_fixture(&db);
        assert_eq!(
            db.find_document_by_hash("ja", "hash-1").unwrap(),
            Some(doc_id)
        );
        assert_eq!(db.find_document_by_hash("ja", "nope").unwrap(), None);
        // The same file may be imported again under another language.
        assert_eq!(db.find_document_by_hash("grc", "hash-1").unwrap(), None);
        let other = db
            .import_document(
                "grc",
                &DocumentMeta::titled("same bytes"),
                "hash-1",
                Utc::now(),
                &[],
            )
            .unwrap();
        assert_ne!(other, doc_id);
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
