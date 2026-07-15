//! Production-chat storage: conversations, messages, and the paper-style
//! annotation spans attached to user messages.

use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::{Db, Result};

/// A conversation in the history sidebar.
#[derive(Debug, Clone)]
pub struct ConversationRow {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub title: String,
    pub message_count: u32,
}

/// One stored chat message.
#[derive(Debug, Clone)]
pub struct ChatMessageRow {
    pub id: i64,
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// One write-up span over a user message (byte offsets into content).
#[derive(Debug, Clone)]
pub struct ChatAnnotationRow {
    pub start: usize,
    pub end: usize,
    /// "error" or "awkward".
    pub severity: String,
    pub note: String,
}

impl Db {
    pub fn create_conversation(&self, lang: &str, at: DateTime<Utc>, title: &str) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO conversations(lang, started_at, title) VALUES (?1, ?2, ?3)",
            params![lang, at, title],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn set_conversation_title(&self, id: i64, title: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE conversations SET title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        Ok(())
    }

    pub fn delete_conversation(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM conversations WHERE id = ?1", [id])?;
        Ok(())
    }

    /// One language's conversations, newest first.
    pub fn list_conversations(&self, lang: &str) -> Result<Vec<ConversationRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT c.id, c.started_at, c.title,
                    (SELECT COUNT(*) FROM chat_messages m WHERE m.conversation_id = c.id)
             FROM conversations c WHERE c.lang = ?1
             ORDER BY c.started_at DESC, c.id DESC",
        )?;
        let rows = stmt.query_map([lang], |r| {
            Ok(ConversationRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                title: r.get(2)?,
                message_count: r.get::<_, i64>(3)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Append a message, returning its id.
    pub fn add_chat_message(
        &self,
        conversation: i64,
        role: &str,
        content: &str,
        at: DateTime<Utc>,
    ) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO chat_messages(conversation_id, idx, role, content, created_at)
             VALUES (?1,
                     (SELECT COALESCE(MAX(idx), -1) + 1 FROM chat_messages
                      WHERE conversation_id = ?1),
                     ?2, ?3, ?4)",
            params![conversation, role, content, at],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Messages of a conversation in order.
    pub fn conversation_messages(&self, conversation: i64) -> Result<Vec<ChatMessageRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT id, role, content, created_at FROM chat_messages
             WHERE conversation_id = ?1 ORDER BY idx",
        )?;
        let rows = stmt.query_map([conversation], |r| {
            Ok(ChatMessageRow {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn add_chat_annotations(
        &self,
        message: i64,
        annotations: &[ChatAnnotationRow],
    ) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chat_annotations(message_id, start, end, severity, note)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for a in annotations {
                stmt.execute(params![
                    message,
                    a.start as i64,
                    a.end as i64,
                    a.severity,
                    a.note
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn chat_annotations(&self, message: i64) -> Result<Vec<ChatAnnotationRow>> {
        let mut stmt = self.conn().prepare(
            "SELECT start, end, severity, note FROM chat_annotations
             WHERE message_id = ?1 ORDER BY start",
        )?;
        let rows = stmt.query_map([message], |r| {
            Ok(ChatAnnotationRow {
                start: r.get::<_, i64>(0)? as usize,
                end: r.get::<_, i64>(1)? as usize,
                severity: r.get(2)?,
                note: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let now = Utc::now();
        let conv = db.create_conversation("ja", now, "").unwrap();
        db.set_conversation_title(conv, "天気の話").unwrap();

        let m1 = db
            .add_chat_message(conv, "user", "今日はいい天気ですね", now)
            .unwrap();
        let m2 = db
            .add_chat_message(conv, "assistant", "そうですね！", now)
            .unwrap();
        assert_ne!(m1, m2);

        db.add_chat_annotations(
            m1,
            &[ChatAnnotationRow {
                start: 0,
                end: 6,
                severity: "awkward".into(),
                note: "more natural to drop は here".into(),
            }],
        )
        .unwrap();

        let convs = db.list_conversations("ja").unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].title, "天気の話");
        assert_eq!(convs[0].message_count, 2);
        // Another language's history is separate.
        assert!(db.list_conversations("grc").unwrap().is_empty());

        let messages = db.conversation_messages(conv).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].content, "そうですね！");

        let annotations = db.chat_annotations(m1).unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].severity, "awkward");
        assert!(db.chat_annotations(m2).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_conversation_cascades() {
        let db = Db::open_in_memory().unwrap();
        let conv = db.create_conversation("ja", Utc::now(), "x").unwrap();
        let m = db
            .add_chat_message(conv, "user", "テスト", Utc::now())
            .unwrap();
        db.add_chat_annotations(
            m,
            &[ChatAnnotationRow {
                start: 0,
                end: 3,
                severity: "error".into(),
                note: "n".into(),
            }],
        )
        .unwrap();
        db.delete_conversation(conv).unwrap();
        assert!(db.list_conversations("ja").unwrap().is_empty());
        let orphans: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM chat_annotations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn message_indices_are_per_conversation() {
        let db = Db::open_in_memory().unwrap();
        let now = Utc::now();
        let a = db.create_conversation("ja", now, "a").unwrap();
        let b = db.create_conversation("ja", now, "b").unwrap();
        db.add_chat_message(a, "user", "one", now).unwrap();
        db.add_chat_message(b, "user", "uno", now).unwrap();
        db.add_chat_message(a, "assistant", "two", now).unwrap();
        let msgs = db.conversation_messages(a).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "one");
        assert_eq!(msgs[1].content, "two");
    }
}
