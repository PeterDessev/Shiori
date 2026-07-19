//! Modern-language pack builder from kaikki.org Wiktextract JSONL.
//!
//! One kaikki per-language extract yields the whole data bundle: the
//! dictionary (per-sense glosses with register labels, usage examples,
//! and IPA), the grammar (inflected `forms` arrays and `form_of` senses
//! inverted into the Tier-1 full-form table, plus the tag table that
//! decodes each parse to prose), and — with a hermitdave OpenSubtitles
//! list — *lemmatized* frequency ranks and graded tiers derived from
//! them. Used by `shiori-packc build-kaikki` in CI and by the app's
//! build-from-the-web feature, which downloads the same public data and
//! processes it locally, exactly like the Japanese reference bundle.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::BufRead;
use std::path::Path;

use crate::fold_lookup;

pub struct Report {
    pub entries: usize,
    pub forms: usize,
    pub tags: usize,
    pub frequency: usize,
    /// Lemmas placed into graded frequency tiers.
    pub graded: usize,
    /// Learned suffix rewrite rules for lemma guessing.
    pub suffix_rules: usize,
}

pub struct LangSpec<'a> {
    pub lang: &'a str,
    pub name: &'a str,
    pub dict_source: &'a str,
    pub license: &'a str,
    /// One or two sentences for the manifest's `description`.
    pub description: &'a str,
    /// Unicode ranges counting as target-language script; empty means
    /// the Latin default.
    pub script_ranges: &'a [(u32, u32)],
    /// Elidable prefixes for the tokenizer ("l", "d", "qu" for French).
    pub elisions: &'a [&'a str],
    /// Portmanteau function words and their expansions ("au" = "à le").
    pub contractions: &'a [(&'a str, &'a str)],
    /// `Some(linkers)` enables compound splitting with these linking
    /// elements between parts ("s" in Arbeitsmaschine); `None` for
    /// non-compounding languages.
    pub compound_linkers: Option<&'a [&'a str]>,
}

/// Basic Latin + Latin-1 Supplement + Latin Extended-A/B.
const LATIN_RANGES: &[(u32, u32)] = &[(65, 90), (97, 122), (192, 591)];

/// How often the line scanner reports progress.
const PROGRESS_EVERY_LINES: usize = 100_000;

/// Graded frequency tiers: (ordinal, label, lemma-rank bound).
const TIERS: &[(u32, &str, usize)] = &[
    (1, "Top 500", 500),
    (2, "Top 1,000", 1000),
    (3, "Top 2,000", 2000),
    (4, "Top 5,000", 5000),
];

/// One meaning of a lemma, carried through to the dictionary entry.
struct SenseOut {
    pos: String,
    /// "transitive"/"intransitive", appended to the POS labels.
    extra_pos: Vec<String>,
    glosses: Vec<String>,
    /// JMdict-style register codes ("col", "arch", …) mapped from
    /// Wiktionary sense tags, so the app's usage-register display works
    /// for built packs exactly as it does for Japanese.
    misc: Vec<String>,
    /// Usage examples, rendered as info lines.
    info: Vec<String>,
}

#[derive(Default)]
struct LemmaOut {
    senses: Vec<SenseOut>,
    /// First IPA pronunciation seen for the lemma.
    ipa: Option<String>,
}

/// String interner: the form table repeats each lemma and tag string
/// thousands of times; ids keep a gigabyte-class dump's tables in
/// bounds.
#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    values: Vec<String>,
}

impl Interner {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.values.len() as u32;
        self.ids.insert(s.to_string(), id);
        self.values.push(s.to_string());
        id
    }

    fn get(&self, id: u32) -> &str {
        &self.values[id as usize]
    }
}

pub fn build_pack(
    kaikki_jsonl: &Path,
    frequency_list: Option<&Path>,
    spec: &LangSpec<'_>,
    out: &Path,
) -> std::io::Result<Report> {
    build_pack_with_progress(kaikki_jsonl, frequency_list, spec, out, &mut |_| {})
}

/// Like [`build_pack`], reporting human-readable progress lines (the
/// scan of a large extract takes a while).
pub fn build_pack_with_progress(
    kaikki_jsonl: &Path,
    frequency_list: Option<&Path>,
    spec: &LangSpec<'_>,
    out: &Path,
    on_progress: &mut dyn FnMut(&str),
) -> std::io::Result<Report> {
    std::fs::create_dir_all(out.join("texts"))?;

    let mut lemmas: BTreeMap<String, LemmaOut> = BTreeMap::new();
    let mut strings = Interner::default();
    // (folded form, lemma id, tag-code-string id)
    let mut form_rows: BTreeSet<(String, u32, u32)> = BTreeSet::new();
    // Every tag code used by a form row, for the decoding table.
    let mut tag_codes: BTreeSet<String> = BTreeSet::new();

    let file = std::fs::File::open(kaikki_jsonl)?;
    let mut scanned = 0usize;
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        scanned += 1;
        if scanned.is_multiple_of(PROGRESS_EVERY_LINES) {
            on_progress(&format!(
                "scanning Wiktionary data… {scanned} entries read, {} words so far",
                lemmas.len()
            ));
        }
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
                    let tags = tag_code(
                        sense["tags"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v.as_str())
                            .filter(|t| *t != "form-of"),
                        &mut tag_codes,
                    );
                    form_rows.insert((
                        fold_lookup(word),
                        strings.intern(lemma),
                        strings.intern(&tags),
                    ));
                }
            }
        }
        if is_form_entry {
            continue;
        }

        // A lemma entry: senses (glosses + register tags + examples),
        // pronunciation, and its own declared forms.
        let mut senses_out = Vec::new();
        for sense in entry["senses"].as_array().into_iter().flatten() {
            let glosses: Vec<String> = sense["glosses"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| g.as_str().map(str::to_string))
                .take(4)
                .collect();
            if glosses.is_empty() {
                continue;
            }
            let mut misc = Vec::new();
            let mut extra_pos = Vec::new();
            for tag in sense["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
            {
                if let Some(code) = register_misc(tag) {
                    if !misc.contains(&code.to_string()) {
                        misc.push(code.to_string());
                    }
                } else if tag == "transitive" || tag == "intransitive" {
                    extra_pos.push(tag.to_string());
                }
            }
            let info: Vec<String> = sense["examples"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|ex| {
                    let text = ex["text"].as_str()?;
                    Some(match ex["english"].as_str() {
                        Some(english) => format!("e.g. {text} — {english}"),
                        None => format!("e.g. {text}"),
                    })
                })
                .take(2)
                .collect();
            senses_out.push(SenseOut {
                pos: pos.clone(),
                extra_pos,
                glosses,
                misc,
                info,
            });
        }
        if !senses_out.is_empty() {
            let slot = lemmas.entry(word.to_string()).or_default();
            for sense in senses_out {
                if slot.senses.len() < 8 {
                    slot.senses.push(sense);
                }
            }
            if slot.ipa.is_none() {
                slot.ipa = entry["sounds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find_map(|s| s["ipa"].as_str())
                    .map(str::to_string);
            }
        }
        for form in entry["forms"].as_array().into_iter().flatten() {
            if let Some(text) = form["form"].as_str() {
                if text == word || text.contains(' ') {
                    continue;
                }
                let raw_tags: Vec<&str> = form["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str())
                    .collect();
                if raw_tags.iter().any(|t| *t == "table" || *t == "inflection") {
                    continue; // template noise, not word forms
                }
                let tags = tag_code(raw_tags.into_iter(), &mut tag_codes);
                form_rows.insert((
                    fold_lookup(text),
                    strings.intern(word),
                    strings.intern(&tags),
                ));
            }
        }
    }

    // Dictionary.
    on_progress("writing dictionary…");
    let mut dictionary = String::new();
    for (lemma, data) in &lemmas {
        let kana = match &data.ipa {
            Some(ipa) => {
                serde_json::json!([{"common": false, "text": ipa, "tags": ["ipa"]}])
            }
            None => serde_json::json!([]),
        };
        let senses: Vec<serde_json::Value> = data
            .senses
            .iter()
            .map(|s| {
                let mut pos_list = Vec::new();
                if !s.pos.is_empty() {
                    pos_list.push(s.pos.clone());
                }
                pos_list.extend(s.extra_pos.iter().cloned());
                serde_json::json!({
                    "partOfSpeech": pos_list,
                    "gloss": s.glosses.iter().map(|g| serde_json::json!({"lang": "eng", "text": g})).collect::<Vec<_>>(),
                    "misc": s.misc,
                    "info": s.info,
                    "related": [], "antonym": [], "field": [], "dialect": []
                })
            })
            .collect();
        let entry = serde_json::json!({
            "id": lemma,
            "kanji": [{"common": true, "text": lemma, "tags": []}],
            "kana": kana,
            "sense": senses,
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
    on_progress("writing grammar tables…");
    let mut forms_tsv = String::new();
    for (form, lemma_id, tags_id) in &form_rows {
        forms_tsv.push_str(&format!(
            "{form}\t{}\t{}\n",
            strings.get(*lemma_id),
            strings.get(*tags_id)
        ));
    }
    std::fs::write(out.join("morph_forms.tsv"), forms_tsv)?;

    // Tag decoding: each segment code back to the Wiktionary phrase it
    // came from ("first_person" → "first person"), so the reader can
    // explain a form's parse in prose. Codes that already read as prose
    // need no row — unmapped segments render verbatim.
    let mut tags_tsv = String::new();
    let mut tag_count = 0usize;
    for code in &tag_codes {
        let label = code.replace('_', " ");
        if label != *code {
            tags_tsv.push_str(&format!("{code}\t{label}\n"));
            tag_count += 1;
        }
    }
    std::fs::write(out.join("tags.tsv"), tags_tsv)?;

    // Frequency, lemmatized: subtitle counts are per surface form, so
    // fold each form's mass onto its lemma(s) through the table just
    // built — *hablar* is ranked by all its conjugations, not just the
    // infinitive's own occurrences. Ambiguous forms split their mass;
    // forms resolving to nothing stay as themselves (Tier-1 keeps
    // unknown surfaces as their own lemmas, so they remain lookupable).
    on_progress("ranking frequency…");
    let mut candidates_by_form: HashMap<&str, Vec<u32>> = HashMap::new();
    for (form, lemma_id, _) in &form_rows {
        let slot = candidates_by_form.entry(form.as_str()).or_default();
        if !slot.contains(lemma_id) {
            slot.push(*lemma_id);
        }
    }
    let folded_of_lemma: HashMap<u32, String> = candidates_by_form
        .values()
        .flatten()
        .map(|id| (*id, fold_lookup(strings.get(*id))))
        .collect();

    on_progress("learning suffix rules…");
    // Suffix rewrite rules learned from the same pairs: "form ending
    // -o rewrites to lemma ending -ar" and the like, so the runtime
    // can guess lemmas for regular inflections missing from the
    // tables (validated against the dictionary before use).
    let mut rule_counts: HashMap<(String, String), u32> = HashMap::new();
    let mut counted: BTreeSet<(&str, u32)> = BTreeSet::new();
    for (form, lemma_id, _) in &form_rows {
        // One vote per distinct (form, lemma) pair.
        if !counted.insert((form.as_str(), *lemma_id)) {
            continue;
        }
        let lemma = &folded_of_lemma[lemma_id];
        let stem = common_prefix_bytes(form, lemma);
        if form[..stem].chars().count() < 3 {
            continue;
        }
        let (form_suffix, lemma_suffix) = (&form[stem..], &lemma[stem..]);
        if form_suffix.is_empty()
            || form_suffix == lemma_suffix
            || form_suffix.chars().count() > 5
            || lemma_suffix.chars().count() > 5
        {
            continue;
        }
        *rule_counts
            .entry((form_suffix.to_string(), lemma_suffix.to_string()))
            .or_insert(0) += 1;
    }
    let mut rules: Vec<((String, String), u32)> = rule_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .collect();
    rules.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rules.truncate(5000);
    let mut rules_tsv = String::new();
    for ((form_suffix, lemma_suffix), count) in &rules {
        rules_tsv.push_str(&format!("{form_suffix}\t{lemma_suffix}\t{count}\n"));
    }
    std::fs::write(out.join("suffix_rules.tsv"), rules_tsv)?;
    // Folded spellings of real dictionary lemmas, for the direct case.
    let lemma_by_folded: HashMap<String, &str> = lemmas
        .keys()
        .map(|lemma| (fold_lookup(lemma), lemma.as_str()))
        .collect();

    let mut mass: HashMap<String, f64> = HashMap::new();
    let mut freq_lines = 0usize;
    if let Some(path) = frequency_list {
        let raw = std::fs::read_to_string(path)?;
        for (i, line) in raw.lines().enumerate() {
            let mut fields = line.split_whitespace();
            let Some(word) = fields.next() else { continue };
            if word.is_empty() {
                continue;
            }
            freq_lines += 1;
            let weight = fields
                .next()
                .and_then(|c| c.parse::<f64>().ok())
                .unwrap_or(1.0 / (i + 1) as f64);
            let folded = fold_lookup(word);
            let mut targets: Vec<String> = Vec::new();
            if lemma_by_folded.contains_key(&folded) {
                targets.push(folded.clone());
            }
            for lemma_id in candidates_by_form
                .get(folded.as_str())
                .into_iter()
                .flatten()
            {
                let lemma_folded = &folded_of_lemma[lemma_id];
                if !targets.contains(lemma_folded) {
                    targets.push(lemma_folded.clone());
                }
            }
            if targets.is_empty() {
                targets.push(folded);
            }
            let share = weight / targets.len() as f64;
            for target in targets {
                *mass.entry(target).or_insert(0.0) += share;
            }
        }
    }
    let mut ranked: Vec<(&String, &f64)> = mass.iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut frequency = String::new();
    for (rank, (folded, _)) in ranked.iter().enumerate() {
        frequency.push_str(&format!("{folded}\t{}\n", rank + 1));
    }
    std::fs::write(out.join("frequency.tsv"), frequency)?;
    let _ = freq_lines;

    // Graded tiers over the lemmatized ranking: only real dictionary
    // lemmas count toward tier positions, stored under their exact
    // spelling (word identities keep exact lemmas).
    let mut graded = String::new();
    let mut graded_count = 0usize;
    for (folded, _) in &ranked {
        let Some(lemma) = lemma_by_folded.get(*folded) else {
            continue;
        };
        let Some((ord, label, _)) = TIERS.iter().find(|(_, _, bound)| graded_count < *bound) else {
            break;
        };
        graded.push_str(&format!("{ord}\t{label}\t{lemma}\t\n"));
        graded_count += 1;
    }
    std::fs::write(out.join("graded.tsv"), graded)?;

    std::fs::write(
        out.join("manifest.toml"),
        manifest_toml(spec, graded_count > 0),
    )?;

    Ok(Report {
        entries: lemmas.len(),
        forms: form_rows.len(),
        tags: tag_count,
        frequency: ranked.len(),
        graded: graded_count,
        suffix_rules: rules.len(),
    })
}

/// Byte length of the longest common prefix ending on a char boundary.
fn common_prefix_bytes(a: &str, b: &str) -> usize {
    let mut p = 0;
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca != cb {
            break;
        }
        p += ca.len_utf8();
    }
    p
}

/// Map a Wiktionary sense tag to the JMdict register code the app's
/// usage display understands. `None` for tags that mark no register.
fn register_misc(tag: &str) -> Option<&'static str> {
    match tag {
        "colloquial" | "informal" => Some("col"),
        "slang" => Some("sl"),
        "internet-slang" => Some("net-sl"),
        "familiar" => Some("fam"),
        "childish" | "child-language" => Some("chn"),
        "humorous" | "jocular" => Some("joc"),
        "formal" => Some("form"),
        "literary" => Some("litf"),
        "poetic" => Some("poet"),
        "honorific" => Some("hon"),
        "humble" => Some("hum"),
        "polite" => Some("pol"),
        "archaic" => Some("arch"),
        "obsolete" => Some("obs"),
        "dated" => Some("dated"),
        "rare" => Some("rare"),
        "derogatory" | "offensive" | "pejorative" => Some("derog"),
        "vulgar" => Some("vulg"),
        _ => None,
    }
}

/// Join a form's Wiktionary tags into one parse-code string. Segments
/// join with '-' (the decoder splits on it), so hyphens *inside* a tag
/// become underscores: ["first-person", "singular"] →
/// "first_person-singular", which `tags.tsv` decodes back to
/// "first person · singular".
fn tag_code<'a>(tags: impl Iterator<Item = &'a str>, seen: &mut BTreeSet<String>) -> String {
    let mut codes = Vec::new();
    for tag in tags {
        let code = tag.replace('-', "_");
        seen.insert(code.clone());
        codes.push(code);
    }
    codes.join("-")
}

/// The generic modern-language manifest.
fn manifest_toml(spec: &LangSpec<'_>, has_tiers: bool) -> String {
    let ranges = if spec.script_ranges.is_empty() {
        LATIN_RANGES
    } else {
        spec.script_ranges
    };
    let ranges = ranges
        .iter()
        .map(|(a, b)| format!("[{a}, {b}]"))
        .collect::<Vec<_>>()
        .join(", ");
    let elisions = if spec.elisions.is_empty() {
        String::new()
    } else {
        format!(
            "elisions = [{}]\n",
            spec.elisions
                .iter()
                .map(|e| format!("\"{}\"", toml_escape(e)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let contractions = if spec.contractions.is_empty() {
        String::new()
    } else {
        format!(
            "contractions = {{ {} }}\n",
            spec.contractions
                .iter()
                .map(|(s, e)| format!("\"{}\" = \"{}\"", toml_escape(s), toml_escape(e)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let compounds = match spec.compound_linkers {
        None => String::new(),
        Some(linkers) => format!(
            "compounds = true\ncompound_linkers = [{}]\n",
            linkers
                .iter()
                .map(|l| format!("\"{}\"", toml_escape(l)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let graded_scheme = if has_tiers {
        "\n[graded_scheme]\nkey = \"frequency-tier\"\ndisplay = \"frequency tier\"\n"
    } else {
        ""
    };
    format!(
        r#"schema = 1
lang = "{lang}"
name = "{name}"
dict_source = "{source}"
description = "{description}"
license = "{license}"
joiner = " "
sentence_enders = [".", "?", "!", "…"]
script_ranges = [{ranges}]
{elisions}{contractions}{compounds}{graded_scheme}
[prompt]
language_name = "{name}"
chat_persona = "a friendly native {name} speaker"
citation_guidance = "When you cite {name}, add a brief English gloss in parentheses where helpful."
grammar_skeleton = "verb conjugation, agreement, word order"
immerse_instruction = "Write natural native {name} without simplification; the user wants full immersion."
"#,
        lang = toml_escape(spec.lang),
        name = toml_escape(spec.name),
        source = toml_escape(spec.dict_source),
        description = toml_escape(spec.description),
        license = toml_escape(spec.license),
    )
}

/// Escape a string for interpolation into a TOML basic string.
fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KAIKKI_ES_SAMPLE: &str = r#"
{"word":"gato","pos":"noun","lang_code":"es","senses":[{"glosses":["cat"],"examples":[{"text":"El gato duerme.","english":"The cat sleeps."}]},{"glosses":["sly person"],"tags":["colloquial","derogatory"]}],"sounds":[{"ipa":"/ˈɡa.to/"}],"forms":[{"form":"gatos","tags":["plural"]},{"form":"gata","tags":["feminine"]}]}
{"word":"gatos","pos":"noun","lang_code":"es","senses":[{"glosses":["plural of gato"],"tags":["form-of","plural"],"form_of":[{"word":"gato"}]}]}
{"word":"hablar","pos":"verb","lang_code":"es","senses":[{"glosses":["to speak, to talk"],"tags":["intransitive"]}],"forms":[{"form":"hablo","tags":["first-person","singular","present"]},{"form":"habló","tags":["third-person","singular","preterite"]}]}
{"word":"cantar","pos":"verb","lang_code":"es","senses":[{"glosses":["to sing"]}],"forms":[{"form":"canto","tags":["first-person","singular","present"]}]}
"#;

    fn spec() -> LangSpec<'static> {
        LangSpec {
            lang: "es",
            name: "Spanish",
            dict_source: "es-pack",
            license: "CC BY-SA 4.0",
            description: "A \"test\" build.",
            script_ranges: &[],
            elisions: &[],
            contractions: &[("au", "à le")],
            compound_linkers: None,
        }
    }

    #[test]
    fn builds_a_spanish_pack() {
        let dir = std::env::temp_dir().join(format!("shiori-kaikki-test-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("es.jsonl");
        std::fs::write(&input, KAIKKI_ES_SAMPLE).unwrap();
        // Subtitle-style "word count" lines: inflected forms carry the
        // mass, and the junk word "que" outranks nothing after folding
        // onto lemmas.
        let freq = dir.join("es_50k.txt");
        std::fs::write(&freq, "hablo 90\nque 80\ngatos 70\ngato 20\nhabló 15\n").unwrap();

        let out = dir.join("packs").join("es");
        let mut progress = Vec::new();
        let report = build_pack_with_progress(&input, Some(&freq), &spec(), &out, &mut |line| {
            progress.push(line.to_string())
        })
        .unwrap();
        assert_eq!(report.entries, 3, "gato, hablar, and cantar are lemmas");
        assert!(report.forms >= 5, "plural, feminine, and verb forms");
        assert!(progress.iter().any(|l| l.contains("grammar")));

        // Suffix rules: "-o → -ar" is attested by two distinct verbs
        // (count 2); one-off pairs stay out.
        let rules = std::fs::read_to_string(out.join("suffix_rules.tsv")).unwrap();
        assert!(rules.contains("o\tar\t2"), "{rules}");
        assert!(
            !rules.contains("a\to"),
            "single-pair rules are dropped: {rules}"
        );
        assert_eq!(report.suffix_rules, 1);

        // The pack loads; the manifest is sound, carries the escaped
        // description, and declares the generated tier scheme.
        let pack = crate::Pack::load(&out).unwrap();
        assert_eq!(pack.manifest.lang, "es");
        assert_eq!(pack.manifest.description, "A \"test\" build.");
        assert_eq!(
            pack.manifest.graded_scheme.as_ref().unwrap().key,
            "frequency-tier"
        );

        // The lemma table inverts both the form_of sense and the forms
        // array; accents fold in the key but the lemma stays exact, and
        // multi-word Wiktionary tags become single '-'-joinable codes.
        let forms = std::fs::read_to_string(out.join("morph_forms.tsv")).unwrap();
        assert!(forms.contains("gatos\tgato"), "{forms}");
        assert!(
            forms.contains("hablo\thablar\tfirst_person-singular-present"),
            "{forms}"
        );
        assert!(
            forms.contains("hablo\thablar\tthird_person-singular-preterite"),
            "folded habló joins hablo's rows: {forms}"
        );

        // The grammar decodes: multi-word tags get tags.tsv rows.
        let tags = std::fs::read_to_string(out.join("tags.tsv")).unwrap();
        assert!(tags.contains("first_person\tfirst person"), "{tags}");
        assert!(
            !tags.contains("singular\tsingular"),
            "prose codes need no row"
        );

        // Frequency is lemmatized: hablo's and habló's subtitle mass
        // lands on hablar (105), beating que (80) and gato (90 across
        // gatos+gato); no surface form appears as its own entry when it
        // resolves to a lemma.
        let frequency = std::fs::read_to_string(out.join("frequency.tsv")).unwrap();
        let rank_of = |w: &str| {
            frequency
                .lines()
                .find(|l| l.starts_with(&format!("{w}\t")))
                .and_then(|l| l.split('\t').nth(1))
                .map(|r| r.parse::<u32>().unwrap())
        };
        assert_eq!(rank_of("hablar"), Some(1), "{frequency}");
        assert_eq!(rank_of("gato"), Some(2), "{frequency}");
        assert_eq!(rank_of("que"), Some(3), "unresolved surfaces stay");
        assert_eq!(rank_of("hablo"), None, "resolved forms fold away");

        // Graded tiers cover the real lemmas, by exact spelling.
        let graded = std::fs::read_to_string(out.join("graded.tsv")).unwrap();
        assert!(graded.contains("1\tTop 500\thablar\t"), "{graded}");
        assert!(graded.contains("1\tTop 500\tgato\t"), "{graded}");
        assert!(!graded.contains("que"), "non-lemmas are not graded");
        assert_eq!(report.graded, 2);

        // The dictionary carries per-sense registers (as JMdict codes),
        // usage examples, and IPA tagged for the opt-in display.
        let dict = std::fs::read_to_string(out.join("dictionary.jsonl")).unwrap();
        assert!(dict.contains("to speak, to talk"));
        assert!(dict.contains(r#""misc":["col","derog"]"#), "{dict}");
        assert!(dict.contains("e.g. El gato duerme. — The cat sleeps."));
        assert!(dict.contains(r#""tags":["ipa"]"#), "{dict}");
        assert!(dict.contains("/ˈɡa.to/"), "{dict}");
        assert!(dict.contains(r#""partOfSpeech":["verb","intransitive"]"#));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_latin_script_ranges_and_elisions_reach_the_manifest() {
        let dir = std::env::temp_dir().join(format!("shiori-kaikki-ru-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("ru.jsonl");
        std::fs::write(
            &input,
            r#"{"word":"кот","pos":"noun","lang_code":"ru","senses":[{"glosses":["cat"]}]}"#,
        )
        .unwrap();
        let out = dir.join("packs").join("ru");
        let spec = LangSpec {
            lang: "ru",
            name: "Russian",
            dict_source: "ru-pack",
            license: "CC BY-SA 4.0",
            description: "",
            script_ranges: &[(0x0400, 0x04FF)],
            elisions: &["l", "d"],
            contractions: &[("im", "in dem")],
            compound_linkers: Some(&["s"]),
        };
        build_pack(&input, None, &spec, &out).unwrap();
        let pack = crate::Pack::load(&out).unwrap();
        assert_eq!(pack.manifest.script_ranges, vec![(0x0400, 0x04FF)]);
        assert_eq!(pack.manifest.elisions, vec!["l", "d"]);
        assert_eq!(
            pack.manifest.contractions.get("im").map(String::as_str),
            Some("in dem")
        );
        // No frequency list → no tier scheme claimed.
        assert!(pack.manifest.graded_scheme.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
