//! Conversation practice: prompt construction and response parsing.
//!
//! One model call per user message returns both the conversational reply
//! (a native speaker chatting, never correcting) and the paper-style
//! write-up of the user's message. Annotations come back as exact quotes
//! rather than character offsets — models can't count characters, but
//! they can copy substrings — and are located in the text here.

use serde::Deserialize;
use shiori_lang::PromptProfile;

use crate::LlmError;

/// One message of chat history sent to the model.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
}

impl ChatRole {
    pub fn as_str(self) -> &'static str {
        match self {
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        }
    }
}

/// How hard the partner's language should push the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Challenge {
    /// Stay at the user's comfortable level.
    Match,
    /// Slightly above: comprehensible input with stretch.
    Push,
    /// Natural unrestricted text, no simplification.
    Immerse,
}

impl Challenge {
    fn instruction(self, profile: &PromptProfile) -> String {
        match self {
            Challenge::Match => "Match the user's level closely: use vocabulary and grammar they \
                 have shown they can handle."
                .to_string(),
            Challenge::Push => "Aim slightly above the user's level: mostly comprehensible, with \
                 a few new words or patterns per reply that context makes clear."
                .to_string(),
            Challenge::Immerse => profile.immerse_instruction.clone(),
        }
    }
}

/// A located write-up span over the user's message (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAnnotation {
    pub start: usize,
    pub end: usize,
    pub severity: AnnotationSeverity,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationSeverity {
    /// Grammatically wrong.
    Error,
    /// Correct but unnatural or clunky.
    Awkward,
}

impl AnnotationSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            AnnotationSeverity::Error => "error",
            AnnotationSeverity::Awkward => "awkward",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "error" => AnnotationSeverity::Error,
            _ => AnnotationSeverity::Awkward,
        }
    }
}

/// Parsed outcome of one chat turn.
#[derive(Debug, Clone)]
pub struct ChatTurnOutcome {
    pub reply: String,
    pub annotations: Vec<ChatAnnotation>,
}

/// System prompt for the conversation partner.
///
/// `level_hint` describes the user's recorded vocabulary; the model is
/// told to weigh the user's actual messages more heavily, so a small
/// recorded vocabulary never caps the conversation. For dead languages
/// the profile's synthetic disclaimer reframes the persona and the
/// correction authority around attested usage.
pub fn chat_system_prompt(
    profile: &PromptProfile,
    level_hint: &str,
    challenge: Challenge,
) -> String {
    let disclaimer = profile
        .synthetic_disclaimer
        .as_ref()
        .map(|d| format!("\n\n{d}"))
        .unwrap_or_default();
    format!(
        "You are {persona} having a written \
         conversation with a learner. Reply ONLY in {name}, naturally and \
         engagingly, as a conversation partner — ask follow-ups, react, share \
         your own thoughts. Keep replies to a few sentences.\n\
         NEVER correct, grade, or comment on the user's {name} inside your \
         reply; respond to what they *meant*, the way a friend would.\n\
         \n\
         Level guidance: {level_hint} This estimate may lag reality — the \
         user's own messages are the better signal; adapt to what they \
         actually write. {challenge}{disclaimer}\n\
         \n\
         Separately from the conversation, write up the user's LATEST message \
         like a teacher marking a paper. For each grammatically wrong or \
         unnatural/clunky span, produce an annotation with: \"quote\" — the \
         EXACT substring copied verbatim from the user's message (it must \
         appear character-for-character); \"severity\" — \"error\" for \
         grammar mistakes, \"awkward\" for correct-but-unnatural phrasing; \
         \"note\" — a short English explanation with the natural alternative. \
         If the message is fine, return an empty annotations array.\n\
         \n\
         Respond with ONLY this JSON, no other text:\n\
         {{\"reply\": \"...\", \"annotations\": [{{\"quote\": \"...\", \
         \"severity\": \"error\", \"note\": \"...\"}}]}}",
        persona = profile.chat_persona,
        name = profile.language_name,
        level_hint = level_hint,
        challenge = challenge.instruction(profile),
        disclaimer = disclaimer,
    )
}

#[derive(Deserialize)]
struct RawOutcome {
    reply: String,
    #[serde(default)]
    annotations: Vec<RawAnnotation>,
}

#[derive(Deserialize)]
struct RawAnnotation {
    quote: String,
    #[serde(default)]
    severity: Option<String>,
    #[serde(default)]
    note: String,
}

/// Parse a model response into a reply plus annotations located in
/// `user_text`. Quotes that can't be found verbatim are dropped (better
/// no underline than a wrong one). Repeated quotes anchor to successive
/// occurrences.
pub fn parse_chat_response(raw: &str, user_text: &str) -> Result<ChatTurnOutcome, LlmError> {
    let json = extract_json_object(raw)
        .ok_or_else(|| LlmError::Response("no JSON object in chat response".into()))?;
    let parsed: RawOutcome = serde_json::from_str(json)
        .map_err(|e| LlmError::Response(format!("bad chat JSON: {e}")))?;

    let mut annotations = Vec::new();
    let mut cursor_per_quote: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for a in parsed.annotations {
        if a.quote.is_empty() {
            continue;
        }
        let from = cursor_per_quote.get(&a.quote).copied().unwrap_or(0);
        let found = user_text
            .get(from..)
            .and_then(|hay| hay.find(&a.quote).map(|i| from + i));
        let Some(start) = found else { continue };
        let end = start + a.quote.len();
        cursor_per_quote.insert(a.quote.clone(), end);
        annotations.push(ChatAnnotation {
            start,
            end,
            severity: AnnotationSeverity::from_str_lossy(
                a.severity.as_deref().unwrap_or("awkward"),
            ),
            note: a.note,
        });
    }
    annotations.sort_by_key(|a| a.start);
    Ok(ChatTurnOutcome {
        reply: parsed.reply,
        annotations,
    })
}

/// The first balanced `{ … }` block in a string (tolerates markdown
/// fences and prose around the JSON).
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let user = "今日は天気がいいですから、公園に行きました。";
        let raw = r#"{"reply": "いいですね！どの公園ですか？", "annotations": [
            {"quote": "いいですから", "severity": "awkward", "note": "ので reads more naturally here"}
        ]}"#;
        let outcome = parse_chat_response(raw, user).unwrap();
        assert_eq!(outcome.reply, "いいですね！どの公園ですか？");
        assert_eq!(outcome.annotations.len(), 1);
        let a = &outcome.annotations[0];
        assert_eq!(&user[a.start..a.end], "いいですから");
        assert_eq!(a.severity, AnnotationSeverity::Awkward);
    }

    #[test]
    fn tolerates_markdown_fences_and_prose() {
        let raw = "Sure! Here's the JSON:\n```json\n{\"reply\": \"こんにちは\", \"annotations\": []}\n```";
        let outcome = parse_chat_response(raw, "やあ").unwrap();
        assert_eq!(outcome.reply, "こんにちは");
        assert!(outcome.annotations.is_empty());
    }

    #[test]
    fn unfindable_quotes_are_dropped() {
        let raw = r#"{"reply": "ok", "annotations": [
            {"quote": "ここにない", "severity": "error", "note": "x"}
        ]}"#;
        let outcome = parse_chat_response(raw, "短いテキスト").unwrap();
        assert!(outcome.annotations.is_empty());
    }

    #[test]
    fn repeated_quotes_anchor_to_successive_occurrences() {
        let user = "猫が好き。猫が好き。";
        let raw = r#"{"reply": "ok", "annotations": [
            {"quote": "猫が好き", "severity": "awkward", "note": "first"},
            {"quote": "猫が好き", "severity": "awkward", "note": "second"}
        ]}"#;
        let outcome = parse_chat_response(raw, user).unwrap();
        assert_eq!(outcome.annotations.len(), 2);
        assert!(outcome.annotations[0].start < outcome.annotations[1].start);
        assert_eq!(
            &user[outcome.annotations[1].start..outcome.annotations[1].end],
            "猫が好き"
        );
    }

    #[test]
    fn braces_inside_strings_do_not_confuse_extraction() {
        let raw = r#"{"reply": "this } has a brace", "annotations": []}"#;
        let outcome = parse_chat_response(raw, "x").unwrap();
        assert_eq!(outcome.reply, "this } has a brace");
    }

    #[test]
    fn missing_json_is_an_error() {
        assert!(parse_chat_response("just prose, no json", "x").is_err());
    }

    #[test]
    fn system_prompt_embeds_level_and_challenge() {
        let p = chat_system_prompt(
            &PromptProfile::japanese(),
            "knows ~1200 words (around JLPT N4).",
            Challenge::Push,
        );
        assert!(p.contains("N4"));
        assert!(p.contains("slightly above"));
        assert!(p.contains("NEVER correct"));
        assert!(p.contains("a friendly native Japanese speaker"));
        assert!(p.contains("Reply ONLY in Japanese"));
    }

    #[test]
    fn dead_language_profile_reframes_the_persona() {
        let mut profile = PromptProfile::japanese();
        profile.language_name = "Koine Greek".into();
        profile.chat_persona = "an educated first-century writer of Koine Greek".into();
        profile.synthetic_disclaimer = Some(
            "Koine Greek has no living native speakers; judge naturalness against \
             attested usage in the period's texts."
                .into(),
        );
        profile.immerse_instruction =
            "Write unrestricted literary Koine; the user wants full immersion.".into();
        let p = chat_system_prompt(&profile, "knows ~300 words.", Challenge::Immerse);
        assert!(p.contains("Reply ONLY in Koine Greek"));
        assert!(p.contains("first-century writer"));
        assert!(p.contains("attested usage"));
        assert!(p.contains("unrestricted literary Koine"));
    }
}
