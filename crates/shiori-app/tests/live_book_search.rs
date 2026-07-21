//! Live network smoke tests for per-language book search. These hit real
//! servers (Gutendex, Wikisource, OPDS distributors) and are `#[ignore]`d
//! so the normal `cargo test` stays offline and deterministic. Run with:
//!
//!   cargo test -p shiori-app --test live_book_search -- --ignored --nocapture

use shiori_app::App;
use shiori_db::Db;

fn app() -> App {
    // A bare in-memory app; active language defaults to Japanese, which
    // is enough to exercise the (language-parametric) search paths.
    App::with_db(Db::open_in_memory().unwrap(), std::env::temp_dir()).unwrap()
}

#[test]
#[ignore = "hits gutendex.com"]
fn gutendex_search_returns_books() {
    let app = app();
    // Gutenberg indexes Japanese records by ROMANIZED metadata, so search
    // a romanized author name (Akutagawa) with the ja language filter.
    let hits = app.search_gutendex("Akutagawa").expect("gutendex search");
    println!("gutendex(ja) 'Akutagawa': {} hits", hits.len());
    assert!(
        !hits.is_empty(),
        "expected Japanese Gutenberg hits for Akutagawa"
    );
    let first = &hits[0];
    println!("  {} — {}", first.title, first.author);
    assert!(
        first.is_importable(),
        "a Gutenberg book should be importable"
    );
    assert!(
        first.languages.iter().any(|l| l == "ja"),
        "language filter should restrict to Japanese"
    );
}

#[test]
#[ignore = "hits ja.wikisource.org"]
fn wikisource_search_returns_whole_works() {
    let app = app();
    let hits = app
        .search_wikisource("夏目漱石")
        .expect("wikisource search");
    println!("ja.wikisource '夏目漱石': {} whole works", hits.len());
    assert!(!hits.is_empty(), "expected Japanese Wikisource hits");
    for h in hits.iter().take(5) {
        println!("  {} ({} words)", h.title, h.wordcount);
    }
    // Multi-part books are collapsed: no chapter subpages leak through.
    assert!(
        hits.iter().all(|h| !h.title.contains('/')),
        "results should be whole works, not `Work/Chapter` fragments"
    );
}

#[test]
#[ignore = "downloads a work via WSexport (ws-export.wmcloud.org) and imports it whole"]
fn wikisource_whole_book_import() {
    let app = app();
    // A standalone Japanese Wikisource work; imported whole via WSexport.
    let id = app
        .import_wikisource_page("第三夜")
        .expect("whole-book Wikisource import");
    println!("imported ja.wikisource '第三夜' as document {id:?}");
}

#[test]
#[ignore = "hits www.gutenberg.org OPDS (two-hop navigation feed)"]
fn opds_gutenberg_two_hop_search() {
    let app = app();
    let hits = app
        .search_opds("https://www.gutenberg.org/ebooks.opds/", "alice wonderland")
        .expect("opds gutenberg search");
    println!("opds gutenberg 'alice wonderland': {} hits", hits.len());
    for h in hits.iter().take(3) {
        println!("  {} — {} [{} links]", h.title, h.author, h.links.len());
    }
    assert!(
        !hits.is_empty(),
        "expected Gutenberg OPDS results via two-hop"
    );
    assert!(
        hits.iter().any(|h| h.best_link().is_some()),
        "at least one result should have an importable acquisition link"
    );
}

#[test]
#[ignore = "hits openlibrary.org OPDS 2.0 (JSON) search"]
fn opds_openlibrary_json_search() {
    let app = app();
    let hits = app
        .search_opds("https://openlibrary.org/opds", "sherlock holmes")
        .expect("opds openlibrary search");
    println!("opds openlibrary 'sherlock holmes': {} hits", hits.len());
    for h in hits.iter().take(3) {
        println!("  {} — {}", h.title, h.author);
    }
    assert!(!hits.is_empty(), "expected Open Library OPDS 2.0 results");
}
