//! Optional LLM backend for grammar explanations and production feedback.
//!
//! The app must be fully functional without this: everything here is behind
//! the [`Explainer`] trait, and [`explainer_from_env`] returns a
//! [`Disabled`] implementation when no API key is configured. The only
//! provided live backend talks to the Anthropic Messages API over HTTP.

mod anthropic;
pub mod chat;
mod ollama;
mod openai_compat;
mod prompts;

pub use anthropic::AnthropicExplainer;
pub use chat::{
    chat_system_prompt, parse_chat_response, AnnotationSeverity, Challenge, ChatAnnotation,
    ChatMessage, ChatRole, ChatTurnOutcome,
};
pub use ollama::{
    OllamaClient, OllamaExplainer, OllamaModel, PullProgress, DEFAULT_OLLAMA_URL,
};
pub use openai_compat::OpenAiCompatExplainer;
pub use prompts::{
    build_explain_prompt, build_feedback_prompt, writing_prompts, SentenceContext,
};

/// Clip a string to at most `max` characters (for error displays).
pub(crate) fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Errors from the LLM backend.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// No backend is configured; the feature is unavailable, not broken.
    #[error("no LLM backend configured (set ANTHROPIC_API_KEY to enable)")]
    NotConfigured,

    #[error("LLM request failed: {0}")]
    Request(String),

    #[error("unexpected LLM response: {0}")]
    Response(String),
}

/// A backend that can explain Japanese and critique the user's writing.
///
/// Implementations must be cheap to share across threads; calls may block
/// (run them off the GUI thread).
pub trait Explainer: Send + Sync {
    /// Human-readable backend name for display in settings.
    fn name(&self) -> &str;

    /// Whether calls can be expected to succeed.
    fn is_available(&self) -> bool;

    /// Explain not just what a sentence means but *why* it is constructed
    /// the way it is.
    fn explain_sentence(&self, context: &SentenceContext) -> Result<String, LlmError>;

    /// Feedback on the naturalness of user-written Japanese.
    fn production_feedback(&self, prompt: &str, user_text: &str) -> Result<String, LlmError>;

    /// One turn of free conversation: a system prompt plus the message
    /// history, returning the model's raw text.
    fn chat(&self, system: &str, history: &[ChatMessage]) -> Result<String, LlmError> {
        let _ = (system, history);
        Err(LlmError::NotConfigured)
    }
}

/// The no-op backend used when nothing is configured.
#[derive(Debug, Default)]
pub struct Disabled;

impl Explainer for Disabled {
    fn name(&self) -> &str {
        "disabled"
    }

    fn is_available(&self) -> bool {
        false
    }

    fn explain_sentence(&self, _context: &SentenceContext) -> Result<String, LlmError> {
        Err(LlmError::NotConfigured)
    }

    fn production_feedback(&self, _prompt: &str, _user_text: &str) -> Result<String, LlmError> {
        Err(LlmError::NotConfigured)
    }
}

/// Build the best available explainer from the environment:
/// `ANTHROPIC_API_KEY` enables the Anthropic backend, otherwise [`Disabled`].
pub fn explainer_from_env() -> Box<dyn Explainer> {
    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) if !key.trim().is_empty() => Box::new(AnthropicExplainer::new(key)),
        _ => Box::new(Disabled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_backend_reports_unavailable() {
        let backend = Disabled;
        assert!(!backend.is_available());
        assert!(matches!(
            backend.explain_sentence(&SentenceContext::new("猫が好きだ。")),
            Err(LlmError::NotConfigured)
        ));
        assert!(matches!(
            backend.production_feedback("prompt", "text"),
            Err(LlmError::NotConfigured)
        ));
    }
}
