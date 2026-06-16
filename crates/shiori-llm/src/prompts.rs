//! Prompt construction (pure, unit-tested) and offline writing prompts.

/// What the user is looking at when they ask for an explanation.
#[derive(Debug, Clone, Default)]
pub struct SentenceContext {
    /// The sentence to explain.
    pub sentence: String,
    /// The word the user clicked, if any.
    pub focus_word: Option<String>,
    /// Rough self-assessed level, e.g. "beginner", "intermediate".
    pub learner_level: Option<String>,
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
}

/// System prompt shared by both features.
pub const SYSTEM_PROMPT: &str = "You are a Japanese language tutor inside a reading application. \
The user is reading real Japanese text. Answer in English, concisely, for an adult learner. \
Do not use emoji — the reader cannot display them. When you cite Japanese, give it in Japanese \
script followed by a reading in parentheses where helpful.";

/// Build the user prompt for a sentence explanation.
pub fn build_explain_prompt(context: &SentenceContext) -> String {
    let mut prompt = String::new();
    prompt.push_str("Explain this Japanese sentence — not just what it means, but why it is constructed the way it is:\n\n");
    prompt.push_str(&context.sentence);
    prompt.push_str("\n\nCover: overall meaning, the grammatical skeleton (particles, verb forms, clause structure), and any register or nuance worth knowing.");
    if let Some(word) = &context.focus_word {
        prompt.push_str(&format!(
            "\nPay special attention to 「{word}」: what it contributes here and how it is used in general."
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
pub fn build_feedback_prompt(writing_prompt: &str, user_text: &str) -> String {
    format!(
        "The learner was given this writing prompt:\n\n{writing_prompt}\n\n\
They wrote:\n\n{user_text}\n\n\
Give feedback on naturalness. Point out anything ungrammatical, then anything grammatical \
but unnatural (word choice, register, phrasing a native speaker would not use), and suggest \
a natural rewrite. Encourage what they got right. Keep it under 250 words. Use plain text."
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
        let prompt = build_feedback_prompt("好きな食べ物は？", "私は寿司が好きです。");
        assert!(prompt.contains("好きな食べ物は？"));
        assert!(prompt.contains("私は寿司が好きです。"));
        assert!(prompt.contains("naturalness"));
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
