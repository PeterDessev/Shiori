//! Language-pack services: discovery, installation and removal, data
//! import, annotated-text import, and parse-code decoding.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use shiori_core::{DocumentId, DocumentMeta};
use shiori_db::{DictFormRow, FormRole, NewSentence, NewToken};

use crate::{App, AppError, Result};

/// Refuse pack downloads larger than this (a full pack with texts is
/// tens of megabytes; anything near this size is not a pack).
const MAX_PACK_DOWNLOAD_BYTES: u64 = 500 * 1024 * 1024;

/// A catalog is a small JSON document; refuse anything bigger.
const MAX_CATALOG_BYTES: u64 = 10 * 1024 * 1024;

/// Where the hosted pack catalog lives unless the user points somewhere
/// else: a release asset on the app repository, re-uploaded under the
/// same tag as packs publish.
pub const DEFAULT_PACK_CATALOG_URL: &str =
    "https://github.com/PeterDessev/Shiori/releases/download/pack-catalog/catalog.json";

/// Cached copy of the catalog inside the data directory, so browsing
/// works offline once it has been fetched.
pub const PACK_CATALOG_FILENAME: &str = "pack-catalog.json";

/// One language the app can operate in, as the languages settings page
/// presents it.
#[derive(Debug, Clone)]
pub struct LanguageInfo {
    pub lang: String,
    pub name: String,
    pub active: bool,
    /// `None` for the built-in Japanese.
    pub pack: Option<PackDetails>,
}

/// What an installed pack ships, read off its directory and manifest.
#[derive(Debug, Clone)]
pub struct PackDetails {
    pub dir: PathBuf,
    pub license: String,
    /// Bundled pre-annotated texts.
    pub text_count: usize,
    pub has_dictionary: bool,
    pub has_frequency: bool,
    /// Full-form table for Tier-1 analysis of plain text.
    pub has_morphology: bool,
    /// Display name of the graded-vocabulary scheme, if any.
    pub graded_scheme: Option<String>,
    /// Font families the pack declares for its script.
    pub fonts: Vec<String>,
}

pub use shiori_pack::catalog::{parse_pack_catalog, PackCatalogEntry};
use shiori_pack::is_safe_lang_code;

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
    /// languages' data. Heavy for a full-size pack (hundreds of
    /// thousands of rows): run on a worker thread, not the frame loop.
    pub fn ensure_pack_data(&self, lang: &str) -> Result<()> {
        self.ensure_pack_data_with_progress(lang, &mut |_| {})
    }

    /// Like [`Self::ensure_pack_data`], reporting a line per stage.
    pub fn ensure_pack_data_with_progress(
        &self,
        lang: &str,
        on_progress: &mut dyn FnMut(&str),
    ) -> Result<()> {
        let Some(pack) = self.packs.get(lang) else {
            return Ok(()); // built-in language (Japanese); nothing to do
        };
        let source = pack.manifest.dict_source.clone();

        if self.db.dict_entry_count(&source)? == 0 {
            let path = pack.dictionary_path();
            if path.exists() {
                on_progress("Importing dictionary…");
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
        // A pack that yields no dictionary at all is broken (files
        // missing or unreadable) — fail loudly instead of activating a
        // language that then looks like it merely needs a download.
        if self.db.dict_entry_count(&source)? == 0 {
            return Err(AppError::Invalid(format!(
                "the '{lang}' pack's data files are missing or empty \
                 ({}); reinstall or rebuild the pack",
                pack.dictionary_path().display()
            )));
        }

        if self.db.frequency_count(lang)? == 0 {
            let path = pack.frequency_path();
            if path.exists() {
                on_progress("Importing frequency list…");
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
                on_progress("Importing grammar tables…");
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

    /// Where installed packs live: `<data>/packs/<lang>/`.
    pub fn packs_dir(&self) -> PathBuf {
        self.data_dir.join("packs")
    }

    /// Every language the app can operate in, Japanese first, packs
    /// alphabetically, with what each pack ships.
    pub fn language_infos(&self) -> Vec<LanguageInfo> {
        let mut out = vec![LanguageInfo {
            lang: "ja".to_string(),
            name: "Japanese".to_string(),
            active: self.active == "ja",
            pack: None,
        }];
        let mut packs: Vec<_> = self.packs.values().collect();
        packs.sort_by(|a, b| a.manifest.lang.cmp(&b.manifest.lang));
        for pack in packs {
            out.push(LanguageInfo {
                lang: pack.manifest.lang.clone(),
                name: pack.manifest.name.clone(),
                active: self.active == pack.manifest.lang,
                pack: Some(PackDetails {
                    dir: pack.dir.clone(),
                    license: pack.manifest.license.clone(),
                    text_count: pack.text_paths().len(),
                    has_dictionary: pack.dictionary_path().exists(),
                    has_frequency: pack.frequency_path().exists(),
                    has_morphology: pack.dir.join("morph_forms.tsv").exists(),
                    graded_scheme: pack
                        .manifest
                        .graded_scheme
                        .as_ref()
                        .map(|s| s.display.clone()),
                    fonts: pack.manifest.fonts.iter().map(|f| f.name.clone()).collect(),
                }),
            });
        }
        out
    }

    /// Install a pack from a directory containing `manifest.toml`: the
    /// directory is copied into `<data>/packs/<lang>/` (replacing any
    /// previous version of that pack) and the language becomes available
    /// immediately, no restart needed. Returns the language code.
    pub fn install_pack_from_dir(&mut self, src: &Path) -> Result<String> {
        let pack = shiori_pack::Pack::load(src)
            .map_err(|e| AppError::Invalid(format!("not a valid language pack: {e}")))?;
        let lang = pack.manifest.lang.clone();
        if lang == "ja" {
            return Err(AppError::Invalid(
                "Japanese is built in and cannot be replaced by a pack".into(),
            ));
        }
        // The code becomes a path component under packs/ (and the target
        // of a replace-existing delete) — never let a hostile manifest
        // smuggle in separators, "..", or an absolute path.
        if !is_safe_lang_code(&lang) {
            return Err(AppError::Invalid(format!(
                "pack declares an unusable language code '{lang}'"
            )));
        }
        // Replacing a pack purges its dict source; the built-in
        // Japanese dictionary must never be claimable.
        if pack.manifest.dict_source == "jmdict" {
            return Err(AppError::Invalid(
                "packs cannot claim the built-in 'jmdict' dictionary source".into(),
            ));
        }
        let dest = self.packs_dir().join(&lang);
        let already_in_place = dest.exists() && src.canonicalize().ok() == dest.canonicalize().ok();
        let replacing = dest.exists() && !already_in_place;
        if !already_in_place {
            if replacing {
                trash_and_remove(&dest).map_err(|e| {
                    AppError::Invalid(format!(
                        "could not replace the previous '{lang}' pack: {e} — is its \
                         folder open in another program?"
                    ))
                })?;
            }
            copy_dir(src, &dest)?;
        }
        let pack = shiori_pack::Pack::load(&dest)
            .map_err(|e| AppError::Invalid(format!("installed pack failed to load: {e}")))?;
        // A replaced pack's reference data is stale (a different pack
        // may live under the same code now); purge it so the next
        // activation imports this pack's data. Library, vocabulary, and
        // review history are separate and untouched.
        if replacing {
            self.db
                .purge_reference_data(&lang, &pack.manifest.dict_source)?;
            self.suffix_rules.remove(&lang);
        }
        self.services.insert(
            lang.clone(),
            std::sync::Arc::new(shiori_pack::PackLanguage::new(&pack.manifest)),
        );
        self.packs.insert(lang.clone(), pack);
        Ok(lang)
    }

    /// Install a pack from a zip archive whose root (or a single
    /// top-level folder) contains `manifest.toml`.
    pub fn install_pack_from_zip(&mut self, zip_path: &Path) -> Result<String> {
        let file = std::fs::File::open(zip_path)?;
        self.install_pack_from_zip_reader(file)
    }

    /// Install a pack from a zip already in memory (a finished
    /// [`download_pack_zip`]).
    pub fn install_pack_from_zip_bytes(&mut self, bytes: &[u8]) -> Result<String> {
        self.install_pack_from_zip_reader(std::io::Cursor::new(bytes))
    }

    fn install_pack_from_zip_reader<R: std::io::Read + std::io::Seek>(
        &mut self,
        reader: R,
    ) -> Result<String> {
        let mut archive = zip::ZipArchive::new(reader)
            .map_err(|e| AppError::Invalid(format!("not a readable zip archive: {e}")))?;
        // Decompression-bomb guard: check the declared sizes before
        // anything is written to disk.
        const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
        const MAX_ENTRIES: usize = 10_000;
        if archive.len() > MAX_ENTRIES {
            return Err(AppError::Invalid(
                "the archive contains too many files to be a language pack".into(),
            ));
        }
        let unpacked: u64 = (0..archive.len())
            .map(|i| archive.by_index(i).map(|f| f.size()).unwrap_or(0))
            .sum();
        if unpacked > MAX_UNPACKED_BYTES {
            return Err(AppError::Invalid(
                "the archive unpacks too large to be a language pack".into(),
            ));
        }
        // Stage outside packs/ so a crashed install can never leave a
        // half-extracted directory where pack discovery would find it.
        let staging = self
            .data_dir
            .join(format!(".pack-staging-{}", std::process::id()));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(&staging)?;
        let result = (|| {
            archive
                .extract(&staging)
                .map_err(|e| AppError::Invalid(format!("could not extract the pack: {e}")))?;
            let root = find_manifest_root(&staging)
                .ok_or_else(|| AppError::Invalid("no manifest.toml found in the archive".into()))?;
            self.install_pack_from_dir(&root)
        })();
        std::fs::remove_dir_all(&staging).ok();
        result
    }

    /// Download a pack zip and install it. When `expected_sha256` is
    /// given (hex), the download is verified against it before anything
    /// is unpacked. Callers holding a lock on the app (the GUI) should
    /// instead run [`download_pack_zip`] unlocked and pass the bytes to
    /// [`Self::install_pack_from_zip_bytes`].
    pub fn install_pack_from_url(
        &mut self,
        url: &str,
        expected_sha256: Option<&str>,
    ) -> Result<String> {
        let bytes = download_pack_zip(url, expected_sha256)?;
        self.install_pack_from_zip_bytes(&bytes)
    }

    /// Uninstall a pack: its directory is deleted and the language
    /// disappears from the app. Library, vocabulary, and review history
    /// for the language stay in the database and reappear if the pack is
    /// reinstalled. The active language cannot be removed.
    pub fn remove_pack(&mut self, lang: &str) -> Result<()> {
        if lang == self.active {
            return Err(AppError::Invalid(
                "switch to another language before removing the active one".into(),
            ));
        }
        let Some(pack) = self.packs.get(lang) else {
            return Err(AppError::Invalid(format!("no pack '{lang}' is installed")));
        };
        // Delete first, deregister after — and delete by renaming the
        // directory aside before removing it, so a locked file (the
        // folder open in Explorer) fails the whole operation cleanly
        // instead of gutting the pack halfway and leaving a zombie.
        let dir = pack.dir.clone();
        let dict_source = pack.manifest.dict_source.clone();
        trash_and_remove(&dir).map_err(|e| {
            AppError::Invalid(format!(
                "could not remove the '{lang}' pack: {e} — is its folder open \
                 in another program?"
            ))
        })?;
        // Reference data reimports on reinstall; the user's words,
        // library, and review history are separate and stay.
        self.db.purge_reference_data(lang, &dict_source)?;
        self.packs.remove(lang);
        self.services.remove(lang);
        self.suffix_rules.remove(lang);
        Ok(())
    }

    /// Import every text bundled with the active language's pack into
    /// the library. Returns (newly imported, already present).
    pub fn import_pack_texts(&self) -> Result<(usize, usize)> {
        let Some(pack) = self.packs.get(self.active_lang()) else {
            return Err(AppError::Invalid(
                "the active language has no pack with bundled texts".into(),
            ));
        };
        let mut new = 0;
        let mut existing = 0;
        for path in pack.text_paths() {
            let jsonl = std::fs::read_to_string(&path)?;
            let hash = crate::ingest::content_hash(&jsonl);
            if self
                .db
                .find_document_by_hash(self.active_lang(), &hash)?
                .is_some()
            {
                existing += 1;
            } else {
                self.import_siat_str(&jsonl)?;
                new += 1;
            }
        }
        Ok((new, existing))
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
    /// up in the full-form table. Unambiguous forms return their lemma
    /// (with the parse when it is also unique). Ambiguous forms fall
    /// back to corpus frequency: when exactly one candidate lemma ranks
    /// best, it wins — *hablo* resolves to the everyday verb, not an
    /// obscure homograph. Forms that stay ambiguous (no ranks, or a
    /// tie) return `None` and the surface stands as its own lemma —
    /// safe, never wrong.
    pub(crate) fn tier1_lemma(&self, surface: &str) -> Result<Option<(String, Option<String>)>> {
        if !self.packs.contains_key(self.active_lang()) {
            return Ok(None);
        }
        let folded = self.service().normalize_lookup(surface);
        let hits = self.db.morph_lookup(self.active_lang(), &folded)?;
        let mut lemmas: Vec<&str> = hits.iter().map(|(l, _)| l.as_str()).collect();
        lemmas.sort_unstable();
        lemmas.dedup();

        let chosen: &str = match lemmas.as_slice() {
            // Not in the table at all: try the learned suffix rules.
            [] => return self.suffix_guess(&folded),
            [single] => single,
            many => {
                let mut best: Option<(u32, &str)> = None;
                let mut tie = false;
                for lemma in many {
                    let key = self.service().normalize_lookup(lemma);
                    let Some(rank) = self.db.frequency_rank(self.active_lang(), &key)? else {
                        continue;
                    };
                    match best {
                        None => best = Some((rank, lemma)),
                        Some((br, _)) if rank < br => {
                            best = Some((rank, lemma));
                            tie = false;
                        }
                        Some((br, _)) if rank == br => tie = true,
                        Some(_) => {}
                    }
                }
                match best {
                    Some((_, lemma)) if !tie => lemma,
                    _ => return Ok(None),
                }
            }
        };
        let mut morphs = hits
            .iter()
            .filter(|(l, _)| l == chosen)
            .map(|(_, m)| m.as_str());
        let first = morphs.next().map(str::to_string);
        // The parse is only trustworthy when this lemma has exactly one
        // row for the form.
        let morph = if morphs.next().is_none() { first } else { None };
        Ok(Some((chosen.to_string(), morph)))
    }

    /// Every candidate analysis the full-form table holds for a surface
    /// form, as (lemma, parse code) pairs — the reader's picker offers
    /// these when a form is ambiguous.
    pub fn tier1_candidates(&self, surface: &str) -> Result<Vec<(String, String)>> {
        if !self.packs.contains_key(self.active_lang()) {
            return Ok(Vec::new());
        }
        let folded = self.service().normalize_lookup(surface);
        let mut hits = self.db.morph_lookup(self.active_lang(), &folded)?;
        hits.sort();
        hits.dedup();
        Ok(hits)
    }

    /// Apply a picked candidate to one token occurrence: the token's
    /// word association (created if needed) and stored parse become the
    /// chosen analysis, so tracking, mining, and statistics follow the
    /// corrected identity from here on.
    pub fn reassign_occurrence(
        &self,
        sentence: shiori_core::SentenceId,
        idx: usize,
        lemma: &str,
        morph: Option<&str>,
    ) -> Result<shiori_core::WordId> {
        let pos = shiori_pack::siat::pos_from_morph(morph.unwrap_or(""));
        let word = self.db.ensure_word(
            self.active_lang(),
            &shiori_core::WordKey::new(lemma, "", pos),
        )?;
        self.db
            .reassign_token(sentence, idx as u32, word.id, morph)?;
        Ok(word.id)
    }

    /// Last-resort Tier-1 for a form absent from the full-form table:
    /// apply the pack's learned suffix rules ("-o" rewrites to "-ar"),
    /// accepting a guess only when the rewritten lemma exists in the
    /// dictionary unambiguously. No parse is claimed for guesses.
    fn suffix_guess(&self, folded: &str) -> Result<Option<(String, Option<String>)>> {
        let Some(rules) = self.suffix_rules.get(self.active_lang()) else {
            return Ok(None);
        };
        let source = self.active_dict_source();
        let mut tried = 0;
        for (form_suffix, lemma_suffix) in rules {
            if !folded.ends_with(form_suffix.as_str()) {
                continue;
            }
            let stem = &folded[..folded.len() - form_suffix.len()];
            if stem.chars().count() < 3 {
                continue;
            }
            tried += 1;
            if tried > 30 {
                break;
            }
            let candidate = format!("{stem}{lemma_suffix}");
            if let [single] = self.db.dict_form_entry_keys(source, &candidate)?.as_slice() {
                return Ok(Some((single.clone(), None)));
            }
        }
        Ok(None)
    }

    /// Split an unknown word into known dictionary words, for packs
    /// that enable it (Germanic compounds): "Kaffeemaschine" → kaffee +
    /// maschine, with the manifest's linking elements ("s" in
    /// "Arbeitsmaschine") allowed between parts. Display-level only —
    /// the compound itself stays the tracked word. `None` when the
    /// language doesn't split, the word is too short/long, or no full
    /// cover into ≥2 known parts exists.
    pub fn decompose_compound(&self, surface: &str) -> Result<Option<Vec<String>>> {
        let Some(pack) = self.packs.get(self.active_lang()) else {
            return Ok(None);
        };
        if !pack.manifest.compounds {
            return Ok(None);
        }
        let folded = self.service().normalize_lookup(surface);
        let n_chars = folded.chars().count();
        if !(7..=48).contains(&n_chars) {
            return Ok(None);
        }
        let source = self.active_dict_source();
        let linkers: Vec<&str> = pack
            .manifest
            .compound_linkers
            .iter()
            .map(String::as_str)
            .collect();
        let bounds: Vec<usize> = folded
            .char_indices()
            .map(|(i, _)| i)
            .chain([folded.len()])
            .collect();

        /// Depth-first cover of `folded[bounds[from]..]` by dictionary
        /// words (longest part first), memoizing dead starts.
        #[allow(clippy::too_many_arguments)] // recursion context, not an API
        fn parts_from(
            db: &shiori_db::Db,
            source: &str,
            folded: &str,
            bounds: &[usize],
            from: usize,
            linkers: &[&str],
            dead: &mut std::collections::HashSet<usize>,
            out: &mut Vec<String>,
        ) -> Result<bool> {
            let last = bounds.len() - 1;
            if from == last {
                return Ok(true);
            }
            if dead.contains(&from) {
                return Ok(false);
            }
            for j in ((from + 3)..=last).rev() {
                // The degenerate "whole word is one part" case is the
                // caller's normal lookup, not a compound.
                if from == 0 && j == last {
                    continue;
                }
                let part = &folded[bounds[from]..bounds[j]];
                if db.dict_form_entry_keys(source, part)?.is_empty() {
                    continue;
                }
                let mut nexts = vec![j];
                for linker in linkers {
                    let l_chars = linker.chars().count();
                    let next = j + l_chars;
                    // A linker must be followed by another part.
                    if next < last && &folded[bounds[j]..bounds[next]] == *linker {
                        nexts.push(next);
                    }
                }
                for next in nexts {
                    out.push(part.to_string());
                    if parts_from(db, source, folded, bounds, next, linkers, dead, out)? {
                        return Ok(true);
                    }
                    out.pop();
                }
            }
            dead.insert(from);
            Ok(false)
        }

        let mut out = Vec::new();
        let ok = parts_from(
            &self.db,
            source,
            &folded,
            &bounds,
            0,
            &linkers,
            &mut std::collections::HashSet::new(),
            &mut out,
        )?;
        Ok((ok && out.len() >= 2).then_some(out))
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

/// Download a pack zip into memory, verifying an optional SHA-256 (hex).
/// A free function on purpose: the GUI runs it without holding the app
/// lock, so a slow download never freezes the interface.
pub fn download_pack_zip(url: &str, expected_sha256: Option<&str>) -> Result<Vec<u8>> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::Invalid("no download URL given".into()));
    }
    // Connect/read timeouts (not a total timeout: big packs on slow
    // links are legitimate) so a stalled connection fails instead of
    // pinning the "installing…" state for the whole session.
    let agent = ureq::AgentBuilder::new()
        .user_agent("shiori/0.1")
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| AppError::Invalid(format!("download failed: {e}")))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_PACK_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PACK_DOWNLOAD_BYTES {
        return Err(AppError::Invalid(
            "the download is too large to be a language pack".into(),
        ));
    }
    if let Some(expected) = expected_sha256.map(str::trim).filter(|s| !s.is_empty()) {
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(AppError::Invalid(format!(
                "checksum mismatch — expected {expected}, got {actual}; \
                 the download may be corrupt or tampered with"
            )));
        }
    }
    Ok(bytes)
}

/// Parse a catalog document with the shared schema, mapping errors into
/// the app's error type.
fn parse_catalog(json: &str) -> Result<Vec<PackCatalogEntry>> {
    parse_pack_catalog(json).map_err(|e| AppError::Invalid(e.to_string()))
}

/// Fetch the hosted pack catalog, caching it in the data directory the
/// way the Aozora catalog is cached: the cached copy serves until a
/// forced refresh, and a refresh that fails falls back to it, so
/// browsing works offline. A free function on purpose — the GUI calls
/// it without holding the app lock. `url` empty means the default
/// catalog.
pub fn fetch_pack_catalog(
    data_dir: &Path,
    url: &str,
    force: bool,
) -> Result<Vec<PackCatalogEntry>> {
    let url = match url.trim() {
        "" => DEFAULT_PACK_CATALOG_URL,
        trimmed => trimmed,
    };
    let target = data_dir.join(PACK_CATALOG_FILENAME);
    if force || !target.exists() {
        // A download that succeeds but does not parse (an HTML page, a
        // wrong URL) is just as much a failed refresh as a network
        // error: keep serving the cached copy when there is one, and
        // only surface the failure on a cold fetch with nothing cached.
        let refreshed = download_catalog_json(url).and_then(|json| {
            parse_catalog(&json).map_err(|e| {
                AppError::Invalid(format!(
                    "{e} — check that the catalog URL points at the \
                     catalog.json itself ({url})"
                ))
            })?;
            Ok(json)
        });
        match refreshed {
            Ok(json) => {
                std::fs::create_dir_all(data_dir)?;
                let tmp = target.with_extension("part");
                std::fs::write(&tmp, &json)?;
                std::fs::rename(&tmp, &target)?;
            }
            Err(e) if !target.exists() => return Err(e),
            Err(_) => {}
        }
    }
    parse_catalog(&std::fs::read_to_string(&target)?)
}

fn download_catalog_json(url: &str) -> Result<String> {
    let agent = ureq::AgentBuilder::new()
        .user_agent("shiori/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| AppError::Invalid(format!("catalog download failed: {e}")))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_CATALOG_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        return Err(AppError::Invalid(
            "the download is too large to be a pack catalog".into(),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError::Invalid("the pack catalog is not valid UTF-8".into()))
}

/// Delete a directory by renaming it aside first. The rename either
/// succeeds atomically or fails before anything inside is touched — a
/// held handle (Explorer sitting in the folder) can no longer abort a
/// recursive delete halfway and gut the directory. A trash directory
/// whose removal fails is swept on the next startup.
fn trash_and_remove(dir: &Path) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    if !dir.exists() {
        return Ok(());
    }
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let trash = dir.with_file_name(format!(".trash-{}-{n}", std::process::id()));
    std::fs::rename(dir, &trash)?;
    std::fs::remove_dir_all(&trash).ok();
    Ok(())
}

/// Remove leftovers of interrupted pack operations: staging/build/
/// download temporaries in the data directory and `.trash-*` renames
/// under packs/. Runs at startup; every live temporary embeds the
/// current process id and is created after this sweep.
pub(crate) fn sweep_pack_leftovers(data_dir: &Path) {
    let sweep = |dir: &Path, prefix: &str| {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                let path = entry.path();
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).ok();
                } else {
                    std::fs::remove_file(&path).ok();
                }
            }
        }
    };
    sweep(data_dir, ".pack-");
    sweep(&data_dir.join("packs"), ".trash-");
}

/// Load a pack's learned suffix rules, longest form-suffix first (so
/// the most specific rewrite wins), capped to keep guessing cheap.
pub(crate) fn load_suffix_rules(path: &Path) -> Vec<(String, String)> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut rules: Vec<(String, String, u32)> = raw
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let form_suffix = fields.next()?.trim();
            let lemma_suffix = fields.next()?.trim().to_string();
            let count: u32 = fields.next().unwrap_or("0").trim().parse().ok()?;
            (!form_suffix.is_empty()).then(|| (form_suffix.to_string(), lemma_suffix, count))
        })
        .collect();
    rules.sort_by(|a, b| {
        b.0.chars()
            .count()
            .cmp(&a.0.chars().count())
            .then_with(|| b.2.cmp(&a.2))
    });
    rules.truncate(2000);
    rules.into_iter().map(|(f, l, _)| (f, l)).collect()
}

/// Recursively copy a directory tree.
fn copy_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The directory holding `manifest.toml` inside an extracted archive:
/// the root itself, or one of its immediate subdirectories (zips often
/// wrap the pack in a single top-level folder).
fn find_manifest_root(dir: &Path) -> Option<PathBuf> {
    if dir.join("manifest.toml").exists() {
        return Some(dir.to_path_buf());
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("manifest.toml").exists())
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

/// Lowercase hex SHA-256 of a byte buffer.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
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

    /// Unique per-test temp directory (process id + thread name).
    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shiori-{tag}-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace("::", "-")
        ))
    }

    /// Write the sample Greek pack's files into `pack_dir`.
    fn write_sample_pack(pack_dir: &Path) {
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
        std::fs::write(
            pack_dir.join("frequency.tsv"),
            "λογοσ\t5\nξυνα\t3\nαρχη\t40\n",
        )
        .unwrap();
        // Tier-1 full-form table: ἦν → εἰμί unambiguously; a fake
        // ambiguous form to prove ambiguity stays untouched.
        std::fs::write(
            pack_dir.join("morph_forms.tsv"),
            "ην\tεἰμί\tV-IAI-3S\nαμφι\tἀμφί-α\tP\nαμφι\tἀμφί-β\tX\nξυν\tξύνα\tP\nξυν\tξύνβ\tX\n",
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
        // Learned suffix rule: accusative -ον rewrites to nominative -οσ.
        std::fs::write(pack_dir.join("suffix_rules.tsv"), "ον\tοσ\t5\n").unwrap();
    }

    /// Build an app whose data dir contains the sample Greek pack.
    fn app_with_greek_pack() -> App {
        let dir = unique_dir("pack-test");
        write_sample_pack(&dir.join("packs").join("grc"));
        App::with_db(shiori_db::Db::open_in_memory().unwrap(), dir).unwrap()
    }

    /// John 1:1 as a SIAT document, offsets computed to tile the text.
    fn sample_siat_jsonl() -> String {
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
        format!(
            "{}\n{}\n",
            serde_json::json!({
                "siat": 1, "lang": "grc", "title": "ΚΑΤΑ ΙΩΑΝΝΗΝ",
                "license": "CC BY 4.0", "quality": "gold"
            }),
            serde_json::json!({"p": 0, "ref": "John.1.1", "text": text, "tokens": tokens})
        )
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
        app.ensure_pack_data("grc").unwrap();
        assert_eq!(app.active_lang(), "grc");
        assert_eq!(app.active_dict_source(), "grc-pack");

        // Pack data was installed, scoped to the pack's source/lang.
        assert_eq!(app.db().dict_entry_count("grc-pack").unwrap(), 2);
        assert_eq!(app.db().frequency_count("grc").unwrap(), 3);
        assert_eq!(app.db().frequency_rank("grc", "λογοσ").unwrap(), Some(5));

        // Unknown languages are rejected.
        assert!(app.set_active_lang("tlh").is_err());
    }

    #[test]
    fn siat_import_needs_no_analyzer_and_survives_reload() {
        let mut app = app_with_greek_pack();
        app.set_active_lang("grc").unwrap();
        app.ensure_pack_data("grc").unwrap();

        // Sample from the shiori-pack tests: John 1:1 with parse codes.
        let jsonl = sample_siat_jsonl();
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
        app.ensure_pack_data("grc").unwrap();

        // No annotations here: a pasted plain-text fragment.
        let doc = app
            .import_text(
                "fragment",
                "ὁ λόγος ἦν καλός. ἀμφὶ δὲ ἦν. ξὺν δὲ ἦν. λόγον δὲ ἦν.",
            )
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

        // Ambiguous forms with no frequency signal stay untouched.
        let rows2 = app.db().sentence_tokens(sentences[1].id).unwrap();
        let amphi = rows2.iter().find(|r| r.token.surface == "ἀμφὶ").unwrap();
        assert_eq!(amphi.token.lemma, "ἀμφὶ");
        assert_eq!(amphi.morph, None);

        // Ambiguous forms where one candidate is corpus-ranked resolve
        // to it: ξυν → ξύνα (rank 3) beats the unranked ξύνβ, and the
        // winning lemma's single row supplies the parse.
        let rows3 = app.db().sentence_tokens(sentences[2].id).unwrap();
        let xyn = rows3.iter().find(|r| r.token.surface == "ξὺν").unwrap();
        assert_eq!(xyn.token.lemma, "ξύνα");
        assert_eq!(xyn.morph.as_deref(), Some("P"));

        // A form absent from the table entirely resolves through the
        // learned suffix rules: λόγον rewrites -ον → -οσ, and the
        // dictionary confirms λογοσ → λόγος unambiguously. No parse is
        // claimed for a guess.
        let rows4 = app.db().sentence_tokens(sentences[3].id).unwrap();
        let logon = rows4.iter().find(|r| r.token.surface == "λόγον").unwrap();
        assert_eq!(logon.token.lemma, "λόγος");
        assert_eq!(logon.morph, None);

        // The candidate picker: the unresolved ἀμφὶ lists both
        // analyses, and applying one fixes exactly that occurrence.
        let candidates = app.tier1_candidates("ἀμφὶ").unwrap();
        assert_eq!(candidates.len(), 2);
        let amphi_idx = rows2
            .iter()
            .position(|r| r.token.surface == "ἀμφὶ")
            .unwrap();
        app.reassign_occurrence(sentences[1].id, amphi_idx, "ἀμφί-β", Some("X"))
            .unwrap();
        let rows2b = app.db().sentence_tokens(sentences[1].id).unwrap();
        let amphi = rows2b.iter().find(|r| r.token.surface == "ἀμφὶ").unwrap();
        assert_eq!(amphi.token.lemma, "ἀμφί-β");
        assert_eq!(amphi.morph.as_deref(), Some("X"));
    }

    #[test]
    fn pack_installs_from_directory_and_removes() {
        let data_dir = unique_dir("pack-install");
        std::fs::remove_dir_all(&data_dir).ok();
        let src = data_dir.join("incoming").join("grc-src");
        write_sample_pack(&src);

        let mut app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();
        assert_eq!(app.available_languages().len(), 1);

        // Live install: the language is usable without a restart.
        assert_eq!(app.install_pack_from_dir(&src).unwrap(), "grc");
        assert!(data_dir
            .join("packs")
            .join("grc")
            .join("manifest.toml")
            .exists());
        app.set_active_lang("grc").unwrap();
        app.ensure_pack_data("grc").unwrap();
        assert_eq!(app.db().dict_entry_count("grc-pack").unwrap(), 2);

        // language_infos reports what the pack ships.
        let infos = app.language_infos();
        assert_eq!(infos[0].lang, "ja");
        assert!(infos[0].pack.is_none());
        let grc = infos.iter().find(|i| i.lang == "grc").unwrap();
        assert!(grc.active);
        let details = grc.pack.as_ref().unwrap();
        assert!(details.has_dictionary && details.has_frequency && details.has_morphology);
        assert_eq!(details.graded_scheme.as_deref(), Some("GNT tier"));

        // The active language cannot be removed; after switching away it
        // can, and its directory is gone.
        assert!(app.remove_pack("grc").is_err());
        app.set_active_lang("ja").unwrap();
        app.remove_pack("grc").unwrap();
        assert!(!data_dir.join("packs").join("grc").exists());
        assert!(!app.available_languages().iter().any(|(c, _)| c == "grc"));
    }

    #[test]
    fn replacing_a_pack_purges_its_stale_reference_data() {
        let data_dir = unique_dir("pack-replace");
        std::fs::remove_dir_all(&data_dir).ok();
        let src = data_dir.join("incoming").join("grc");
        write_sample_pack(&src);

        let mut app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();
        app.install_pack_from_dir(&src).unwrap();
        app.set_active_lang("grc").unwrap();
        app.ensure_pack_data("grc").unwrap();
        assert_eq!(app.db().dict_entry_count("grc-pack").unwrap(), 2);
        assert_eq!(app.db().frequency_count("grc").unwrap(), 3);

        // A new version of the pack under the same code, with a
        // different dictionary.
        std::fs::write(
            src.join("dictionary.jsonl"),
            concat!(
                r#"{"key":"νόμος","forms":[{"text":"νομοσ","role":"canonical","common":true}],"entry":{"id":"νόμος","kanji":[{"common":true,"text":"νόμος","tags":[]}],"kana":[],"sense":[{"partOfSpeech":["noun"],"gloss":[{"lang":"eng","text":"law"}],"related":[],"antonym":[],"field":[],"dialect":[],"misc":[],"info":[]}]}}"#,
                "\n",
            ),
        )
        .unwrap();
        app.install_pack_from_dir(&src).unwrap();
        assert_eq!(
            app.db().dict_entry_count("grc-pack").unwrap(),
            0,
            "stale reference data purged on replace"
        );

        // Re-activation imports the new pack's data, not the old one's.
        app.set_active_lang("grc").unwrap();
        app.ensure_pack_data("grc").unwrap();
        assert_eq!(app.db().dict_entry_count("grc-pack").unwrap(), 1);
    }

    #[test]
    fn a_gutted_pack_fails_activation_loudly() {
        let mut app = app_with_greek_pack();
        let pack_dir = app.data_dir().join("packs").join("grc");
        std::fs::remove_file(pack_dir.join("dictionary.jsonl")).unwrap();

        // Activation errors with a pointer to reinstall, and the app
        // stays on the previous language instead of half-switching.
        let err = app.set_active_lang("grc").unwrap_err();
        assert!(err.to_string().contains("reinstall"), "{err}");
        assert_eq!(app.active_lang(), "ja");
    }

    #[test]
    fn locked_pack_removal_fails_cleanly_without_gutting() {
        let data_dir = unique_dir("pack-locked");
        std::fs::remove_dir_all(&data_dir).ok();
        let pack_dir = data_dir.join("packs").join("grc");
        write_sample_pack(&pack_dir);
        let mut app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();

        // Windows: hold a no-share-delete handle on a file inside the
        // pack (what Explorer or an indexer does). The removal must
        // fail as a whole — nothing deleted, language still installed —
        // rather than gut the directory halfway.
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_SHARE_READ: u32 = 1;
            let _lock = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .open(pack_dir.join("dictionary.jsonl"))
                .unwrap();
            let err = app.remove_pack("grc").unwrap_err();
            assert!(err.to_string().contains("could not remove"), "{err}");
            assert!(pack_dir.join("manifest.toml").exists(), "nothing gutted");
            assert!(app.available_languages().iter().any(|(c, _)| c == "grc"));
        }

        // Unlocked, removal succeeds completely.
        app.remove_pack("grc").unwrap();
        assert!(!pack_dir.exists());
        assert!(!app.available_languages().iter().any(|(c, _)| c == "grc"));
    }

    #[test]
    fn startup_sweeps_pack_leftovers() {
        let data_dir = unique_dir("pack-sweep");
        std::fs::remove_dir_all(&data_dir).ok();
        write_sample_pack(&data_dir.join("packs").join("grc"));
        std::fs::create_dir_all(data_dir.join(".pack-staging-99999")).unwrap();
        std::fs::create_dir_all(data_dir.join(".pack-build-99999")).unwrap();
        std::fs::write(data_dir.join(".pack-download-99999.zip"), b"junk").unwrap();
        let trash = data_dir.join("packs").join(".trash-99999-0");
        std::fs::create_dir_all(&trash).unwrap();
        std::fs::write(trash.join("x.txt"), b"junk").unwrap();

        let app = App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();
        assert!(!data_dir.join(".pack-staging-99999").exists());
        assert!(!data_dir.join(".pack-build-99999").exists());
        assert!(!data_dir.join(".pack-download-99999.zip").exists());
        assert!(!trash.exists());
        // The real pack survived the sweep.
        assert!(app.available_languages().iter().any(|(c, _)| c == "grc"));
    }

    #[test]
    fn pack_installs_from_zip_with_wrapped_root() {
        let data_dir = unique_dir("pack-zip");
        std::fs::remove_dir_all(&data_dir).ok();
        let src = data_dir.join("incoming").join("grc");
        write_sample_pack(&src);

        // Zip the pack under a single top-level "grc/" folder, the shape
        // hosted pack archives will have.
        let zip_path = data_dir.join("incoming").join("grc.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for name in [
            "manifest.toml",
            "dictionary.jsonl",
            "frequency.tsv",
            "morph_forms.tsv",
            "tags.tsv",
            "graded.tsv",
        ] {
            use std::io::Write as _;
            writer.start_file(format!("grc/{name}"), opts).unwrap();
            writer
                .write_all(&std::fs::read(src.join(name)).unwrap())
                .unwrap();
        }
        writer.finish().unwrap();

        let mut app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();
        assert_eq!(app.install_pack_from_zip(&zip_path).unwrap(), "grc");
        app.set_active_lang("grc").unwrap();
        app.ensure_pack_data("grc").unwrap();
        assert_eq!(app.active_dict_source(), "grc-pack");

        // A garbage zip is rejected cleanly.
        let bad = data_dir.join("incoming").join("bad.zip");
        std::fs::write(&bad, b"not a zip").unwrap();
        assert!(app.install_pack_from_zip(&bad).is_err());
    }

    #[test]
    fn bundled_pack_texts_import_once() {
        let mut app = app_with_greek_pack();
        let texts = app.data_dir().join("packs").join("grc").join("texts");
        std::fs::write(texts.join("john.siat.jsonl"), sample_siat_jsonl()).unwrap();

        app.set_active_lang("grc").unwrap();
        app.ensure_pack_data("grc").unwrap();
        assert_eq!(app.import_pack_texts().unwrap(), (1, 0));
        // Re-importing dedupes by content hash.
        assert_eq!(app.import_pack_texts().unwrap(), (0, 1));
        let docs = app.db().list_documents().unwrap();
        assert!(docs.iter().any(|d| d.document.title == "ΚΑΤΑ ΙΩΑΝΝΗΝ"));
    }

    #[test]
    fn hostile_lang_codes_cannot_escape_the_packs_dir() {
        // A pack whose manifest declares a traversal code is refused
        // before anything is copied or deleted (the code validation
        // itself is unit-tested in shiori-pack).
        let data_dir = unique_dir("pack-hostile");
        std::fs::remove_dir_all(&data_dir).ok();
        let src = data_dir.join("incoming").join("evil");
        write_sample_pack(&src);
        let manifest = std::fs::read_to_string(src.join("manifest.toml")).unwrap();
        std::fs::write(
            src.join("manifest.toml"),
            manifest.replace("lang = \"grc\"", "lang = \"../escape\""),
        )
        .unwrap();
        let mut app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();
        let err = app.install_pack_from_dir(&src).unwrap_err();
        assert!(err.to_string().contains("language code"));
        assert!(!data_dir.join("escape").exists());
    }

    #[test]
    fn catalog_parse_errors_map_to_app_errors() {
        // Parsing itself is tested in shiori-pack; here only the error
        // mapping into AppError matters.
        assert!(parse_catalog(r#"{"catalog": 9, "packs": []}"#)
            .unwrap_err()
            .to_string()
            .contains("catalog version"));
    }

    #[test]
    fn catalog_cache_serves_when_the_network_is_down() {
        let dir = unique_dir("pack-catalog");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        // Port 1 on localhost: nothing listens, connection fails fast.
        let dead_url = "http://127.0.0.1:1/catalog.json";

        // Cold fetch with nothing cached: the failure surfaces.
        assert!(fetch_pack_catalog(&dir, dead_url, false).is_err());

        // With a cached copy, both plain loads and failed refreshes
        // serve it.
        let json = r#"{"catalog": 1, "packs": [
            {"lang": "grc", "name": "Koine Greek", "url": "https://x/grc.zip"}
        ]}"#;
        std::fs::write(dir.join(PACK_CATALOG_FILENAME), json).unwrap();
        let packs = fetch_pack_catalog(&dir, dead_url, false).unwrap();
        assert_eq!(packs.len(), 1);
        let packs = fetch_pack_catalog(&dir, dead_url, true).unwrap();
        assert_eq!(packs[0].lang, "grc");
    }

    #[test]
    fn catalog_refresh_with_garbage_content_keeps_the_cache() {
        let dir = unique_dir("pack-catalog-garbage");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();

        // A server that happily returns 200 with an HTML page — what a
        // directory listing or captive portal looks like.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for stream in listener.incoming().take(2).flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 1024];
                let _ = std::io::Read::read(&mut stream, &mut buf);
                let body = "<!DOCTYPE html><html>not json</html>";
                let _ = std::io::Write::write_all(
                    &mut stream,
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\
                         Connection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        let url = format!("http://{addr}/");

        // Cold fetch: the garbage surfaces, pointing at the URL.
        let err = fetch_pack_catalog(&dir, &url, false).unwrap_err();
        assert!(err.to_string().contains("catalog.json"), "{err}");

        // With a cached copy, the same garbage refresh serves the cache.
        let json = r#"{"catalog": 1, "packs": [
            {"lang": "grc", "name": "Koine Greek", "url": "https://x/grc.zip"}
        ]}"#;
        std::fs::write(dir.join(PACK_CATALOG_FILENAME), json).unwrap();
        let packs = fetch_pack_catalog(&dir, &url, true).unwrap();
        assert_eq!(packs.len(), 1);
        server.join().unwrap();
    }

    #[test]
    fn compounds_split_into_known_dictionary_words() {
        let data_dir = unique_dir("pack-compound");
        std::fs::remove_dir_all(&data_dir).ok();
        let pack_dir = data_dir.join("packs").join("de");
        std::fs::create_dir_all(pack_dir.join("texts")).unwrap();
        std::fs::write(
            pack_dir.join("manifest.toml"),
            r#"
schema = 1
lang = "de"
name = "German"
dict_source = "de-pack"
compounds = true
compound_linkers = ["s", "n"]

[prompt]
language_name = "German"
chat_persona = "a speaker"
immerse_instruction = "Write German."
"#,
        )
        .unwrap();
        let entry = |word: &str, gloss: &str| {
            format!(
                r#"{{"key":"{word}","forms":[{{"text":"{word}","role":"canonical","common":true}}],"entry":{{"id":"{word}","kanji":[{{"common":true,"text":"{word}","tags":[]}}],"kana":[],"sense":[{{"partOfSpeech":["noun"],"gloss":[{{"lang":"eng","text":"{gloss}"}}],"related":[],"antonym":[],"field":[],"dialect":[],"misc":[],"info":[]}}]}}}}"#
            )
        };
        std::fs::write(
            pack_dir.join("dictionary.jsonl"),
            format!(
                "{}\n{}\n{}\n",
                entry("kaffee", "coffee"),
                entry("maschine", "machine"),
                entry("arbeit", "work"),
            ),
        )
        .unwrap();

        let mut app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), data_dir.clone()).unwrap();
        app.set_active_lang("de").unwrap();
        app.ensure_pack_data("de").unwrap();

        // Straight concatenation and a linking element both split.
        assert_eq!(
            app.decompose_compound("Kaffeemaschine").unwrap(),
            Some(vec!["kaffee".to_string(), "maschine".to_string()])
        );
        assert_eq!(
            app.decompose_compound("Arbeitsmaschine").unwrap(),
            Some(vec!["arbeit".to_string(), "maschine".to_string()])
        );

        // Unknown material, short words, and known single words do not.
        assert_eq!(app.decompose_compound("Xyzqmaschine").unwrap(), None);
        assert_eq!(app.decompose_compound("kaffee").unwrap(), None);
        // A trailing linker cannot be swallowed to fake a split.
        assert_eq!(app.decompose_compound("maschines").unwrap(), None);
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
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
