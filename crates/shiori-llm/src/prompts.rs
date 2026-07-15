//! Prompt construction (pure, unit-tested) and offline writing prompts.
//!
//! Every builder takes the language's [`PromptProfile`] so the same
//! machinery serves Japanese, Koine Greek, and whatever comes next.

use shiori_lang::PromptProfile;

/// What the user is looking at when they ask for an explanation.
#[derive(Debug, Clone)]
pub struct SentenceContext {
    /// The sentence to explain.
    pub sentence: String,
    /// The word the user clicked, if any.
    pub focus_word: Option<String>,
    /// Rough self-assessed level, e.g. "beginner", "intermediate".
    pub learner_level: Option<String>,
    /// Language fragments for prompt construction. Defaults to Japanese;
    /// callers set it from the active language service.
    pub profile: PromptProfile,
}

impl Default for SentenceContext {
    fn default() -> Self {
        Self {
            sentence: String::new(),
            focus_word: None,
            learner_level: None,
            profile: PromptProfile::japanese(),
        }
    }
}

impl SentenceContext {
    pub fn new(sentence: impl Into<String>) -> Self {
        Self {
            sentence: sentence.into(),
            ..Default::default()
        }
    }

    pub fn with_focus(mut self, word: impl Into<String>) -> Self {
        self.focus_word = Some(word.into());
        self
    }

    pub fn with_profile(mut self, profile: PromptProfile) -> Self {
        self.profile = profile;
        self
    }
}

/// System prompt shared by the explain and feedback features.
pub fn system_prompt(profile: &PromptProfile) -> String {
    let mut prompt = format!(
        "You are a {name} language tutor inside a reading application. \
The user is reading real {name} text. Answer in English, concisely, for an adult learner. \
Do not use emoji — the reader cannot display them. {citation}",
        name = profile.language_name,
        citation = profile.citation_guidance,
    );
    if let Some(disclaimer) = &profile.synthetic_disclaimer {
        prompt.push(' ');
        prompt.push_str(disclaimer);
    }
    prompt
}

/// Build the user prompt for a sentence explanation.
pub fn build_explain_prompt(context: &SentenceContext) -> String {
    let profile = &context.profile;
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "Explain this {} sentence — not just what it means, but why it is constructed the way it is:\n\n",
        profile.language_name
    ));
    prompt.push_str(&context.sentence);
    prompt.push_str(&format!(
        "\n\nCover: overall meaning, the grammatical skeleton ({}), and any register or nuance worth knowing.",
        profile.grammar_skeleton
    ));
    if let Some(word) = &context.focus_word {
        prompt.push_str(&format!(
            "\nPay special attention to {}{word}{}: what it contributes here and how it is used in general.",
            profile.quote_open, profile.quote_close
        ));
    }
    if let Some(level) = &context.learner_level {
        prompt.push_str(&format!("\nPitch the explanation at a {level} learner."));
    }
    // The reader renders Markdown, so light structure reads better than a wall
    // of text. Tables render poorly (their cells do not wrap), so ask for lists
    // instead.
    prompt.push_str(
        "\n\nFormat the answer with light Markdown — short headings, **bold** for key terms, \
and bullet or numbered lists. Do not use tables; their cells will not wrap here, so use lists \
to lay out comparisons instead.",
    );
    prompt
}

/// Build the user prompt for production-mode feedback.
pub fn build_feedback_prompt(
    profile: &PromptProfile,
    writing_prompt: &str,
    user_text: &str,
) -> String {
    format!(
        "The learner was given this writing prompt:\n\n{writing_prompt}\n\n\
They wrote:\n\n{user_text}\n\n\
Give feedback on naturalness. Point out anything ungrammatical, then anything grammatical \
but unnatural (word choice, register, {authority}), and suggest \
a natural rewrite. Encourage what they got right. Keep it under 250 words. Use plain text.",
        authority = profile.unnatural_authority,
    )
}

/// The chat message that requests a composition exercise. Sent through
/// the ordinary chat pipeline: the partner replies with a topic, the
/// learner writes, and the usual paper-style write-up grades it.
pub fn composition_request(profile: &PromptProfile) -> String {
    format!(
        "Please give me a short composition exercise: one topic to write \
         2–3 sentences about in {name}. State the topic in {name} with a \
         brief English hint in parentheses, then wait for my attempt.",
        name = profile.language_name
    )
}

/// The chat message that requests a translation drill over a sentence
/// from the user's own reading.
pub fn translation_drill_request(profile: &PromptProfile, sentence: &str) -> String {
    format!(
        "Translation drill: here is a sentence from my reading:\n\n{sentence}\n\n\
         Give me a natural English translation of it, then ask me to \
         translate that English back into {name} without looking. After \
         my attempt, compare it with the original.",
        name = profile.language_name
    )
}

/// Built-in writing prompts so production mode has material without any
/// network dependency.
pub fn writing_prompts() -> &'static [&'static str] {
    &[
        "今日は何をしましたか。簡単に説明してください。",
        "好きな食べ物について書いてください。なぜ好きですか。",
        "あなたの町を友達に紹介してください。",
        "子供の時の思い出を一つ書いてください。",
        "明日の予定について書いてください。",
        "最近読んだ本や見た映画について、感想を書いてください。",
        "雨の日は何をしますか。",
        "あなたにとって大切な物について書いてください。",
        "日本に行ったら、何をしたいですか。",
        "朝のルーティンを説明してください。",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_prompt_includes_sentence_and_focus() {
        let ctx = SentenceContext::new("猫がソファーで寝ている。").with_focus("ソファー");
        let prompt = build_explain_prompt(&ctx);
        assert!(prompt.contains("猫がソファーで寝ている。"));
        assert!(prompt.contains("「ソファー」"));
        assert!(prompt.contains("why it is constructed"));
    }

    #[test]
    fn explain_prompt_without_focus_has_no_focus_clause() {
        let prompt = build_explain_prompt(&SentenceContext::new("行く。"));
        assert!(!prompt.contains("special attention"));
    }

    #[test]
    fn feedback_prompt_embeds_both_texts() {
        let prompt = build_feedback_prompt(
            &PromptProfile::japanese(),
            "好きな食べ物は？",
            "私は寿司が好きです。",
        );
        assert!(prompt.contains("好きな食べ物は？"));
        assert!(prompt.contains("私は寿司が好きです。"));
        assert!(prompt.contains("naturalness"));
        assert!(prompt.contains("phrasing a native speaker would not use"));
    }

    #[test]
    fn system_prompt_is_profile_driven() {
        let ja = system_prompt(&PromptProfile::japanese());
        assert!(ja.contains("You are a Japanese language tutor"));
        assert!(ja.contains("reading real Japanese text"));
        assert!(ja.contains("in Japanese script followed by a reading in parentheses"));
    }

    #[test]
    fn writing_prompts_are_nonempty_japanese() {
        let prompts = writing_prompts();
        assert!(prompts.len() >= 5);
        for p in prompts {
            assert!(!p.is_empty());
        }
    }
}
