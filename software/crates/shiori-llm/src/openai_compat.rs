//! Generic OpenAI-compatible backend: any server speaking the
//! `/chat/completions` dialect (LM Studio, llama.cpp server, vLLM,
//! OpenAI itself, Ollama's `/v1`, …).

use serde::Deserialize;

use crate::prompts::{build_explain_prompt, build_feedback_prompt, SentenceContext};
use crate::{Explainer, LlmError};

/// Explainer over an OpenAI-compatible chat-completions endpoint.
pub struct OpenAiCompatExplainer {
    /// Base URL up to and including the version segment, e.g.
    /// `http://localhost:1234/v1`; `/chat/completions` is appended.
    base_url: String,
    api_key: String,
    model: String,
    agent: ureq::Agent,
}

impl OpenAiCompatExplainer {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            model: model.into(),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(600))
                .build(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, system: &str, user_prompt: &str) -> Result<String, LlmError> {
        self.request(serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user_prompt}
            ],
            "stream": false
        }))
    }

    fn request(&self, body: serde_json::Value) -> Result<String, LlmError> {
        #[derive(Deserialize)]
        struct ChatResponse {
            choices: Vec<Choice>,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }
        #[derive(Deserialize)]
        struct Message {
            #[serde(default)]
            content: Option<String>,
        }
        let mut request = self
            .agent
            .post(&format!("{}/chat/completions", self.base_url))
            .set("content-type", "application/json");
        if !self.api_key.trim().is_empty() {
            request = request.set("authorization", &format!("Bearer {}", self.api_key));
        }
        let parsed: ChatResponse = request
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(code, resp) => {
                    let detail = resp.into_string().unwrap_or_default();
                    LlmError::Request(format!("HTTP {code}: {}", crate::truncate(&detail, 300)))
                }
                other => LlmError::Request(other.to_string()),
            })?
            .into_json()
            .map_err(|e| LlmError::Response(e.to_string()))?;

        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| LlmError::Response("no content in response".into()))
    }
}

impl Explainer for OpenAiCompatExplainer {
    fn name(&self) -> &str {
        "OpenAI-compatible"
    }

    fn is_available(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }

    fn explain_sentence(&self, context: &SentenceContext) -> Result<String, LlmError> {
        self.complete(
            &crate::prompts::system_prompt(&context.profile),
            &build_explain_prompt(context),
        )
    }

    fn production_feedback(
        &self,
        profile: &crate::PromptProfile,
        prompt: &str,
        user_text: &str,
    ) -> Result<String, LlmError> {
        self.complete(
            &crate::prompts::system_prompt(profile),
            &build_feedback_prompt(profile, prompt, user_text),
        )
    }

    fn chat(&self, system: &str, history: &[crate::ChatMessage]) -> Result<String, LlmError> {
        let mut messages = vec![serde_json::json!({"role": "system", "content": system})];
        messages.extend(
            history
                .iter()
                .map(|m| serde_json::json!({"role": m.role.as_str(), "content": m.content})),
        );
        self.request(serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_needs_url_and_model() {
        assert!(!OpenAiCompatExplainer::new("", "", "").is_available());
        assert!(!OpenAiCompatExplainer::new("http://x/v1", "", "").is_available());
        let backend = OpenAiCompatExplainer::new("http://localhost:1234/v1/", "", "qwen");
        assert!(backend.is_available());
        assert_eq!(backend.name(), "OpenAI-compatible");
    }
}
