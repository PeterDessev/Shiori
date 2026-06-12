//! End-to-end integration tests: real morphological analyzer, in-memory
//! database, fixture dictionary and frequency list.

use jrc_app::App;
use jrc_core::KnowledgeStatus;
use jrc_db::Db;
use jrc_srs::{CardState, Rating};

/// Minimal jmdict-simplified fixture covering the test document's words.
const DICT_FIXTURE: &str = r#"{
  "version": "test",
  "tags": {"hon": "honorific", "uk": "usually kana", "col": "colloquial"},
  "words": [
    {"id": "1358280",
     "kanji": [{"common": true, "text": "食べる", "tags": []}],
     "kana": [{"common": true, "text": "たべる", "tags": []}],
     "sense": [{"partOfSpeech": ["v1"], "misc": [],
                "related": [["食う","くう"]],
                "gloss": [{"text": "to eat"}]}]},
    {"id": "1467640",
     "kanji": [{"common": true, "text": "猫", "tags": []}],
     "kana": [{"common": true, "text": "ねこ", "tags": []}],
     "sense": [{"partOfSpeech": ["n"], "misc": [],
                "gloss": [{"text": "cat"}]}]},
    {"id": "1245290",
     "kanji": [{"common": true, "text": "空", "tags": []}],
     "kana": [{"common": true, "text": "そら", "tags": []}],
     "sense": [{"partOfSpeech": ["n"], "misc": [],
                "gloss": [{"text": "sky"}]}]},
    {"id": "1577100",
     "kanji": [{"common": false, "text": "召し上がる", "tags": []}],
     "kana": [{"common": true, "text": "めしあがる", "tags": []}],
     "sense": [{"partOfSpeech": ["v5r"], "misc": ["hon"],
                "gloss": [{"text": "to eat (honorific)"}]}]}
  ]
}"#;

const FREQ_FIXTURE: &str = "の\nに\nは\n猫\n食べる\n空\n";

const TEXT: &str = "猫が魚を食べました。猫は空を見た。\n\n空は青い。";

fn app() -> App {
    let app = App::with_db(Db::open_in_memory().unwrap(), std::env::temp_dir())
        .expect("analyzer should initialize");
    app.import_dictionary_json(DICT_FIXTURE).unwrap();
    app.import_frequency_text(FREQ_FIXTURE).unwrap();
    app.db()
        .import_kanji(vec![jrc_db::KanjiRow {
            literal: "猫".into(),
            grade: Some(8),
            stroke_count: 11,
            jlpt: Some(2),
            freq: None,
            on_readings: vec!["ビョウ".into()],
            kun_readings: vec!["ねこ".into()],
            nanori: vec![],
            meanings: vec!["cat".into()],
            variants: vec![],
            strokes: vec![],
        }])
        .unwrap();
    assert!(app.data_status().unwrap().is_ready());
    app
}

#[test]
fn import_preserves_structure_and_dedupes() {
    let app = app();
    let doc = app.import_text("テスト", TEXT).unwrap();

    let docs = app.db().list_documents().unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].document.title, "テスト");
    assert_eq!(docs[0].sentence_count, 3);

    let sentences = app.db().sentences(doc).unwrap();
    assert_eq!(sentences[0].text, "猫が魚を食べました。");
    assert_eq!(sentences[0].paragraph, 0);
    assert_eq!(sentences[2].text, "空は青い。");
    assert_eq!(sentences[2].paragraph, 1);

    // Re-importing identical content must not duplicate.
    let again = app.import_text("コピー", TEXT).unwrap();
    assert_eq!(again, doc);
    assert_eq!(app.db().list_documents().unwrap().len(), 1);
}

#[test]
fn import_rejects_garbage() {
    let app = app();
    assert!(app.import_text("", TEXT).is_err(), "empty title");
    assert!(app.import_text("x", "").is_err(), "empty text");
}

#[test]
fn mining_ranks_and_filters() {
    let app = app();
    let doc = app.import_text("テスト", TEXT).unwrap();
    let candidates = app.mining_candidates(doc).unwrap();

    // Function words (が, を, は, ました…) must never appear.
    assert!(candidates
        .iter()
        .all(|c| c.word.key.pos.is_content_word()), "{candidates:?}");

    // 猫 (2 occurrences, corpus rank 4) should beat 魚 (1 occurrence,
    // unlisted).
    let neko_pos = candidates
        .iter()
        .position(|c| c.word.key.lemma == "猫")
        .expect("猫 must be a candidate");
    let sakana_pos = candidates
        .iter()
        .position(|c| c.word.key.lemma == "魚")
        .expect("魚 must be a candidate");
    assert!(neko_pos < sakana_pos);

    // 食べました was lemmatized: the candidate is 食べる with a gloss.
    let taberu = candidates
        .iter()
        .find(|c| c.word.key.lemma == "食べる")
        .expect("食べる must be a candidate");
    assert_eq!(taberu.word.key.reading, "たべる");
    let entry = taberu.entry.as_ref().expect("dictionary entry resolved");
    assert_eq!(entry.short_gloss(), "to eat");
    assert_eq!(taberu.corpus_rank, Some(5));

    // Context sentence is the word's first occurrence.
    let neko = &candidates[neko_pos];
    assert!(neko.sentence.text.contains('猫'));
    assert_eq!(neko.occurrences, 2);
}

#[test]
fn full_srs_cycle_updates_knowledge() {
    let app = app();
    let doc = app.import_text("テスト", TEXT).unwrap();
    let candidates = app.mining_candidates(doc).unwrap();
    let neko = candidates
        .iter()
        .find(|c| c.word.key.lemma == "猫")
        .unwrap();

    // Start learning: card exists, status flips, due immediately.
    app.start_learning(neko.word.id, neko.sentence.id).unwrap();
    assert_eq!(
        app.db().word(neko.word.id).unwrap().status,
        KnowledgeStatus::Learning
    );
    assert_eq!(app.due_count().unwrap(), 1);

    // Starting again is a no-op.
    app.start_learning(neko.word.id, neko.sentence.id).unwrap();
    assert_eq!(app.db().card_count().unwrap(), 1);

    // The queue item carries the context sentence and the gloss.
    let queue = app.due_reviews(10).unwrap();
    assert_eq!(queue.len(), 1);
    let item = &queue[0];
    assert_eq!(item.word.key.lemma, "猫");
    assert!(item.sentence.as_ref().unwrap().text.contains('猫'));
    assert_eq!(item.entry.as_ref().unwrap().short_gloss(), "cat");
    assert_eq!(item.card.state, CardState::New);

    // Answer Good: card schedules into a learning step, review is logged.
    let card = app.answer_review(neko.word.id, Rating::Good).unwrap();
    assert_eq!(card.state, CardState::Learning);
    assert_eq!(app.db().review_count().unwrap(), 1);
    assert_eq!(app.due_count().unwrap(), 0, "card moved into the future");

    // Mining no longer offers the word.
    assert!(app
        .mining_candidates(doc)
        .unwrap()
        .iter()
        .all(|c| c.word.key.lemma != "猫"));

    // Answering a card that does not exist is an error, not a panic.
    assert!(app.answer_review(jrc_core::WordId(99999), Rating::Good).is_err());
}

#[test]
fn marking_known_and_ignored_moves_stats() {
    let app = app();
    let doc = app.import_text("テスト", TEXT).unwrap();

    let before = app.document_stats(doc).unwrap();
    assert_eq!(before.known_tokens, 0);
    assert!(before.unknown_tokens > 0);
    assert_eq!(before.unknown_share(), 1.0, "everything starts unknown");

    // Mark 猫 known and 魚 ignored.
    let candidates = app.mining_candidates(doc).unwrap();
    let neko = candidates.iter().find(|c| c.word.key.lemma == "猫").unwrap();
    let sakana = candidates.iter().find(|c| c.word.key.lemma == "魚").unwrap();
    app.mark_known(neko.word.id).unwrap();
    app.ignore_word(sakana.word.id).unwrap();

    let after = app.document_stats(doc).unwrap();
    assert_eq!(after.known_tokens, 2, "猫 occurs twice");
    assert_eq!(after.ignored_tokens, 1);
    assert!(after.known_share() > before.known_share());
    assert!(after.unknown_share() < 1.0);
    assert_eq!(after.content_tokens, before.content_tokens);

    // Reset puts the word back.
    app.reset_word(neko.word.id).unwrap();
    let reset = app.document_stats(doc).unwrap();
    assert_eq!(reset.known_tokens, 0);
}

#[test]
fn forgotten_words_reenter_rotation() {
    let app = app();
    let doc = app.import_text("テスト", TEXT).unwrap();
    let candidates = app.mining_candidates(doc).unwrap();
    let neko = candidates.iter().find(|c| c.word.key.lemma == "猫").unwrap();
    let (word_id, sentence_id) = (neko.word.id, neko.sentence.id);

    // Learn it, answer until it is well known, then mark it known.
    app.start_learning(word_id, sentence_id).unwrap();
    app.answer_review(word_id, Rating::Easy).unwrap();
    app.mark_known(word_id).unwrap();
    assert_eq!(
        app.db().word(word_id).unwrap().status,
        KnowledgeStatus::Known
    );
    assert_eq!(app.due_count().unwrap(), 0);

    // Forgot it: a fresh card is due immediately, status back to learning.
    app.mark_forgotten(word_id, Some(sentence_id)).unwrap();
    assert_eq!(
        app.db().word(word_id).unwrap().status,
        KnowledgeStatus::Learning
    );
    assert_eq!(app.due_count().unwrap(), 1);
    let queue = app.due_reviews(10).unwrap();
    assert_eq!(queue[0].word.id, word_id);
    assert_eq!(queue[0].card.state, CardState::New, "fresh card, not the old one");
    assert!(queue[0].sentence.is_some(), "context sentence preserved");
}

#[test]
fn recommendations_prefer_sweet_spot() {
    let app = app();
    // Document A: will be made ~fully known (comfortable).
    let doc_a = app.import_text("A", "猫は空を見た。").unwrap();
    // Document B: completely unknown (too hard).
    let doc_b = app.import_text("B", "猫が魚を食べました。空は青い。").unwrap();

    // Make everything in A known.
    for w in app.db().document_words(doc_a).unwrap() {
        if w.word.key.pos.is_content_word() {
            app.mark_known(w.word.id).unwrap();
        }
    }

    let recs = app.recommendations().unwrap();
    assert_eq!(recs.len(), 2);
    // A (0% unknown) is closer to the 3.5% ideal than B (100% unknown).
    assert_eq!(recs[0].summary.document.id, doc_a);
    assert_eq!(recs[1].summary.document.id, doc_b);
    assert!(recs[0].score < recs[1].score);
    assert_eq!(
        recs[1].stats.band,
        jrc_app::DifficultyBand::TooHard
    );
}

#[test]
fn honorific_register_is_surfaced() {
    let app = app();
    let doc = app.import_text("敬語", "先生は召し上がりました。").unwrap();
    let candidates = app.mining_candidates(doc).unwrap();
    let meshiagaru = candidates
        .iter()
        .find(|c| c.word.key.lemma == "召し上がる")
        .expect("召し上がりました lemmatizes to 召し上がる");
    let entry = meshiagaru.entry.as_ref().unwrap();
    let profile = jrc_dict::register::UsageProfile::from_misc_codes(entry.misc_codes());
    assert_eq!(
        profile.registers,
        vec![jrc_dict::register::Register::Honorific]
    );
}
