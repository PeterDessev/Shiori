//! Anthropic Messages API backend (raw HTTP; Rust has no official SDK).

use serde::Deserialize;

use crate::prompts::{build_explain_prompt, build_feedback_prompt, SentenceContext, SYSTEM_PROMPT};
use crate::{Explainer, LlmError};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-opus-4-8";
const MAX_TOKENS: u32 = 4096;

/// Explainer backed by the Anthropic Messages API.
pub struct AnthropicExplainer {
    api_key: String,
    model: String,
    agent: ureq::Agent,
}

impl AnthropicExplainer {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_model(api_key, DEFAULT_MODEL)
    }

    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(120))
                .build(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, user_prompt: &str) -> Result<String, LlmError> {
        self.request(serde_json::json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": SYSTEM_PROMPT,
            "thinking": {"type": "adaptive"},
            "messages": [
                {"role": "user", "content": user_prompt}
            ]
        }))
    }

    fn request(&self, body: serde_json::Value) -> Result<String, LlmError> {
        let response = self
            .agent
            .post(API_URL)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", API_VERSION)
            .set("content-type", "application/json")
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, resp) => {
                    let detail = resp.into_string().unwrap_or_default();
                    LlmError::Request(format!("HTTP {code}: {}", truncate(&detail, 300)))
                }
                other => LlmError::Request(other.to_string()),
            })?;

        let parsed: MessagesResponse = response
            .into_json()
            .map_err(|e| LlmError::Response(e.to_string()))?;
        extract_text(&parsed)
    }
}

impl Explainer for AnthropicExplainer {
    fn name(&self) -> &str {
        "Anthropic"
    }

    fn is_available(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    fn explain_sentence(&self, context: &SentenceContext) -> Result<String, LlmError> {
        self.complete(&build_explain_prompt(context))
    }

    fn production_feedback(&self, prompt: &str, user_text: &str) -> Result<String, LlmError> {
        self.complete(&build_feedback_prompt(prompt, user_text))
    }

    fn chat(
        &self,
        system: &str,
        history: &[crate::ChatMessage],
    ) -> Result<String, LlmError> {
        let messages: Vec<serde_json::Value> = history
            .iter()
            .map(|m| serde_json::json!({"role": m.role.as_str(), "content": m.content}))
            .collect();
        self.request(serde_json::json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": system,
            "messages": messages
        }))
    }
}

/// Subset of the Messages API response we care about.
#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

/// Content blocks: text plus anything else (thinking, tool_use), which we
/// skip. `other` keeps deserialization future-proof.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

fn extract_text(response: &MessagesResponse) -> Result<String, LlmError> {
    if response.stop_reason.as_deref() == Some("refusal") {
        return Err(LlmError::Response("the model declined to answer".into()));
    }
    let text: Vec<&str> = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            ContentBlock::Other => None,
        })
        .collect();
    if text.is_empty() {
        return Err(LlmError::Response("no text content in response".into()));
    }
    Ok(text.join("\n"))
}

use crate::truncate;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_blocks_and_skips_thinking() {
        let json = r#"{
            "id": "msg_1",
            "content": [
                {"type": "thinking", "thinking": "", "signature": "x"},
                {"type": "text", "text": "Explanation here."}
            ],
            "stop_reason": "end_turn"
        }"#;
        let parsed: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&parsed).unwrap(), "Explanation here.");
    }

    #[test]
    fn refusal_is_an_error() {
        let json = r#"{"content": [{"type": "text", "text": "x"}], "stop_reason": "refusal"}"#;
        let parsed: MessagesResponse = serde_json::from_str(json).unwrap();
        assert!(extract_text(&parsed).is_err());
    }

    #[test]
    fn empty_content_is_an_error() {
        let json = r#"{"content": [], "stop_reason": "end_turn"}"#;
        let parsed: MessagesResponse = serde_json::from_str(json).unwrap();
        assert!(extract_text(&parsed).is_err());
    }

    #[test]
    fn multiple_text_blocks_are_joined() {
        let json = r#"{"content": [
            {"type": "text", "text": "Part 1."},
            {"type": "text", "text": "Part 2."}
        ]}"#;
        let parsed: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&parsed).unwrap(), "Part 1.\nPart 2.");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("ねこねこ", 2), "ねこ");
        assert_eq!(truncate("ab", 10), "ab");
    }

    #[test]
    fn backend_metadata() {
        let backend = AnthropicExplainer::new("key");
        assert_eq!(backend.name(), "Anthropic");
        assert!(backend.is_available());
        assert_eq!(backend.model(), "claude-opus-4-8");
        assert!(!AnthropicExplainer::new("  ").is_available());
    }
}
