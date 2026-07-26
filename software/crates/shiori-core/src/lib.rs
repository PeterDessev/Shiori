//! Shared domain types and errors for Shiori.
//!
//! Everything in this crate is plain data: no I/O, no panics, no policy.
//! The other crates agree on these types so that, for example, the NLP
//! pipeline, the database, and the GUI all mean the same thing by "a word".

pub mod error;
pub mod text;
pub mod word;

pub use error::{Error, Result};
pub use text::{Document, DocumentId, DocumentMeta, Sentence, SentenceId, Token};
pub use word::{KnowledgeStatus, PartOfSpeech, WordId, WordKey};
