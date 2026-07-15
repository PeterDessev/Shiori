//! Language-pack services: discovery, data installation, annotated-text
//! import, and parse-code decoding.

use serde::Deserialize;
use shiori_core::{DocumentId, DocumentMeta};
use shiori_db::{DictFormRow, FormRole, NewSentence, NewToken};

use crate::{App, AppError, Result};

/// One line of a pack's `dictionary.jsonl`.
#[derive(Debug, Deserialize)]
struct PackDictLine {
    /// Entry key within the pack's dict source (lemma+homograph).
    key: String,
    /// Lookup forms, pre-folded with the pack's normalization.
    #[serde(default)]
    forms: Vec<PackDictForm>,
    /// jmdict-simplified-shaped word object, stored verbatim — the shape
    /// every dictionary view already renders.
    entry: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct PackDictForm {
    text: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    common: bool,
}

impl App {
    /// Import a pack's reference data into the database if it isn't
    /// there yet: dictionary, frequency list, tag decodings, graded
    /// vocabulary. Idempotent and scoped — never touches other
    /// languages' data.
    pub fn ensure_pack_data(&self, lang: &str) -> Result<()> {
        let Some(pack) = self.packs.get(lang) else {
            return Ok(()); // built-in language (Japanese); nothing to do
        };
        let source = pack.manifest.dict_source.clone();

        if self.db.dict_entry_count(&source)? == 0 {
            let path = pack.dictionary_path();
            if path.exists() {
                let raw = std::fs::read_to_string(&path)?;
                let entries = raw.lines().filter(|l| !l.trim().is_empty()).map(|line| {
                    let parsed: PackDictLine = serde_json::from_str(line).unwrap_or(PackDictLine {
                        key: String::new(),
                        forms: Vec::new(),
                        entry: serde_json::Value::Null,
                    });
                    let forms = parsed
                        .forms
                        .into_iter()
                        .map(|f| DictFormRow {
                            text: f.text,
                            role: FormRole::from_str_lossy(&f.role),
                            is_common: f.common,
                        })
                        .collect();
                    (parsed.key, parsed.entry.to_string(), forms)
                });
                self.db
                    .import_dictionary(&source, entries.filter(|(k, _, _)| !k.is_empty()))?;
            }
        }

        if self.db.frequency_count(lang)? == 0 {
            let path = pack.frequency_path();
            if path.exists() {
                let raw = std::fs::read_to_string(&path)?;
                let ranks = raw.lines().filter_map(|line| {
                    let (word, rank) = line.split_once('\t')?;
                    Some((word.trim(), rank.trim().parse::<u32>().ok()?))
                });
                self.db.import_frequency(lang, ranks)?;
            }
        }

        if self.db.morph_form_count(lang)? == 0 {
            let path = pack.dir.join("morph_forms.tsv");
            if path.exists() {
                let raw = std::fs::read_to_string(&path)?;
                let rows = raw.lines().filter_map(|line| {
                    let mut fields = line.split('\t');
                    let form = fields.next()?.trim().to_string();
                    let lemma = fields.next()?.trim().to_string();
                    let morph = fields.next().unwrap_or("").trim().to_string();
                    (!form.is_empty()).then_some((form, lemma, morph))
                });
                self.db.import_morph_forms(lang, rows)?;
            }
        }

        {
            let path = pack.tags_path();
            if path.exists() {
                let raw = std::fs::read_to_string(&path)?;
                let tags = raw.lines().filter_map(|line| {
                    let (code, label) = line.split_once('\t')?;
                    Some((code.trim().to_string(), label.trim().to_string()))
                });
                self.db.import_dict_tags(&source, tags)?;
            }
        }

        if let Some(scheme) = &pack.manifest.graded_scheme {
            if self.db.graded_count(lang, &scheme.key)? == 0 {
                let path = pack.graded_path();
                if path.exists() {
                    let raw = std::fs::read_to_string(&path)?;
                    let rows = raw.lines().filter_map(|line| {
                        let mut fields = line.split('\t');
                        let ord: u32 = fields.next()?.trim().parse().ok()?;
                        let label = fields.next()?.trim().to_string();
                        let form = fields.next()?.trim().to_string();
                        let alt = fields.next().unwrap_or("").trim().to_string();
                        Some((ord, label, form, alt))
                    });
                    self.db.import_graded_vocab(lang, &scheme.key, rows)?;
                }
            }
        }

        Ok(())
    }

    /// Import a SIAT pre-annotated text: every token arrives carrying
    /// lemma, parse code, and gloss — no analyzer runs at all.
    ///
    /// The document imports under the file's language, which must be the
    /// active one (so the reader's service and the text agree).
    pub fn import_siat_str(&self, jsonl: &str) -> Result<DocumentId> {
        let doc = shiori_pack::siat::parse(jsonl)
            .map_err(|e| AppError::Invalid(format!("bad annotated text: {e}")))?;
        if doc.header.lang != self.active_lang() {
            return Err(AppError::Invalid(format!(
                "this text is for language '{}' but the active language is '{}' — \
                 switch languages before importing it",
                doc.header.lang,
                self.active_lang()
            )));
        }

        let hash = crate::ingest::content_hash(jsonl);
        if let Some(existing) = self.db.find_document_by_hash(self.active_lang(), &hash)? {
            return Ok(existing);
        }

        let sentences: Vec<NewSentence> = doc
            .sentences
            .iter()
            .map(|s| NewSentence {
                paragraph: s.p,
                text: s.text.clone(),
                tokens: s
                    .tokens
                    .iter()
                    .map(|t| NewToken {
                        surface: t.s.clone(),
                        lemma: t.l.clone(),
                        reading: String::new(),
                        pos: shiori_pack::siat::pos_from_morph(&t.m),
                        start: t.start,
                        end: t.end,
                        morph: (!t.m.is_empty()).then(|| t.m.clone()),
                        gloss: (!t.g.is_empty()).then(|| t.g.clone()),
                    })
                    .collect(),
            })
            .collect();

        let meta = DocumentMeta {
            title: doc.header.title.clone(),
            author: doc.header.author.clone(),
            publisher: doc.header.license.clone(),
            published: String::new(),
        };
        Ok(self.db.import_document(
            self.active_lang(),
            &meta,
            &hash,
            chrono::Utc::now(),
            &sentences,
        )?)
    }

    /// Tier-1 lemma resolution for pack languages: look a surface form
    /// up in the full-form table. Returns `(lemma, parse)` when the form
    /// resolves to exactly one lemma (the parse only when it is also
    /// unique); ambiguous or unknown forms return `None` and the surface
    /// stands as its own lemma — safe, never wrong, refined by the
    /// candidate picker later.
    pub(crate) fn tier1_lemma(&self, surface: &str) -> Result<Option<(String, Option<String>)>> {
        if !self.packs.contains_key(self.active_lang()) {
            return Ok(None);
        }
        let folded = self.service().normalize_lookup(surface);
        let hits = self.db.morph_lookup(self.active_lang(), &folded)?;
        let mut lemmas: Vec<&str> = hits.iter().map(|(l, _)| l.as_str()).collect();
        lemmas.sort_unstable();
        lemmas.dedup();
        match lemmas.as_slice() {
            [single] => {
                let lemma = single.to_string();
                let morph = (hits.len() == 1).then(|| hits[0].1.clone());
                Ok(Some((lemma, morph)))
            }
            _ => Ok(None),
        }
    }

    /// Decode a stored parse code ("V-PAI-3S") into prose using the
    /// active dictionary source's tag table; unknown segments stay
    /// verbatim so nothing is ever hidden.
    pub fn describe_morph(&self, morph: &str) -> String {
        let source = self.active_dict_source().to_string();
        morph
            .split('-')
            .filter(|seg| !seg.is_empty())
            .map(|seg| {
                self.db
                    .dict_tag_label(&source, seg)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| seg.to_string())
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiori_core::{KnowledgeStatus, PartOfSpeech, WordKey};

    fn app() -> App {
        App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap()
    }

    /// Build an app whose data dir contains the sample Greek pack, and
    /// activate it.
    fn app_with_greek_pack() -> App {
        let dir = std::env::temp_dir().join(format!(
            "shiori-pack-test-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace("::", "-")
        ));
        let pack_dir = dir.join("packs").join("grc");
        std::fs::create_dir_all(pack_dir.join("texts")).unwrap();
        std::fs::write(
            pack_dir.join("manifest.toml"),
            shiori_pack::manifest::KOINE_GREEK_MANIFEST,
        )
        .unwrap();
        std::fs::write(
            pack_dir.join("dictionary.jsonl"),
            concat!(
                r#"{"key":"λόγος","forms":[{"text":"λογοσ","role":"canonical","common":true}],"entry":{"id":"λόγος","kanji":[{"common":true,"text":"λόγος","tags":[]}],"kana":[],"sense":[{"partOfSpeech":["noun (2nd declension)"],"gloss":[{"lang":"eng","text":"word, speech, account"}],"related":[],"antonym":[],"field":[],"dialect":[],"misc":[],"info":[]}]}}"#,
                "\n",
                r#"{"key":"ἀρχή","forms":[{"text":"αρχη","role":"canonical","common":true}],"entry":{"id":"ἀρχή","kanji":[{"common":true,"text":"ἀρχή","tags":[]}],"kana":[],"sense":[{"partOfSpeech":["noun (1st declension)"],"gloss":[{"lang":"eng","text":"beginning, origin"}],"related":[],"antonym":[],"field":[],"dialect":[],"misc":[],"info":[]}]}}"#,
                "\n",
            ),
        )
        .unwrap();
        std::fs::write(pack_dir.join("frequency.tsv"), "λογοσ\t5\nαρχη\t40\n").unwrap();
        // Tier-1 full-form table: ἦν → εἰμί unambiguously; a fake
        // ambiguous form to prove ambiguity stays untouched.
        std::fs::write(
            pack_dir.join("morph_forms.tsv"),
            "ην\tεἰμί\tV-IAI-3S\nαμφι\tἀμφί-α\tP\nαμφι\tἀμφί-β\tX\n",
        )
        .unwrap();
        std::fs::write(
            pack_dir.join("tags.tsv"),
            "V\tverb\nN\tnoun\nP\tpreposition\nRA\tarticle\nPAI\tpresent active indicative\nIAI\timperfect active indicative\nDSF\tdative singular feminine\nNSM\tnominative singular masculine\n3S\t3rd person singular\n",
        )
        .unwrap();
        std::fs::write(
            pack_dir.join("graded.tsv"),
            "1\tCore 50×+\tλογοσ\t\n2\tCore 20×+\tαρχη\t\n",
        )
        .unwrap();

        App::with_db(shiori_db::Db::open_in_memory().unwrap(), dir).unwrap()
    }

    #[test]
    fn japanese_is_always_available() {
        let app = app();
        let langs = app.available_languages();
        assert!(langs.iter().any(|(code, _)| code == "ja"));
        assert_eq!(app.active_lang(), "ja");
    }

    #[test]
    fn greek_pack_is_discovered_and_activates() {
        let mut app = app_with_greek_pack();
        let langs = app.available_languages();
        assert!(
            langs.iter().any(|(code, _)| code == "grc"),
            "pack language discovered: {langs:?}"
        );

        app.set_active_lang("grc").unwrap();
        assert_eq!(app.active_lang(), "grc");
        assert_eq!(app.active_dict_source(), "grc-pack");

        // Pack data was installed, scoped to the pack's source/lang.
        assert_eq!(app.db().dict_entry_count("grc-pack").unwrap(), 2);
        assert_eq!(app.db().frequency_count("grc").unwrap(), 2);
        assert_eq!(app.db().frequency_rank("grc", "λογοσ").unwrap(), Some(5));

        // Unknown languages are rejected.
        assert!(app.set_active_lang("tlh").is_err());
    }

    #[test]
    fn siat_import_needs_no_analyzer_and_survives_reload() {
        let mut app = app_with_greek_pack();
        app.set_active_lang("grc").unwrap();

        // Sample from the shiori-pack tests: John 1:1 with parse codes.
        let words: [(&str, &str, &str, &str); 5] = [
            ("Ἐν", "ἐν", "P", "in"),
            ("ἀρχῇ", "ἀρχή", "N-DSF", "beginning"),
            ("ἦν", "εἰμί", "V-IAI-3S", "was"),
            ("ὁ", "ὁ", "RA-NSM", ""),
            ("λόγος", "λόγος", "N-NSM", "word"),
        ];
        let mut text = String::new();
        let mut tokens = Vec::new();
        for (s, l, m, g) in words {
            if !text.is_empty() {
                text.push(' ');
            }
            let start = text.len();
            text.push_str(s);
            tokens.push(serde_json::json!({
                "s": s, "l": l, "m": m, "g": g, "start": start, "end": text.len()
            }));
        }
        let jsonl = format!(
            "{}\n{}\n",
            serde_json::json!({
                "siat": 1, "lang": "grc", "title": "ΚΑΤΑ ΙΩΑΝΝΗΝ",
                "license": "CC BY 4.0", "quality": "gold"
            }),
            serde_json::json!({"p": 0, "ref": "John.1.1", "text": text, "tokens": tokens})
        );

        let doc = app.import_siat_str(&jsonl).unwrap();
        // Re-import dedupes.
        assert_eq!(app.import_siat_str(&jsonl).unwrap(), doc);

        let sentences = app.db().sentences(doc).unwrap();
        assert_eq!(sentences.len(), 1);
        let rows = app.db().sentence_tokens(sentences[0].id).unwrap();
        assert_eq!(rows.len(), 5);
        // Stored tokens carry the annotations.
        assert_eq!(rows[1].morph.as_deref(), Some("N-DSF"));
        assert_eq!(rows[1].gloss.as_deref(), Some("beginning"));
        assert_eq!(rows[1].token.lemma, "ἀρχή");
        assert_eq!(rows[1].token.pos, PartOfSpeech::Noun);
        // The article and preposition are function words, not vocabulary.
        assert_eq!(rows[0].token.pos, PartOfSpeech::Preposition);
        assert!(!rows[0].token.pos.is_content_word());
        assert_eq!(rows[3].token.pos, PartOfSpeech::Article);

        // Words are scoped to grc and never collide with Japanese.
        let logos = app
            .db()
            .find_word("grc", &WordKey::new("λόγος", "", PartOfSpeech::Noun))
            .unwrap()
            .expect("λόγος tracked");
        assert_eq!(logos.status, KnowledgeStatus::Unknown);
        assert!(app
            .db()
            .find_word("ja", &WordKey::new("λόγος", "", PartOfSpeech::Noun))
            .unwrap()
            .is_none());

        // Parse codes decode through the pack's tag table.
        assert_eq!(
            app.describe_morph("V-IAI-3S"),
            "verb · imperfect active indicative · 3rd person singular"
        );

        // Mining finds the Greek content words via the frequency list.
        let candidates = app.mining_candidates(doc).unwrap();
        assert!(candidates.iter().any(|c| c.word.key.lemma == "λόγος"));
        let logos_cand = candidates
            .iter()
            .find(|c| c.word.key.lemma == "λόγος")
            .unwrap();
        assert_eq!(logos_cand.corpus_rank, Some(5));
        // And the dictionary resolves through folded lookup.
        assert!(
            logos_cand.entry.is_some(),
            "λόγος resolves in the pack dictionary"
        );

        // Graded shares work for the pack's scheme.
        let shares = app
            .db()
            .graded_known_shares("grc", "gnt-frequency")
            .unwrap();
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].label, "Core 50×+");
    }

    #[test]
    fn plain_greek_text_resolves_through_the_full_form_table() {
        let mut app = app_with_greek_pack();
        app.set_active_lang("grc").unwrap();

        // No annotations here: a pasted plain-text fragment.
        let doc = app
            .import_text("fragment", "ὁ λόγος ἦν καλός. ἀμφὶ δὲ ἦν.")
            .unwrap();
        let sentences = app.db().sentences(doc).unwrap();
        let rows = app.db().sentence_tokens(sentences[0].id).unwrap();

        // ἦν is unambiguous in the table: lemma εἰμί, parse V-IAI-3S,
        // POS from the parse code.
        let en = rows.iter().find(|r| r.token.surface == "ἦν").unwrap();
        assert_eq!(en.token.lemma, "εἰμί");
        assert_eq!(en.morph.as_deref(), Some("V-IAI-3S"));
        assert_eq!(en.token.pos, PartOfSpeech::Verb);

        // Unknown forms keep their surface as lemma (never wrong).
        let kalos = rows.iter().find(|r| r.token.surface == "καλός").unwrap();
        assert_eq!(kalos.token.lemma, "καλός");

        // Ambiguous forms stay untouched too.
        let rows2 = app.db().sentence_tokens(sentences[1].id).unwrap();
        let amphi = rows2.iter().find(|r| r.token.surface == "ἀμφὶ").unwrap();
        assert_eq!(amphi.token.lemma, "ἀμφὶ");
        assert_eq!(amphi.morph, None);
    }

    #[test]
    fn siat_for_the_wrong_language_is_rejected() {
        let app = app(); // active language: ja
        let jsonl = concat!(
            r#"{"siat":1,"lang":"grc","title":"x"}"#,
            "\n",
            r#"{"p":0,"ref":"","text":"","tokens":[]}"#,
            "\n"
        );
        let err = app.import_siat_str(jsonl).unwrap_err();
        assert!(err.to_string().contains("switch languages"));
    }
}
