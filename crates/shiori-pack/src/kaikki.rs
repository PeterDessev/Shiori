//! Modern-language pack builder from kaikki.org Wiktextract JSONL.
//!
//! One kaikki per-language extract yields the whole data bundle: the
//! dictionary (lemma entries with glosses), the Tier-1 lemma table
//! (inflected `forms` arrays and `form_of` senses, inverted), and —
//! with a hermitdave OpenSubtitles list — the frequency ranks. Packs
//! come out beating the LWT/Lute model, whose per-form tracking never
//! lemmatizes at all.

use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::path::Path;

use shiori_pack::fold_lookup;

pub struct Report {
    pub entries: usize,
    pub forms: usize,
    pub frequency: usize,
}

pub struct LangSpec<'a> {
    pub lang: &'a str,
    pub name: &'a str,
    pub dict_source: &'a str,
    pub license: &'a str,
}

pub fn build_pack(
    kaikki_jsonl: &Path,
    frequency_list: Option<&Path>,
    spec: &LangSpec<'_>,
    out: &Path,
) -> std::io::Result<Report> {
    std::fs::create_dir_all(out.join("texts"))?;

    // lemma → (pos label, glosses)
    let mut lemmas: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    // (folded form, lemma, tag code)
    let mut form_rows: BTreeSet<(String, String, String)> = BTreeSet::new();

    let file = std::fs::File::open(kaikki_jsonl)?;
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(word) = entry["word"].as_str() else {
            continue;
        };
        let pos = entry["pos"].as_str().unwrap_or("").to_string();

        // Inflected-form senses invert into the lemma table.
        let mut is_form_entry = false;
        for sense in entry["senses"].as_array().into_iter().flatten() {
            for form_of in sense["form_of"].as_array().into_iter().flatten() {
                if let Some(lemma) = form_of["word"].as_str() {
                    is_form_entry = true;
                    let tags = sense["tags"]
                        .as_array()
                        .map(|t| {
                            t.iter()
                                .filter_map(|v| v.as_str())
                                .filter(|t| *t != "form-of")
                                .collect::<Vec<_>>()
                                .join("-")
                        })
                        .unwrap_or_default();
                    form_rows.insert((fold_lookup(word), lemma.to_string(), tags));
                }
            }
        }
        if is_form_entry {
            continue;
        }

        // A lemma entry: glosses plus its own declared forms.
        let glosses: Vec<String> = entry["senses"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|s| s["glosses"].as_array().into_iter().flatten())
            .filter_map(|g| g.as_str().map(str::to_string))
            .take(4)
            .collect();
        if !glosses.is_empty() {
            let slot = lemmas
                .entry(word.to_string())
                .or_insert_with(|| (pos.clone(), Vec::new()));
            for gloss in glosses {
                if !slot.1.contains(&gloss) && slot.1.len() < 6 {
                    slot.1.push(gloss);
                }
            }
        }
        for form in entry["forms"].as_array().into_iter().flatten() {
            if let Some(text) = form["form"].as_str() {
                if text == word || text.contains(' ') {
                    continue;
                }
                let tags = form["tags"]
                    .as_array()
                    .map(|t| {
                        t.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join("-")
                    })
                    .unwrap_or_default();
                if tags.contains("table") || tags.contains("inflection") {
                    continue; // template noise, not word forms
                }
                form_rows.insert((fold_lookup(text), word.to_string(), tags));
            }
        }
    }

    // Dictionary.
    let mut dictionary = String::new();
    for (lemma, (pos, glosses)) in &lemmas {
        let entry = serde_json::json!({
            "id": lemma,
            "kanji": [{"common": true, "text": lemma, "tags": []}],
            "kana": [],
            "sense": [{
                "partOfSpeech": if pos.is_empty() { serde_json::json!([]) } else { serde_json::json!([pos]) },
                "gloss": glosses.iter().map(|g| serde_json::json!({"lang": "eng", "text": g})).collect::<Vec<_>>(),
                "related": [], "antonym": [], "field": [], "dialect": [],
                "misc": [], "info": []
            }]
        });
        let line = serde_json::json!({
            "key": lemma,
            "forms": [{"text": fold_lookup(lemma), "role": "canonical", "common": true}],
            "entry": entry,
        });
        dictionary.push_str(&line.to_string());
        dictionary.push('\n');
    }
    std::fs::write(out.join("dictionary.jsonl"), dictionary)?;

    // Lemma table: skip forms that are themselves lemmas of other words
    // only when identical (fold collisions keep both rows; ambiguity is
    // handled at lookup time).
    let mut forms_tsv = String::new();
    for (form, lemma, tags) in &form_rows {
        forms_tsv.push_str(&format!("{form}\t{lemma}\t{tags}\n"));
    }
    std::fs::write(out.join("morph_forms.tsv"), forms_tsv)?;

    // Frequency, folded and deduplicated to first (best) rank.
    let mut frequency = String::new();
    let mut freq_count = 0usize;
    if let Some(path) = frequency_list {
        let mut seen = BTreeSet::new();
        let raw = std::fs::read_to_string(path)?;
        for line in raw.lines() {
            let word = line.split_whitespace().next().unwrap_or("");
            if word.is_empty() {
                continue;
            }
            let folded = fold_lookup(word);
            if seen.insert(folded.clone()) {
                freq_count += 1;
                frequency.push_str(&format!("{folded}\t{freq_count}\n"));
            }
        }
    }
    std::fs::write(out.join("frequency.tsv"), frequency)?;

    std::fs::write(out.join("manifest.toml"), manifest_toml(spec))?;

    Ok(Report {
        entries: lemmas.len(),
        forms: form_rows.len(),
        frequency: freq_count,
    })
}

/// The generic modern-language manifest.
fn manifest_toml(spec: &LangSpec<'_>) -> String {
    format!(
        r#"schema = 1
lang = "{lang}"
name = "{name}"
dict_source = "{source}"
license = "{license}"
joiner = " "
sentence_enders = [".", "?", "!", "…"]
# Basic Latin + Latin-1 Supplement + Latin Extended-A/B.
script_ranges = [[65, 90], [97, 122], [192, 591]]

[prompt]
language_name = "{name}"
chat_persona = "a friendly native {name} speaker"
citation_guidance = "When you cite {name}, add a brief English gloss in parentheses where helpful."
grammar_skeleton = "verb conjugation, agreement, word order"
immerse_instruction = "Write natural native {name} without simplification; the user wants full immersion."
"#,
        lang = spec.lang,
        name = spec.name,
        source = spec.dict_source,
        license = spec.license,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAIKKI_ES_SAMPLE: &str = r#"
{"word":"gato","pos":"noun","lang_code":"es","senses":[{"glosses":["cat"]}],"forms":[{"form":"gatos","tags":["plural"]},{"form":"gata","tags":["feminine"]}]}
{"word":"gatos","pos":"noun","lang_code":"es","senses":[{"glosses":["plural of gato"],"tags":["form-of","plural"],"form_of":[{"word":"gato"}]}]}
{"word":"hablar","pos":"verb","lang_code":"es","senses":[{"glosses":["to speak, to talk"]}],"forms":[{"form":"hablo","tags":["first-person","singular","present"]},{"form":"habló","tags":["third-person","singular","preterite"]}]}
"#;

    #[test]
    fn builds_a_spanish_pack() {
        let dir = std::env::temp_dir().join(format!("shiori-kaikki-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("es.jsonl");
        std::fs::write(&input, KAIKKI_ES_SAMPLE).unwrap();
        let freq = dir.join("es_50k.txt");
        std::fs::write(&freq, "de 9|\nque 8\ngato 7\nhablar 6\n").unwrap();

        let out = dir.join("packs").join("es");
        let spec = LangSpec {
            lang: "es",
            name: "Spanish",
            dict_source: "es-pack",
            license: "CC BY-SA 4.0",
        };
        let report = build_pack(&input, Some(&freq), &spec, &out).unwrap();
        assert_eq!(report.entries, 2, "gato and hablar are lemmas");
        assert!(report.forms >= 4, "plural, feminine, and verb forms");
        assert_eq!(report.frequency, 4);

        // The pack loads and the manifest is sound.
        let pack = shiori_pack::Pack::load(&out).unwrap();
        assert_eq!(pack.manifest.lang, "es");
        assert_eq!(pack.manifest.prompt_profile().language_name, "Spanish");

        // The lemma table inverts both the form_of sense and the forms
        // array; accents fold in the key but the lemma stays exact.
        let forms = std::fs::read_to_string(out.join("morph_forms.tsv")).unwrap();
        assert!(forms.contains("gatos\tgato"), "{forms}");
        assert!(forms.contains("hablo\thablar"), "{forms}");
        assert!(
            forms
                .lines()
                .any(|l| l.starts_with("hablo\t") && l.contains("preterite"))
                || forms.contains("hablo\thablar\tthird-person-singular-preterite"),
            "folded habló joins hablo's rows: {forms}"
        );

        // Dictionary glosses survive.
        let dict = std::fs::read_to_string(out.join("dictionary.jsonl")).unwrap();
        assert!(dict.contains("to speak, to talk"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
