//! Build language packs from public web sources, no hosting required.
//!
//! The same model as the Japanese reference bundle: well-known public
//! data (kaikki.org's per-language Wiktextract dumps for the dictionary
//! and grammar, hermitdave's FrequencyWords for frequency ranks) is
//! downloaded from its stable upstream URLs and processed locally by
//! [`shiori_pack::kaikki`]. The only thing maintained here is the list
//! of languages the builder is known to handle — no catalog, no hosted
//! zips, no repository of ours.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::{AppError, Result};

/// A kaikki dump can exceed a gigabyte; refuse anything past this.
const MAX_SOURCE_DOWNLOAD_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Progress granularity for streamed downloads.
const PROGRESS_EVERY_BYTES: u64 = 16 * 1024 * 1024;

/// One language buildable from public sources. `approx_mb` is the dump
/// size measured 2026-07 — a display hint, not a limit; the real number
/// drifts as Wiktionary grows.
#[derive(Debug, Clone, Copy)]
pub struct WebPackSource {
    /// Pack language code ("es").
    pub lang: &'static str,
    /// English display name ("Spanish").
    pub name: &'static str,
    /// kaikki.org path segment (may contain spaces: "Ancient Greek").
    pub kaikki_path: &'static str,
    /// kaikki.org filename segment (no spaces: "AncientGreek").
    pub kaikki_file: &'static str,
    /// hermitdave FrequencyWords code; `None` when no list exists.
    pub frequency_code: Option<&'static str>,
    /// Approximate dump size in MB, for the UI.
    pub approx_mb: u32,
    /// Script ranges for the manifest; empty = Latin default.
    pub script_ranges: &'static [(u32, u32)],
    /// Elidable prefixes for the tokenizer ("l" so l'eau splits into
    /// the article and the noun); empty for most languages.
    pub elisions: &'static [&'static str],
    /// Fused function words and their expansions ("au" = "a le").
    pub contractions: &'static [(&'static str, &'static str)],
}

const GREEK: &[(u32, u32)] = &[(0x0370, 0x03FF), (0x1F00, 0x1FFF)];
const CYRILLIC: &[(u32, u32)] = &[(0x0400, 0x04FF)];
const HANGUL: &[(u32, u32)] = &[(0x1100, 0x11FF), (0xAC00, 0xD7AF)];

/// French elidable words: l'eau is the article + the noun.
const FR_ELISIONS: &[&str] = &[
    "l", "d", "j", "n", "m", "t", "s", "c", "qu", "jusqu", "lorsqu", "puisqu", "quoiqu",
];
/// Italian elidable words (dell'acqua, un'ora, ...).
const IT_ELISIONS: &[&str] = &[
    "l", "un", "d", "c", "s", "m", "t", "v", "n", "gl", "dell", "nell", "all", "dall", "sull",
    "coll", "quest", "quell", "sant", "anch", "senz", "dov",
];

/// Fused preposition+article portmanteaus per language: the reader
/// shows the expansion and the token counts as a function word.
const FR_CONTRACTIONS: &[(&str, &str)] = &[
    ("au", "à le"),
    ("aux", "à les"),
    ("du", "de le"),
    ("des", "de les"),
];
const ES_CONTRACTIONS: &[(&str, &str)] = &[("al", "a el"), ("del", "de el")];
const DE_CONTRACTIONS: &[(&str, &str)] = &[
    ("am", "an dem"),
    ("ans", "an das"),
    ("aufs", "auf das"),
    ("beim", "bei dem"),
    ("im", "in dem"),
    ("ins", "in das"),
    ("vom", "von dem"),
    ("zum", "zu dem"),
    ("zur", "zu der"),
];
const PT_CONTRACTIONS: &[(&str, &str)] = &[
    ("ao", "a o"),
    ("aos", "a os"),
    ("do", "de o"),
    ("da", "de a"),
    ("dos", "de os"),
    ("das", "de as"),
    ("no", "em o"),
    ("na", "em a"),
    ("nos", "em os"),
    ("nas", "em as"),
    ("pelo", "por o"),
    ("pela", "por a"),
    ("pelos", "por os"),
    ("pelas", "por as"),
    ("num", "em um"),
    ("numa", "em uma"),
    ("dum", "de um"),
    ("duma", "de uma"),
];
const IT_CONTRACTIONS: &[(&str, &str)] = &[
    ("al", "a il"),
    ("allo", "a lo"),
    ("alla", "a la"),
    ("ai", "a i"),
    ("agli", "a gli"),
    ("alle", "a le"),
    ("dal", "da il"),
    ("dallo", "da lo"),
    ("dalla", "da la"),
    ("dai", "da i"),
    ("dagli", "da gli"),
    ("dalle", "da le"),
    ("del", "di il"),
    ("dello", "di lo"),
    ("della", "di la"),
    ("dei", "di i"),
    ("degli", "di gli"),
    ("delle", "di le"),
    ("nel", "in il"),
    ("nello", "in lo"),
    ("nella", "in la"),
    ("nei", "in i"),
    ("negli", "in gli"),
    ("nelle", "in le"),
    ("sul", "su il"),
    ("sullo", "su lo"),
    ("sulla", "su la"),
    ("sui", "su i"),
    ("sugli", "su gli"),
    ("sulle", "su le"),
    ("col", "con il"),
    ("coi", "con i"),
];

/// Languages the kaikki builder is known to produce a sound pack for:
/// whitespace-tokenized scripts with rich Wiktionary inflection data.
/// (No-whitespace scripts like Chinese need a segmentation engine — a
/// Shiori release, not a pack.)
#[rustfmt::skip]
pub const WEB_PACK_SOURCES: &[WebPackSource] = &[
    src("cs", "Czech", "Czech", "Czech", Some("cs"), 197, &[], &[], &[]),
    src("da", "Danish", "Danish", "Danish", Some("da"), 105, &[], &[], &[]),
    src("de", "German", "German", "German", Some("de"), 901, &[], &[], DE_CONTRACTIONS),
    src("es", "Spanish", "Spanish", "Spanish", Some("es"), 966, &[], &[], ES_CONTRACTIONS),
    src("fi", "Finnish", "Finnish", "Finnish", Some("fi"), 419, &[], &[], &[]),
    src("fr", "French", "French", "French", Some("fr"), 544, &[], FR_ELISIONS, FR_CONTRACTIONS),
    src("grc", "Ancient Greek", "Ancient Greek", "AncientGreek", None, 373, GREEK, &[], &[]),
    src("hu", "Hungarian", "Hungarian", "Hungarian", Some("hu"), 176, &[], &[], &[]),
    src("id", "Indonesian", "Indonesian", "Indonesian", Some("id"), 56, &[], &[], &[]),
    src("it", "Italian", "Italian", "Italian", Some("it"), 550, &[], IT_ELISIONS, IT_CONTRACTIONS),
    src("ko", "Korean", "Korean", "Korean", Some("ko"), 186, HANGUL, &[], &[]),
    src("la", "Latin", "Latin", "Latin", None, 1156, &[], &[], &[]),
    src("nl", "Dutch", "Dutch", "Dutch", Some("nl"), 232, &[], &[], &[]),
    src("pl", "Polish", "Polish", "Polish", Some("pl"), 383, &[], &[], &[]),
    src("pt", "Portuguese", "Portuguese", "Portuguese", Some("pt"), 331, &[], &[], PT_CONTRACTIONS),
    src("ro", "Romanian", "Romanian", "Romanian", Some("ro"), 175, &[], &[], &[]),
    src("ru", "Russian", "Russian", "Russian", Some("ru"), 741, CYRILLIC, &[], &[]),
    src("sv", "Swedish", "Swedish", "Swedish", Some("sv"), 175, &[], &[], &[]),
    src("tr", "Turkish", "Turkish", "Turkish", Some("tr"), 121, &[], &[], &[]),
];

#[allow(clippy::too_many_arguments)] // a table-row constructor, not an API
const fn src(
    lang: &'static str,
    name: &'static str,
    kaikki_path: &'static str,
    kaikki_file: &'static str,
    frequency_code: Option<&'static str>,
    approx_mb: u32,
    script_ranges: &'static [(u32, u32)],
    elisions: &'static [&'static str],
    contractions: &'static [(&'static str, &'static str)],
) -> WebPackSource {
    WebPackSource {
        lang,
        name,
        kaikki_path,
        kaikki_file,
        frequency_code,
        approx_mb,
        script_ranges,
        elisions,
        contractions,
    }
}

impl WebPackSource {
    /// The per-language Wiktextract dump on kaikki.org.
    pub fn kaikki_url(&self) -> String {
        format!(
            "https://kaikki.org/dictionary/{}/kaikki.org-dictionary-{}.jsonl",
            self.kaikki_path.replace(' ', "%20"),
            self.kaikki_file
        )
    }

    /// The hermitdave FrequencyWords 50k list, when one exists.
    pub fn frequency_url(&self) -> Option<String> {
        self.frequency_code.map(|code| {
            format!(
                "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/{code}/{code}_50k.txt"
            )
        })
    }
}

/// Look a source up by its language code.
pub fn web_pack_source(lang: &str) -> Option<&'static WebPackSource> {
    WEB_PACK_SOURCES.iter().find(|s| s.lang == lang)
}

/// Stream a URL to a file with progress, capped, atomically (a partial
/// download never leaves a truncated file behind). Skips the download
/// when `dest` already exists (offline retries reuse it), and resumes
/// an interrupted `.part` file with an HTTP Range request instead of
/// starting a gigabyte over.
pub fn download_source_file(
    url: &str,
    dest: &Path,
    label: &str,
    on_progress: &mut dyn FnMut(&str),
) -> Result<()> {
    if dest.exists() {
        on_progress(&format!("{label}: using previously downloaded copy"));
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("part");
    let offset = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);

    let agent = ureq::AgentBuilder::new()
        .user_agent("shiori/0.1")
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .build();
    let mut request = agent.get(url);
    if offset > 0 {
        request = request.set("Range", &format!("bytes={offset}-"));
    }
    let response = request
        .call()
        .map_err(|e| AppError::Invalid(format!("{label} download failed: {e}")))?;

    // 206 = the server honored the range; anything else restarts clean.
    let resuming = offset > 0 && response.status() == 206;
    let total = if resuming {
        // "bytes <from>-<to>/<total>"
        response
            .header("Content-Range")
            .and_then(|v| v.rsplit('/').next())
            .and_then(|t| t.parse::<u64>().ok())
    } else {
        response
            .header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
    };
    let (mut file, mut written) = if resuming {
        on_progress(&format!(
            "{label}: resuming at {} MB",
            offset / (1024 * 1024)
        ));
        let file = std::fs::OpenOptions::new().append(true).open(&tmp)?;
        (file, offset)
    } else {
        (std::fs::File::create(&tmp)?, 0)
    };

    let mut reader = response.into_reader();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut last_report = written;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > MAX_SOURCE_DOWNLOAD_BYTES {
            drop(file);
            std::fs::remove_file(&tmp).ok();
            return Err(AppError::Invalid(format!(
                "{label} download exceeded the size limit"
            )));
        }
        file.write_all(&buf[..n])?;
        if written - last_report >= PROGRESS_EVERY_BYTES {
            last_report = written;
            match total {
                Some(total) if total > 0 => on_progress(&format!(
                    "{label}: {} / {} MB",
                    written / (1024 * 1024),
                    total / (1024 * 1024)
                )),
                _ => on_progress(&format!("{label}: {} MB", written / (1024 * 1024))),
            }
        }
    }
    file.flush()?;
    drop(file);
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Where a source's downloads cache inside the data directory.
pub fn web_source_cache_paths(
    data_dir: &Path,
    source: &WebPackSource,
) -> (PathBuf, Option<PathBuf>) {
    let dir = data_dir.join("web-sources");
    (
        dir.join(format!("kaikki-{}.jsonl", source.lang)),
        source
            .frequency_code
            .map(|code| dir.join(format!("frequency-{code}.txt"))),
    )
}

/// Download a source's inputs (dictionary dump + frequency list) into
/// the cache. Lock-free on purpose: the GUI runs this on a worker
/// thread without holding the app lock.
pub fn download_web_pack_inputs(
    data_dir: &Path,
    source: &WebPackSource,
    on_progress: &mut dyn FnMut(&str),
) -> Result<(PathBuf, Option<PathBuf>)> {
    let (kaikki, frequency) = web_source_cache_paths(data_dir, source);
    on_progress(&format!(
        "downloading {} Wiktionary data (~{} MB)…",
        source.name, source.approx_mb
    ));
    download_source_file(
        &source.kaikki_url(),
        &kaikki,
        &format!("{} dictionary", source.name),
        on_progress,
    )?;
    if let (Some(url), Some(path)) = (source.frequency_url(), &frequency) {
        on_progress("downloading frequency list…");
        // Frequency ranks are an enhancement: losing them must never
        // waste the gigabyte-class dictionary download.
        if let Err(e) = download_source_file(&url, path, "frequency list", on_progress) {
            on_progress(&format!("{e}; continuing without frequency ranks"));
            return Ok((kaikki, None));
        }
    }
    Ok((kaikki, frequency))
}

/// Run the builder over downloaded (or local) inputs, producing a pack
/// directory at `staging`. Lock-free; the caller installs the result
/// via `App::install_pack_from_dir`.
pub fn build_web_pack(
    source: &WebPackSource,
    kaikki_jsonl: &Path,
    frequency: Option<&Path>,
    staging: &Path,
    on_progress: &mut dyn FnMut(&str),
) -> Result<shiori_pack::kaikki::Report> {
    let dict_source = format!("{}-pack", source.lang);
    let description = format!(
        "{} dictionary, inflection tables, and{} built locally from \
         Wiktionary (kaikki.org) data.",
        source.name,
        if source.frequency_code.is_some() {
            " frequency list,"
        } else {
            ""
        }
    );
    let spec = shiori_pack::kaikki::LangSpec {
        lang: source.lang,
        name: source.name,
        dict_source: &dict_source,
        license: "Wiktionary data CC BY-SA 4.0 & GFDL (via kaikki.org); \
                  frequency list CC BY-SA 4.0 (hermitdave FrequencyWords)",
        description: &description,
        script_ranges: source.script_ranges,
        elisions: source.elisions,
        contractions: source.contractions,
    };
    if staging.exists() {
        std::fs::remove_dir_all(staging)?;
    }
    shiori_pack::kaikki::build_pack_with_progress(
        kaikki_jsonl,
        frequency,
        &spec,
        staging,
        on_progress,
    )
    .map_err(|e| AppError::Invalid(format!("pack build failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_sound() {
        let mut seen = std::collections::HashSet::new();
        for source in WEB_PACK_SOURCES {
            assert!(
                shiori_pack::is_safe_lang_code(source.lang),
                "{} must be a safe code",
                source.lang
            );
            assert!(seen.insert(source.lang), "duplicate code {}", source.lang);
            assert_ne!(source.lang, "ja", "Japanese is built in");
            assert!(!source.kaikki_file.contains(' '));
            assert!(source.kaikki_url().starts_with("https://kaikki.org/"));
            if let Some(url) = source.frequency_url() {
                assert!(url.contains("FrequencyWords"));
            }
        }
        assert!(web_pack_source("es").is_some());
        assert!(web_pack_source("tlh").is_none());
    }

    #[test]
    fn kaikki_urls_encode_spaces() {
        let grc = web_pack_source("grc").unwrap();
        assert_eq!(
            grc.kaikki_url(),
            "https://kaikki.org/dictionary/Ancient%20Greek/kaikki.org-dictionary-AncientGreek.jsonl"
        );
    }

    #[test]
    fn web_pack_builds_from_local_inputs_and_installs() {
        let dir = std::env::temp_dir().join(format!(
            "shiori-webpack-test-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace("::", "-")
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let kaikki = dir.join("es.jsonl");
        std::fs::write(
            &kaikki,
            concat!(
                r#"{"word":"gato","pos":"noun","lang_code":"es","senses":[{"glosses":["cat"]}],"forms":[{"form":"gatos","tags":["plural"]}]}"#,
                "\n",
                r#"{"word":"hablar","pos":"verb","lang_code":"es","senses":[{"glosses":["to speak"]}],"forms":[{"form":"hablo","tags":["first-person","singular","present"]}]}"#,
                "\n",
            ),
        )
        .unwrap();
        let freq = dir.join("es_50k.txt");
        std::fs::write(&freq, "gato 7\nhablar 6\n").unwrap();

        let source = *web_pack_source("es").unwrap();
        let staging = dir.join("staging");
        let report = build_web_pack(&source, &kaikki, Some(&freq), &staging, &mut |_| {}).unwrap();
        assert_eq!(report.entries, 2);

        // The staged pack installs and activates like any other, with
        // the grammar working end to end: an inflected form resolves to
        // its lemma and its parse decodes to prose.
        let mut app =
            crate::App::with_db(shiori_db::Db::open_in_memory().unwrap(), dir.clone()).unwrap();
        assert_eq!(app.install_pack_from_dir(&staging).unwrap(), "es");
        app.set_active_lang("es").unwrap();
        let doc = app.import_text("prueba", "hablo gatos.").unwrap();
        let sentences = app.db().sentences(doc).unwrap();
        let rows = app.db().sentence_tokens(sentences[0].id).unwrap();
        let hablo = rows.iter().find(|r| r.token.surface == "hablo").unwrap();
        assert_eq!(hablo.token.lemma, "hablar");
        assert_eq!(
            app.describe_morph(hablo.morph.as_deref().unwrap()),
            "first person · singular · present"
        );
        let gatos = rows.iter().find(|r| r.token.surface == "gatos").unwrap();
        assert_eq!(gatos.token.lemma, "gato");

        std::fs::remove_dir_all(&dir).ok();
    }
}
