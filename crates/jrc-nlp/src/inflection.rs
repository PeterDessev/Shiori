//! Phrase grouping and inflection analysis.
//!
//! The morphological analyzer splits 読んでいる into 読ん／で／いる. For
//! display and lookup the reader wants the whole conjugated unit treated
//! as one phrase, and wants to know *what form* it is. This module groups
//! tokens into such phrases and names the grammar of the tail.

use jrc_core::{PartOfSpeech, Token};

/// Verbal suffix lemmas IPADIC tags as 動詞,接尾 (mapped to `Suffix`).
/// Only these continue a verb chain; noun suffixes like 版 do not.
const VERBAL_SUFFIXES: &[&str] = &["れる", "られる", "せる", "させる"];

/// Connective particles that glue conjugations together (読ん＋で＋いる).
const CONNECTIVE_PARTICLES: &[&str] = &["て", "で", "ちゃ", "じゃ", "たり", "だり"];

/// Split a sentence's tokens into phrase groups; returns `(start, end)`
/// half-open ranges covering all tokens in order.
///
/// - A verb/adjective/adjectival-noun starts a chain that absorbs
///   auxiliary verbs, verbal suffixes (れる…), and connective て/で.
/// - A noun absorbs a directly following noun-suffix (日本語＋版) and
///   chains of nouns are left alone (the analyzer already compounds most).
/// - Everything else is its own group.
pub fn phrase_groups(tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let end = phrase_end(tokens, i);
        groups.push((i, end));
        i = end;
    }
    groups
}

/// End (exclusive) of the phrase starting at `start`.
fn phrase_end(tokens: &[Token], start: usize) -> usize {
    let head = &tokens[start];
    let mut end = start + 1;

    match head.pos {
        PartOfSpeech::Verb | PartOfSpeech::Adjective | PartOfSpeech::AuxiliaryVerb => {
            while end < tokens.len() && continues_verb_chain(&tokens[end]) {
                end += 1;
            }
        }
        PartOfSpeech::AdjectivalNoun | PartOfSpeech::Noun | PartOfSpeech::ProperNoun => {
            // Absorb a directly attached noun suffix: 日本語＋版, 田中＋さん.
            while end < tokens.len()
                && tokens[end].pos == PartOfSpeech::Suffix
                && !VERBAL_SUFFIXES.contains(&tokens[end].lemma.as_str())
            {
                end += 1;
            }
        }
        _ => {}
    }
    end
}

fn continues_verb_chain(token: &Token) -> bool {
    match token.pos {
        PartOfSpeech::AuxiliaryVerb => true,
        PartOfSpeech::Suffix => VERBAL_SUFFIXES.contains(&token.lemma.as_str()),
        PartOfSpeech::Particle => CONNECTIVE_PARTICLES.contains(&token.surface.as_str()),
        _ => false,
    }
}

/// What a conjugated phrase is doing grammatically.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inflection {
    /// Headline for well-known constructions, e.g.
    /// "〜ている — ongoing action or resulting state (te-iru form)".
    pub summary: Option<String>,
    /// One line per grammatical component after the stem, in order.
    pub parts: Vec<String>,
}

impl Inflection {
    pub fn is_plain(&self) -> bool {
        self.summary.is_none() && self.parts.is_empty()
    }
}

/// Describe the inflection of a phrase group (as produced by
/// [`phrase_groups`]). The first token is the stem; the rest are the
/// grammatical tail.
pub fn analyze_inflection(group: &[Token]) -> Inflection {
    if group.len() < 2 {
        return Inflection::default();
    }
    let tail = &group[1..];
    let lemmas: Vec<&str> = tail.iter().map(|t| t.lemma.as_str()).collect();

    let mut parts = Vec::new();
    for token in tail {
        if let Some(desc) = component_meaning(token) {
            parts.push(format!("{} — {desc}", token.surface));
        }
    }

    Inflection {
        summary: combo_summary(&lemmas),
        parts,
    }
}

/// Meaning of one grammatical component, by dictionary form.
fn component_meaning(token: &Token) -> Option<&'static str> {
    let meaning = match token.lemma.as_str() {
        "て" | "で" => "connective (te-form)",
        "ちゃ" | "じゃ" => "contracted ては/では",
        "たり" | "だり" => "representative listing (〜たり)",
        "いる" | "おる" => "ongoing action or resulting state",
        "ある" => "resulting state (intentional)",
        "いく" | "行く" | "ゆく" => "change continuing away/onward (〜ていく)",
        "くる" | "来る" => "change coming up to now (〜てくる)",
        "しまう" => "completion, often with regret",
        "おく" => "doing in advance/preparation",
        "みる" => "trying something out (〜てみる)",
        "ます" => "polite",
        "た" => "past / completed",
        "だ" => "copula (plain)",
        "です" => "copula (polite)",
        "ない" | "ぬ" | "ん" => "negative",
        "たい" => "desire (want to)",
        "れる" => "passive or potential",
        "られる" => "passive or potential",
        "せる" | "させる" => "causative (make/let someone do)",
        "う" | "よう" => "volitional / let's",
        "まい" => "negative volitional",
        "らしい" => "seeming / hearsay",
        "そう" => "appearance (looks like)",
        "べし" => "obligation (classical)",
        "くれる" => "doing for the speaker",
        "もらう" => "receiving the action",
        "あげる" | "やる" => "doing for someone else",
        "ください" | "くださる" => "polite request",
        _ => return None,
    };
    Some(meaning)
}

/// Headline summaries for frequent multi-token constructions. `lemmas` is
/// the tail (everything after the stem), in order.
fn combo_summary(lemmas: &[&str]) -> Option<String> {
    let has = |l: &str| lemmas.contains(&l);
    let te = has("て") || has("で");

    let core: &str = if te && (has("いる") || has("おる")) {
        "〜ている — ongoing action or resulting state (te-iru form)"
    } else if te && has("しまう") {
        "〜てしまう — completely done, often with regret"
    } else if te && has("おく") {
        "〜ておく — done in advance / left in place"
    } else if te && has("みる") {
        "〜てみる — try doing"
    } else if te && has("ある") {
        "〜てある — left in a (deliberately created) state"
    } else if te && (has("ください") || has("くださる")) {
        "〜てください — polite request"
    } else if te && has("くる") {
        "〜てくる — change/movement up to now"
    } else if te && has("いく") {
        "〜ていく — change/movement onward"
    } else if (has("せる") || has("させる")) && has("られる") {
        "causative-passive — was made to do"
    } else if has("せる") || has("させる") {
        "causative — make/let someone do"
    } else if has("れる") || has("られる") {
        "passive or potential"
    } else if has("たい") {
        "〜たい — want to"
    } else if has("ます") && has("ん") && has("です") && has("た") {
        "〜ませんでした — polite negative past"
    } else if has("ます") && has("た") {
        "〜ました — polite past"
    } else if has("ます") && (has("ん") || has("ない")) {
        "〜ません — polite negative"
    } else if has("ます") {
        "〜ます — polite"
    } else if has("ない") && has("た") {
        "〜なかった — negative past"
    } else if has("ない") || has("ぬ") {
        "negative"
    } else if has("う") || has("よう") {
        "volitional — let's / shall"
    } else if has("た") {
        "plain past"
    } else if lemmas == ["て"] || lemmas == ["で"] {
        "te-form — connective"
    } else {
        return None;
    };

    let mut out = core.to_string();
    // Add a past marker when it is not already part of the headline.
    if has("た") && !out.contains("past") {
        out.push_str(" + past");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Analyzer;
    use std::sync::OnceLock;

    fn analyzer() -> &'static Analyzer {
        static A: OnceLock<Analyzer> = OnceLock::new();
        A.get_or_init(|| Analyzer::new().expect("embedded dictionary should load"))
    }

    /// Tokenize and return (groups, tokens).
    fn groups_of(sentence: &str) -> (Vec<(usize, usize)>, Vec<Token>) {
        let tokens = analyzer().tokenize_sentence(sentence).unwrap();
        let groups = phrase_groups(&tokens);
        (groups, tokens)
    }

    /// Surface of the group containing token index `idx`.
    fn group_surface(sentence: &str, contains: &str) -> (String, Inflection) {
        let (groups, tokens) = groups_of(sentence);
        let idx = tokens
            .iter()
            .position(|t| t.surface == contains)
            .unwrap_or_else(|| panic!("token {contains} not found in {sentence}"));
        let (start, end) = *groups
            .iter()
            .find(|(s, e)| (*s..*e).contains(&idx))
            .unwrap();
        let surface: String = tokens[start..end].iter().map(|t| t.surface.as_str()).collect();
        (surface, analyze_inflection(&tokens[start..end]))
    }

    #[test]
    fn groups_cover_all_tokens_in_order() {
        let (groups, tokens) = groups_of("猫が魚を食べました。");
        let mut covered = 0;
        for (s, e) in &groups {
            assert_eq!(*s, covered);
            assert!(e > s);
            covered = *e;
        }
        assert_eq!(covered, tokens.len());
    }

    #[test]
    fn te_iru_groups_and_is_identified() {
        let (surface, inflection) = group_surface("本を読んでいる。", "読ん");
        assert_eq!(surface, "読んでいる");
        let summary = inflection.summary.expect("te-iru should have a summary");
        assert!(summary.contains("ている"), "{summary}");
        assert!(summary.contains("te-iru"), "{summary}");
        assert!(!inflection.parts.is_empty());
    }

    #[test]
    fn te_ita_marks_past_too() {
        let (surface, inflection) = group_surface("本を読んでいた。", "読ん");
        assert_eq!(surface, "読んでいた");
        let summary = inflection.summary.unwrap();
        assert!(summary.contains("ている"), "{summary}");
        assert!(summary.contains("past"), "{summary}");
    }

    #[test]
    fn polite_past_is_identified() {
        let (surface, inflection) = group_surface("魚を食べました。", "食べ");
        assert_eq!(surface, "食べました");
        assert!(inflection.summary.unwrap().contains("ました"));
    }

    #[test]
    fn negative_past_is_identified() {
        let (surface, inflection) = group_surface("学校に行かなかった。", "行か");
        assert_eq!(surface, "行かなかった");
        assert!(inflection.summary.unwrap().contains("なかった"));
    }

    #[test]
    fn passive_is_identified() {
        let (surface, inflection) = group_surface("手紙が書かれた。", "書か");
        assert_eq!(surface, "書かれた");
        let summary = inflection.summary.unwrap();
        assert!(summary.contains("passive"), "{summary}");
        assert!(summary.contains("past"), "{summary}");
    }

    #[test]
    fn causative_is_identified() {
        let (surface, inflection) = group_surface("野菜を食べさせる。", "食べ");
        assert_eq!(surface, "食べさせる");
        assert!(inflection.summary.unwrap().contains("causative"));
    }

    #[test]
    fn desiderative_is_identified() {
        let (surface, inflection) = group_surface("水を飲みたい。", "飲み");
        assert_eq!(surface, "飲みたい");
        assert!(inflection.summary.unwrap().contains("たい"));
    }

    #[test]
    fn te_shimau_past_is_identified() {
        let (surface, inflection) = group_surface("全部読んでしまった。", "読ん");
        assert_eq!(surface, "読んでしまった");
        let summary = inflection.summary.unwrap();
        assert!(summary.contains("しまう"), "{summary}");
    }

    #[test]
    fn noun_plus_suffix_groups() {
        let (surface, inflection) = group_surface("日本語版を読む。", "版");
        assert_eq!(surface, "日本語版");
        // Noun compounds are not conjugations.
        assert!(inflection.summary.is_none());
        let (surface, _) = group_surface("田中さんが来た。", "さん");
        assert_eq!(surface, "田中さん");
    }

    #[test]
    fn case_particle_de_does_not_join_noun() {
        // で after a noun is a case particle, not a connective.
        let (groups, tokens) = groups_of("学校で勉強した。");
        let de = tokens.iter().position(|t| t.surface == "で").unwrap();
        let de_group = groups.iter().find(|(s, e)| (*s..*e).contains(&de)).unwrap();
        assert_eq!(de_group.1 - de_group.0, 1, "で must stand alone after 学校");
        // …while the verb still groups: 勉強した = 勉強(noun) し(verb) た(aux).
        let shi = tokens.iter().position(|t| t.surface == "し").unwrap();
        let shi_group = groups.iter().find(|(s, e)| (*s..*e).contains(&shi)).unwrap();
        let surface: String = tokens[shi_group.0..shi_group.1]
            .iter()
            .map(|t| t.surface.as_str())
            .collect();
        assert_eq!(surface, "した");
    }

    #[test]
    fn plain_words_have_no_inflection() {
        let (surface, inflection) = group_surface("猫がいる。", "猫");
        assert_eq!(surface, "猫");
        assert!(inflection.is_plain());
    }

    #[test]
    fn adjective_negative_groups() {
        let (surface, inflection) = group_surface("高くない。", "高く");
        assert_eq!(surface, "高くない");
        assert!(inflection.summary.unwrap().contains("negative"));
    }
}
