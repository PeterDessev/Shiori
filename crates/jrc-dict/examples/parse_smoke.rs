//! One-off smoke test: parse a real jmdict-simplified file.
//! Usage: cargo run -p jrc-dict --example parse_smoke -- <path-to-json>

fn main() {
    let path = std::env::args().nth(1).expect("pass a path to jmdict json");
    let json = std::fs::read_to_string(&path).expect("read file");
    let start = std::time::Instant::now();
    let dict = jrc_dict::Dictionary::parse(&json).expect("parse");
    println!("parsed {} entries in {:?}", dict.len(), start.elapsed());

    for (lemma, reading) in [("食べる", "たべる"), ("行く", "いく"), ("召し上がる", "めしあがる")] {
        match dict.lookup_best(lemma, reading) {
            Some(e) => println!(
                "{lemma}: [{}] {} | misc={:?} related={:?}",
                e.reading(),
                e.short_gloss(),
                e.misc_codes(),
                e.related_words()
            ),
            None => println!("{lemma}: NOT FOUND"),
        }
    }
}
