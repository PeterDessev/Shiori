//! The Koine Greek pack builder: MorphGNT + a gloss index → a complete
//! pack directory (manifest, SIAT texts, dictionary, frequency, tags,
//! graded tiers).

use std::collections::HashMap;
use std::path::Path;

use shiori_pack::siat;

pub struct Report {
    pub texts: usize,
    pub sentences: usize,
    pub lemmas: usize,
    pub tags: usize,
}

/// GNT frequency tiers for the graded scheme: learning the tiers in
/// order tracks the classic "read the GNT" vocabulary curricula.
const TIERS: &[(u32, u32, &str)] = &[
    (1, 50, "Core 50×+"),
    (2, 30, "30×+"),
    (3, 20, "20×+"),
    (4, 10, "10×+"),
    (5, 5, "5×+"),
];

pub fn build_pack(
    morphgnt_dir: &Path,
    glosses: &HashMap<String, String>,
    out: &Path,
    license: &str,
) -> std::io::Result<Report> {
    let texts_dir = out.join("texts");
    std::fs::create_dir_all(&texts_dir)?;

    let gloss_of = |lemma: &str| glosses.get(lemma).cloned();

    // Convert every MorphGNT book file, accumulating lemma statistics.
    let mut lemma_counts: HashMap<String, u64> = HashMap::new();
    let mut lemma_pos: HashMap<String, HashMap<String, u64>> = HashMap::new();
    let mut seen_tags: std::collections::BTreeSet<String> = Default::default();
    let mut texts = 0usize;
    let mut sentences = 0usize;

    let mut files: Vec<_> = std::fs::read_dir(morphgnt_dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("-morphgnt.txt"))
        })
        .collect();
    files.sort();

    let mut morph_forms: std::collections::BTreeSet<(String, String, String)> = Default::default();

    for path in files {
        let raw = std::fs::read_to_string(&path)?;
        // Full-form table rows straight from the columns: the normalized
        // word (col 6) folded, its lemma and Robinson code. This is what
        // lets plain-text Greek imports lemmatize without an analyzer.
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 7 {
                morph_forms.insert((
                    shiori_pack::fold_lookup(fields[5]),
                    fields[6].to_string(),
                    siat::robinson_code(fields[1].trim_end_matches('-'), fields[2]),
                ));
            }
        }
        let stem = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim_end_matches("-morphgnt.txt")
            .to_string();
        // Filenames look like "64-Jn"; the part after the dash is the
        // book abbreviation used in citations.
        let book = stem
            .split_once('-')
            .map(|(_, b)| b.to_string())
            .unwrap_or(stem.clone());
        let title = format!("SBLGNT — {book}");
        let doc = siat::from_morphgnt(&raw, &book, &title, license, &gloss_of)
            .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;

        for sentence in &doc.sentences {
            for token in &sentence.tokens {
                *lemma_counts.entry(token.l.clone()).or_default() += 1;
                *lemma_pos
                    .entry(token.l.clone())
                    .or_default()
                    .entry(token.m.split('-').next().unwrap_or("").to_string())
                    .or_default() += 1;
                for segment in token.m.split('-').filter(|s| !s.is_empty()) {
                    seen_tags.insert(segment.to_string());
                }
            }
        }
        sentences += doc.sentences.len();
        std::fs::write(
            texts_dir.join(format!("{stem}.siat.jsonl")),
            siat::to_jsonl(&doc),
        )?;
        texts += 1;
    }

    // Frequency list: folded lemma by descending corpus count.
    let mut ranked: Vec<(&String, &u64)> = lemma_counts.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut frequency = String::new();
    for (rank, (lemma, _)) in ranked.iter().enumerate() {
        frequency.push_str(&format!(
            "{}\t{}\n",
            shiori_pack::fold_lookup(lemma),
            rank + 1
        ));
    }
    std::fs::write(out.join("frequency.tsv"), frequency)?;

    let mut forms_tsv = String::new();
    for (form, lemma, morph) in &morph_forms {
        forms_tsv.push_str(&format!("{form}\t{lemma}\t{morph}\n"));
    }
    std::fs::write(out.join("morph_forms.tsv"), forms_tsv)?;

    // Dictionary: one entry per lemma, glossed where the index has one,
    // in the jmdict-simplified shape the app renders.
    let mut dictionary = String::new();
    for (lemma, _) in &ranked {
        let gloss = glosses.get(*lemma).cloned().unwrap_or_default();
        let pos_label = lemma_pos
            .get(*lemma)
            .and_then(|m| m.iter().max_by_key(|(_, n)| **n))
            .map(|(code, _)| pos_display(code))
            .unwrap_or("word");
        let entry = serde_json::json!({
            "id": lemma,
            "kanji": [{"common": true, "text": lemma, "tags": []}],
            "kana": [],
            "sense": [{
                "partOfSpeech": [pos_label],
                "gloss": if gloss.is_empty() {
                    serde_json::json!([])
                } else {
                    serde_json::json!([{"lang": "eng", "text": gloss}])
                },
                "related": [], "antonym": [], "field": [], "dialect": [],
                "misc": [], "info": []
            }]
        });
        let line = serde_json::json!({
            "key": lemma,
            "forms": [{
                "text": shiori_pack::fold_lookup(lemma),
                "role": "canonical",
                "common": true
            }],
            "entry": entry,
        });
        dictionary.push_str(&line.to_string());
        dictionary.push('\n');
    }
    std::fs::write(out.join("dictionary.jsonl"), dictionary)?;

    // Tag decodings for every code the corpus actually uses.
    let mut tags = String::new();
    let mut tag_count = 0usize;
    for code in &seen_tags {
        if let Some(label) = tag_label(code) {
            tags.push_str(&format!("{code}\t{label}\n"));
            tag_count += 1;
        }
    }
    std::fs::write(out.join("tags.tsv"), tags)?;

    // Graded tiers from corpus frequency.
    let mut graded = String::new();
    for (lemma, count) in &ranked {
        if let Some((ord, _, label)) = TIERS.iter().find(|(_, min, _)| **count >= u64::from(*min)) {
            graded.push_str(&format!(
                "{ord}\t{label}\t{}\t\n",
                shiori_pack::fold_lookup(lemma)
            ));
        }
    }
    std::fs::write(out.join("graded.tsv"), graded)?;

    std::fs::write(
        out.join("manifest.toml"),
        shiori_pack::manifest::KOINE_GREEK_MANIFEST.trim_start(),
    )?;

    Ok(Report {
        texts,
        sentences,
        lemmas: ranked.len(),
        tags: tag_count,
    })
}

fn pos_display(code: &str) -> &'static str {
    match code {
        "N" => "noun",
        "V" => "verb",
        "A" => "adjective",
        "D" => "adverb",
        "C" => "conjunction",
        "P" => "preposition",
        "RA" => "article",
        "RP" => "personal pronoun",
        "RR" => "relative pronoun",
        "RD" => "demonstrative pronoun",
        "RI" => "interrogative/indefinite pronoun",
        "X" => "particle",
        "I" => "interjection",
        _ => "word",
    }
}

/// Labels for Robinson-style segments: POS heads, tense/voice/mood
/// triples, person+number, case+number+gender, degree.
fn tag_label(code: &str) -> Option<String> {
    // POS heads.
    let head = match code {
        "N" => Some("noun"),
        "V" => Some("verb"),
        "A" => Some("adjective"),
        "D" => Some("adverb"),
        "C" => Some("conjunction"),
        "P" => Some("preposition"),
        "RA" => Some("article"),
        "RP" => Some("personal pronoun"),
        "RR" => Some("relative pronoun"),
        "RD" => Some("demonstrative pronoun"),
        "RI" => Some("interrogative/indefinite pronoun"),
        "X" => Some("particle"),
        "I" => Some("interjection"),
        "COMP" => Some("comparative"),
        "SUPL" => Some("superlative"),
        _ => None,
    };
    if let Some(label) = head {
        return Some(label.to_string());
    }

    let chars: Vec<char> = code.chars().collect();
    // Person + number: "3S".
    if chars.len() == 2 && chars[0].is_ascii_digit() {
        let person = match chars[0] {
            '1' => "1st person",
            '2' => "2nd person",
            '3' => "3rd person",
            _ => return None,
        };
        let number = number_label(chars[1])?;
        return Some(format!("{person} {number}"));
    }
    // Tense + voice + mood: "PAI", "IAI", "AAN", "PAP".
    if chars.len() == 3 && !chars[0].is_ascii_digit() {
        if let (Some(tense), Some(voice), Some(mood)) = (
            tense_label(chars[0]),
            voice_label(chars[1]),
            mood_label(chars[2]),
        ) {
            return Some(format!("{tense} {voice} {mood}"));
        }
        // Case + number + gender: "NSM", "DSF".
        if let (Some(case), Some(number), Some(gender)) = (
            case_label(chars[0]),
            number_label(chars[1]),
            gender_label(chars[2]),
        ) {
            return Some(format!("{case} {number} {gender}"));
        }
        // Case + number, no gender: "DS-".
        if let (Some(case), Some(number)) = (case_label(chars[0]), number_label(chars[1])) {
            return Some(format!("{case} {number}"));
        }
    }
    None
}

fn tense_label(c: char) -> Option<&'static str> {
    Some(match c {
        'P' => "present",
        'I' => "imperfect",
        'F' => "future",
        'A' => "aorist",
        'X' => "perfect",
        'Y' => "pluperfect",
        _ => return None,
    })
}

fn voice_label(c: char) -> Option<&'static str> {
    Some(match c {
        'A' => "active",
        'M' => "middle",
        'P' => "passive",
        'E' => "middle/passive",
        _ => return None,
    })
}

fn mood_label(c: char) -> Option<&'static str> {
    Some(match c {
        'I' => "indicative",
        'D' => "imperative",
        'S' => "subjunctive",
        'O' => "optative",
        'N' => "infinitive",
        'P' => "participle",
        _ => return None,
    })
}

fn case_label(c: char) -> Option<&'static str> {
    Some(match c {
        'N' => "nominative",
        'G' => "genitive",
        'D' => "dative",
        'A' => "accusative",
        'V' => "vocative",
        _ => return None,
    })
}

fn number_label(c: char) -> Option<&'static str> {
    Some(match c {
        'S' => "singular",
        'P' => "plural",
        'D' => "dual",
        _ => return None,
    })
}

fn gender_label(c: char) -> Option<&'static str> {
    Some(match c {
        'M' => "masculine",
        'F' => "feminine",
        'N' => "neuter",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOHN_SAMPLE: &str = "\
040101 P- -------- Ἐν Ἐν ἐν ἐν
040101 N- ----DSF- ἀρχῇ ἀρχῇ ἀρχῇ ἀρχή
040101 V- 3IAI-S-- ἦν ἦν ἦν εἰμί
040101 RA ----NSM- ὁ ὁ ὁ ὁ
040101 N- ----NSM- λόγος, λόγος λόγος λόγος
040102 RA ----NSM- ὁ ὁ ὁ ὁ
040102 N- ----NSM- λόγος λόγος λόγος λόγος
";

    #[test]
    fn builds_a_pack_the_app_can_load() {
        let dir = std::env::temp_dir().join(format!("shiori-packc-test-{}", std::process::id()));
        let src = dir.join("morphgnt");
        let out = dir.join("out").join("grc");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("64-Jn-morphgnt.txt"), JOHN_SAMPLE).unwrap();

        let mut glosses = HashMap::new();
        glosses.insert("λόγος".to_string(), "word, speech".to_string());

        let report = build_pack(&src, &glosses, &out, "CC BY-SA 4.0").unwrap();
        assert_eq!(report.texts, 1);
        assert_eq!(report.sentences, 2);
        assert!(report.lemmas >= 4);
        assert!(report.tags >= 6);

        // The emitted pack loads with the runtime loader…
        let pack = shiori_pack::Pack::load(&out).unwrap();
        assert_eq!(pack.manifest.lang, "grc");
        assert_eq!(pack.text_paths().len(), 1);

        // …and its SIAT output revalidates.
        let text = std::fs::read_to_string(&pack.text_paths()[0]).unwrap();
        let doc = shiori_pack::siat::parse(&text).unwrap();
        assert_eq!(doc.sentences[0].reference, "Jn.1.1");
        // Punctuation stays attached to the MorphGNT text column and the
        // tiling contract still holds (validated by parse above).

        // Frequency ranks λόγος (2×) above the hapaxes.
        let freq = std::fs::read_to_string(out.join("frequency.tsv")).unwrap();
        let first = freq.lines().next().unwrap();
        assert!(first.starts_with("λογοσ\t") || first.starts_with("ο\t"));

        // The dictionary carries the gloss.
        let dict = std::fs::read_to_string(out.join("dictionary.jsonl")).unwrap();
        assert!(dict.contains("word, speech"));

        // The full-form table maps every attested form to lemma + parse.
        let forms = std::fs::read_to_string(out.join("morph_forms.tsv")).unwrap();
        assert!(forms.contains("ην\tεἰμί\tV-IAI-3S"), "{forms}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tag_labels_cover_the_usual_codes() {
        assert_eq!(tag_label("V").as_deref(), Some("verb"));
        assert_eq!(
            tag_label("PAI").as_deref(),
            Some("present active indicative")
        );
        assert_eq!(
            tag_label("IAI").as_deref(),
            Some("imperfect active indicative")
        );
        assert_eq!(
            tag_label("NSM").as_deref(),
            Some("nominative singular masculine")
        );
        assert_eq!(tag_label("3S").as_deref(), Some("3rd person singular"));
        assert_eq!(
            tag_label("PAP").as_deref(),
            Some("present active participle")
        );
        assert_eq!(tag_label("ZZZ"), None);
    }
}
