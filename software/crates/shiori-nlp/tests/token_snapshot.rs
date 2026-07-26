//! Golden snapshot of the analyzer's stored-token output.
//!
//! The multilingual refactor moves this analyzer behind a `LanguageService`
//! trait; this fixture pins the exact tokens (surfaces, lemmas, readings,
//! POS, byte spans) and phrase groups produced for representative Japanese
//! text, so the trait extraction can prove itself byte-identical.
//!
//! Regenerate deliberately with:
//! `UPDATE_FIXTURES=1 cargo test -p shiori-nlp --test token_snapshot`

use shiori_nlp::Analyzer;

/// Texts chosen to exercise the seams that must not move: conjugated verb
/// chains, katakana readings, quotes and brackets, sentence enders, kana
/// fallbacks, and paragraph splits.
const TEXTS: &[&str] = &[
    "猫が好きだ。犬も好きだ。",
    "彼は本を読んでいるところだった。「面白い！」と言った。",
    "東京タワーへ行きました。パーティーが始まる？",
    "食べさせられたくなかった。",
    "雨が降る。\n\n風も吹く。すごく強い風だ。",
];

fn snapshot() -> serde_json::Value {
    let analyzer = Analyzer::new().expect("analyzer builds");
    let mut out = Vec::new();
    for text in TEXTS {
        let analyzed = analyzer.analyze(text).expect("analysis succeeds");
        let paragraphs: Vec<serde_json::Value> = analyzed
            .paragraphs
            .iter()
            .map(|p| {
                let sentences: Vec<serde_json::Value> = p
                    .sentences
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "text": s.text,
                            "tokens": s.tokens,
                            "phrase_groups": shiori_nlp::phrase_groups(&s.tokens),
                        })
                    })
                    .collect();
                serde_json::json!({ "sentences": sentences })
            })
            .collect();
        out.push(serde_json::json!({
            "input": text,
            "paragraphs": paragraphs,
        }));
    }
    serde_json::Value::Array(out)
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tokens_ja.json")
}

#[test]
fn analyzer_output_matches_fixture() {
    let actual = snapshot();
    let path = fixture_path();

    if std::env::var("UPDATE_FIXTURES").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&actual).unwrap()).unwrap();
        return;
    }

    let raw = std::fs::read_to_string(&path).expect(
        "fixture missing — regenerate with UPDATE_FIXTURES=1 cargo test -p shiori-nlp \
         --test token_snapshot",
    );
    let expected: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        actual, expected,
        "analyzer token output changed; if intentional, regenerate the fixture"
    );
}
