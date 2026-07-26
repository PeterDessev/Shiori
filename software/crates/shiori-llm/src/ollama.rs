//! Ollama backend: local models over the native REST API.
//!
//! Conventions (per the official API docs): NDJSON streaming (not SSE),
//! durations in nanoseconds, mid-stream errors as `{"error": ...}` lines
//! on a 200 response.

use std::io::BufRead;

use serde::Deserialize;

use crate::prompts::{build_explain_prompt, build_feedback_prompt, SentenceContext};
use crate::{Explainer, LlmError};

pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// A locally installed model, for the settings picker.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModel {
    /// `model:tag` identifier.
    pub model: String,
    /// Size on disk in bytes.
    pub size: u64,
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    #[serde(default)]
    pub parameter_size: Option<String>,
    #[serde(default)]
    pub quantization_level: Option<String>,
}

/// One line of `/api/pull` progress.
#[derive(Debug, Clone, Deserialize)]
pub struct PullProgress {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Management client: liveness, model list, pulls. Separate from the
/// [`Explainer`] so settings can probe without a model configured.
pub struct OllamaClient {
    base_url: String,
    agent: ureq::Agent,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: normalize_base(base_url.into()),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(2))
                .build(),
        }
    }

    /// Server version, if Ollama is actually running there.
    pub fn version(&self) -> Result<String, LlmError> {
        #[derive(Deserialize)]
        struct Version {
            version: String,
        }
        let v: Version = self
            .agent
            .get(&format!("{}/api/version", self.base_url))
            .timeout(std::time::Duration::from_secs(3))
            .call()
            .map_err(request_error)?
            .into_json()
            .map_err(|e| LlmError::Response(e.to_string()))?;
        Ok(v.version)
    }

    /// Locally installed models.
    pub fn list_models(&self) -> Result<Vec<OllamaModel>, LlmError> {
        #[derive(Deserialize)]
        struct Tags {
            models: Vec<OllamaModel>,
        }
        let tags: Tags = self
            .agent
            .get(&format!("{}/api/tags", self.base_url))
            .timeout(std::time::Duration::from_secs(5))
            .call()
            .map_err(request_error)?
            .into_json()
            .map_err(|e| LlmError::Response(e.to_string()))?;
        Ok(tags.models)
    }

    /// Pull a model from the registry, streaming NDJSON progress to the
    /// callback. Blocking and potentially very long — run on a worker
    /// thread. Returns once the final `success` line arrives.
    pub fn pull(
        &self,
        model: &str,
        mut on_progress: impl FnMut(&PullProgress),
    ) -> Result<(), LlmError> {
        let response = self
            .agent
            .post(&format!("{}/api/pull", self.base_url))
            .send_json(serde_json::json!({ "model": model }))
            .map_err(request_error)?;

        let reader = std::io::BufReader::new(response.into_reader());
        let mut succeeded = false;
        for line in reader.lines() {
            let line = line.map_err(|e| LlmError::Request(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let progress: PullProgress = serde_json::from_str(&line)
                .map_err(|e| LlmError::Response(format!("bad progress line: {e}")))?;
            if let Some(error) = &progress.error {
                return Err(LlmError::Request(error.clone()));
            }
            if progress.status.as_deref() == Some("success") {
                succeeded = true;
            }
            on_progress(&progress);
        }
        if succeeded {
            Ok(())
        } else {
            Err(LlmError::Response(
                "pull stream ended without success".into(),
            ))
        }
    }
}

/// Explainer over a local Ollama model via `/api/chat`.
pub struct OllamaExplainer {
    base_url: String,
    model: String,
    agent: ureq::Agent,
}

impl OllamaExplainer {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: normalize_base(base_url.into()),
            model: model.into(),
            agent: ureq::AgentBuilder::new()
                // First call after a cold start loads the model into
                // memory, which alone can take tens of seconds.
                .timeout(std::time::Duration::from_secs(600))
                .build(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, system: &str, user_prompt: &str) -> Result<String, LlmError> {
        #[derive(Deserialize)]
        struct ChatResponse {
            message: ChatMessage,
            #[serde(default)]
            done_reason: Option<String>,
        }
        #[derive(Deserialize)]
        struct ChatMessage {
            content: String,
        }
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user_prompt}
            ]
        });
        let parsed: ChatResponse = self
            .agent
            .post(&format!("{}/api/chat", self.base_url))
            .send_json(body)
            .map_err(request_error)?
            .into_json()
            .map_err(|e| LlmError::Response(e.to_string()))?;
        if parsed.message.content.is_empty() {
            return Err(LlmError::Response(format!(
                "empty response (done_reason: {})",
                parsed.done_reason.as_deref().unwrap_or("unknown")
            )));
        }
        Ok(parsed.message.content)
    }
}

impl Explainer for OllamaExplainer {
    fn name(&self) -> &str {
        "Ollama"
    }

    fn is_available(&self) -> bool {
        !self.model.trim().is_empty()
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
        #[derive(Deserialize)]
        struct ChatResponse {
            message: ChatMessageContent,
        }
        #[derive(Deserialize)]
        struct ChatMessageContent {
            content: String,
        }
        let mut messages = vec![serde_json::json!({"role": "system", "content": system})];
        messages.extend(
            history
                .iter()
                .map(|m| serde_json::json!({"role": m.role.as_str(), "content": m.content})),
        );
        let parsed: ChatResponse = self
            .agent
            .post(&format!("{}/api/chat", self.base_url))
            .send_json(serde_json::json!({
                "model": self.model,
                "stream": false,
                "messages": messages
            }))
            .map_err(request_error)?
            .into_json()
            .map_err(|e| LlmError::Response(e.to_string()))?;
        if parsed.message.content.is_empty() {
            return Err(LlmError::Response("empty chat response".into()));
        }
        Ok(parsed.message.content)
    }
}

fn normalize_base(mut url: String) -> String {
    if url.trim().is_empty() {
        url = DEFAULT_OLLAMA_URL.to_string();
    }
    url.trim().trim_end_matches('/').to_string()
}

fn request_error(e: ureq::Error) -> LlmError {
    match e {
        ureq::Error::Status(code, resp) => {
            let detail = resp.into_string().unwrap_or_default();
            // Ollama errors are JSON {"error": "..."}; surface the message.
            let msg = serde_json::from_str::<serde_json::Value>(&detail)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or(detail);
            LlmError::Request(format!("HTTP {code}: {}", crate::truncate(&msg, 300)))
        }
        other => LlmError::Request(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_normalization() {
        assert_eq!(normalize_base("".into()), DEFAULT_OLLAMA_URL);
        assert_eq!(
            normalize_base("http://localhost:11434/".into()),
            "http://localhost:11434"
        );
        assert_eq!(
            normalize_base("  http://10.0.0.5:11434  ".into()),
            "http://10.0.0.5:11434"
        );
    }

    #[test]
    fn explainer_needs_a_model() {
        assert!(!OllamaExplainer::new("", "").is_available());
        let backend = OllamaExplainer::new("", "qwen3:8b");
        assert!(backend.is_available());
        assert_eq!(backend.name(), "Ollama");
        assert_eq!(backend.model(), "qwen3:8b");
    }

    #[test]
    fn pull_progress_lines_parse() {
        let early: PullProgress =
            serde_json::from_str(r#"{"status": "pulling manifest"}"#).unwrap();
        assert_eq!(early.status.as_deref(), Some("pulling manifest"));
        assert!(early.completed.is_none());

        let mid: PullProgress = serde_json::from_str(
            r#"{"status":"pulling 6a0746a1ec1a","digest":"6a0746a1ec1a","total":2142590208,"completed":241970}"#,
        )
        .unwrap();
        assert_eq!(mid.total, Some(2142590208));
        assert_eq!(mid.completed, Some(241970));

        let err: PullProgress =
            serde_json::from_str(r#"{"error":"pull model manifest: file does not exist"}"#)
                .unwrap();
        assert!(err.error.is_some());
    }

    #[test]
    fn models_response_parses() {
        let json = r#"{
            "model": "llama3.2:latest",
            "size": 2019393189,
            "details": {"parameter_size": "3.2B", "quantization_level": "Q4_K_M"}
        }"#;
        let m: OllamaModel = serde_json::from_str(json).unwrap();
        assert_eq!(m.model, "llama3.2:latest");
        assert_eq!(m.details.unwrap().parameter_size.as_deref(), Some("3.2B"));
    }
}
