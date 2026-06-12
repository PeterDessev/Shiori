//! End-to-end smoke run against real data.
//!
//! Usage:
//!   cargo run -p shiori-app --example smoke -- <data_dir> <text_file> <title>
//!
//! Downloads (or reuses cached) JMdict + frequency data in `data_dir`,
//! imports the text, and walks the whole loop: stats → mining → start
//! learning → review.

use shiori_srs::Rating;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(data_dir), Some(text_file), Some(title)) = (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: smoke <data_dir> <text_file> <title>");
        std::process::exit(2);
    };

    let app = shiori_app::App::open(std::path::Path::new(&data_dir)).expect("open app");

    let status = app
        .download_and_import_data(|line| println!("[data] {line}"))
        .expect("acquire reference data");
    println!(
        "[data] {} dictionary entries, {} frequency words",
        status.dict_entries, status.frequency_words
    );

    let text = shiori_app::extract::extract_text(std::path::Path::new(&text_file))
        .expect("extract text from file");
    let doc = app.import_text(&title, &text).expect("import text");
    println!("[import] document id {}", doc.0);

    let stats = app.document_stats(doc).expect("stats");
    println!(
        "[stats] {} content tokens · {:.1}% unknown · {} distinct unknown words · band: {}",
        stats.content_tokens,
        stats.unknown_share() * 100.0,
        stats.distinct_unknown_words,
        stats.band.label()
    );

    let candidates = app.mining_candidates(doc).expect("mining");
    println!("[mining] top candidates:");
    for c in candidates.iter().take(10) {
        println!(
            "  {} ({}) ×{} rank={} — {}",
            c.word.key.lemma,
            c.word.key.reading,
            c.occurrences,
            c.corpus_rank.map(|r| r.to_string()).unwrap_or("—".into()),
            c.entry
                .as_ref()
                .map(|e| e.short_gloss())
                .unwrap_or_default()
        );
    }

    let Some(top) = candidates.first() else {
        println!("[done] nothing to mine");
        return;
    };
    app.start_learning(top.word.id, top.sentence.id)
        .expect("start learning");
    let due = app.due_reviews(10).expect("due reviews");
    println!(
        "[srs] {} due; first card: {} in 「{}」",
        due.len(),
        due[0].word.key.lemma,
        due[0]
            .sentence
            .as_ref()
            .map(|s| s.text.as_str())
            .unwrap_or("—")
    );
    let card = app
        .answer_review(top.word.id, Rating::Good)
        .expect("answer");
    println!(
        "[srs] answered Good → state {:?}, due {}",
        card.state, card.due
    );

    let recs = app.recommendations().expect("recommendations");
    println!("[recommend] next reads:");
    for r in &recs {
        println!(
            "  {} — {:.1}% unknown ({})",
            r.summary.document.title,
            r.stats.unknown_share() * 100.0,
            r.stats.band.label()
        );
    }
    println!("[done] smoke run complete");
}
