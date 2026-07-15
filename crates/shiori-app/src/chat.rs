//! Production-chat services: analyzing messages for clickable tokens,
//! creating words on demand, and describing the user's level.

use chrono::Utc;
use shiori_core::{KnowledgeStatus, Token, WordId, WordKey};
use shiori_db::WordRow;
use shiori_srs::Card;

use crate::{App, Result};

/// One token of a chat message, with byte offsets into the *whole*
/// message and the tracked word it resolves to (if any exists yet —
/// chat doesn't create words until the user interacts with one).
#[derive(Debug, Clone)]
pub struct ChatTokenRow {
    pub token: Token,
    pub word: Option<WordRow>,
}

/// One analyzed sentence of a chat message: its tokens (offsets made
/// absolute into the whole message) and their phrase groups.
pub type ChatSentence = (Vec<ChatTokenRow>, Vec<(usize, usize)>);

impl App {
    /// Tokenize a chat message for display, sentence by sentence.
    pub fn analyze_chat_text(&self, text: &str) -> Result<Vec<ChatSentence>> {
        let analyzed = self.analyzer().analyze(text)?;
        let mut out = Vec::new();
        // Sentences appear in order; walk a cursor to locate each one's
        // byte offset in the original text.
        let mut cursor = 0usize;
        for paragraph in &analyzed.paragraphs {
            for sentence in &paragraph.sentences {
                let base = text
                    .get(cursor..)
                    .and_then(|hay| hay.find(&sentence.text).map(|i| cursor + i))
                    .unwrap_or(cursor);
                cursor = base + sentence.text.len();

                let groups = shiori_nlp::phrase_groups(&sentence.tokens);
                let tokens = sentence
                    .tokens
                    .iter()
                    .map(|t| {
                        let mut token = t.clone();
                        token.start += base;
                        token.end += base;
                        let word = self
                            .db()
                            .find_word(
                                self.active_lang(),
                                &WordKey {
                                    lemma: t.lemma.clone(),
                                    reading: t.reading.clone(),
                                    pos: t.pos,
                                },
                            )
                            .unwrap_or(None);
                        ChatTokenRow { token, word }
                    })
                    .collect();
                out.push((tokens, groups));
            }
        }
        Ok(out)
    }

    /// The tracked word for a key, created at `unknown` if new — used
    /// when the user clicks a chat token the library has never seen.
    pub fn ensure_word(&self, key: &WordKey) -> Result<WordRow> {
        Ok(self.db().ensure_word(self.active_lang(), key)?)
    }

    /// Put a word into the SRS without a context sentence (chat words
    /// have no document sentence to anchor to).
    pub fn start_learning_uncontexted(&self, word_id: WordId) -> Result<()> {
        if self.db().card(word_id)?.is_some() {
            return Ok(());
        }
        self.db()
            .upsert_card(word_id, None, &Card::new(Utc::now()))?;
        self.db()
            .set_word_status(word_id, KnowledgeStatus::Learning)?;
        Ok(())
    }

    /// One-line description of the user's recorded vocabulary for the
    /// chat partner's system prompt.
    pub fn chat_level_hint(&self) -> Result<String> {
        let known: u32 = self
            .db()
            .word_status_counts(self.active_lang())?
            .iter()
            .filter(|(s, _)| *s == KnowledgeStatus::Known)
            .map(|(_, n)| *n)
            .sum();
        let level = match known {
            0..=150 => "a beginner (around JLPT N5)",
            151..=600 => "an upper beginner (around JLPT N5–N4)",
            601..=1500 => "lower intermediate (around JLPT N4)",
            1501..=3500 => "intermediate (around JLPT N3)",
            3501..=7000 => "upper intermediate (around JLPT N2)",
            _ => "advanced (around JLPT N1)",
        };
        Ok(format!(
            "The user has about {known} recorded known words, suggesting {level}."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap()
    }

    #[test]
    fn chat_analysis_has_absolute_offsets() {
        let app = app();
        let text = "猫が好きだ。犬も好きだ。";
        let sentences = app.analyze_chat_text(text).unwrap();
        assert_eq!(sentences.len(), 2);
        // Every token's span must slice cleanly out of the whole text.
        for (tokens, _) in &sentences {
            for row in tokens {
                assert_eq!(
                    &text[row.token.start..row.token.end],
                    row.token.surface,
                    "token offsets must be absolute into the message"
                );
            }
        }
        // Second sentence's first token starts after the first sentence.
        let first_of_second = &sentences[1].0[0];
        assert!(first_of_second.token.start >= "猫が好きだ。".len());
    }

    #[test]
    fn ensure_word_creates_once() {
        let app = app();
        let key = WordKey::new("勉強", "べんきょう", shiori_core::PartOfSpeech::Noun);
        let first = app.ensure_word(&key).unwrap();
        assert_eq!(first.status, KnowledgeStatus::Unknown);
        let second = app.ensure_word(&key).unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn uncontexted_learning_creates_a_card() {
        let app = app();
        let key = WordKey::new("勉強", "べんきょう", shiori_core::PartOfSpeech::Noun);
        let word = app.ensure_word(&key).unwrap();
        app.start_learning_uncontexted(word.id).unwrap();
        assert_eq!(
            app.db().word(word.id).unwrap().status,
            KnowledgeStatus::Learning
        );
        let card = app.db().card(word.id).unwrap().unwrap();
        assert!(card.sentence_id.is_none());
        // Idempotent.
        app.start_learning_uncontexted(word.id).unwrap();
    }

    #[test]
    fn level_hint_mentions_count() {
        let app = app();
        let hint = app.chat_level_hint().unwrap();
        assert!(hint.contains("0 recorded"));
        assert!(hint.contains("N5"));
    }
}
