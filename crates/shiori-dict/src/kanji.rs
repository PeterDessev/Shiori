//! Kanji reference data: KANJIDIC2 (readings, meanings, grades) and
//! KanjiVG (stroke-order paths), downloaded at runtime like JMdict.
//!
//! KANJIDIC2 © EDRDG, CC BY-SA 4.0. KanjiVG © Ulrich Apel, CC BY-SA 3.0
//! (http://kanjivg.tagaini.net).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::DictError;

/// KANJIDIC2 is regenerated daily at this canonical URL (no pinned
/// version exists).
const KANJIDIC2_URL: &str = "https://www.edrdg.org/kanjidic/kanjidic2.xml.gz";
/// KanjiVG combined XML, pinned to an immutable release tag.
const KANJIVG_URL: &str =
    "https://github.com/KanjiVG/kanjivg/releases/download/r20250816/kanjivg-20250816.xml.gz";

pub const KANJIDIC2_FILENAME: &str = "kanjidic2.xml.gz";
pub const KANJIVG_FILENAME: &str = "kanjivg.xml.gz";

/// One kanji as parsed from KANJIDIC2 (optionally joined with strokes).
#[derive(Debug, Clone, Default)]
pub struct KanjiEntry {
    pub literal: String,
    /// Kyōiku grade 1–6, 8 = Jōyō, 9/10 = Jinmeiyō.
    pub grade: Option<u8>,
    pub stroke_count: u8,
    /// Old (pre-2010) JLPT level 1–4.
    pub jlpt: Option<u8>,
    /// Newspaper frequency rank 1–2500.
    pub freq: Option<u16>,
    pub on_readings: Vec<String>,
    pub kun_readings: Vec<String>,
    pub nanori: Vec<String>,
    /// English meanings only.
    pub meanings: Vec<String>,
    /// Variant/archaic forms resolved to characters (ucs variants only).
    pub variants: Vec<String>,
    /// SVG path data per stroke, in stroke order (from KanjiVG); empty
    /// when KanjiVG has no entry (it covers roughly half of KANJIDIC2).
    pub strokes: Vec<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent("japanese-reading-companion/0.1")
        .build()
}

fn download_to(url: &str, target: &Path) -> Result<(), DictError> {
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let response = agent().get(url).call()?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    let tmp = target.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, target)?;
    Ok(())
}

/// Ensure the KANJIDIC2 archive exists in `data_dir`; returns its path.
pub fn ensure_kanjidic2(data_dir: &Path) -> Result<PathBuf, DictError> {
    let target = data_dir.join(KANJIDIC2_FILENAME);
    download_to(KANJIDIC2_URL, &target)?;
    Ok(target)
}

/// Ensure the KanjiVG archive exists in `data_dir`; returns its path.
pub fn ensure_kanjivg(data_dir: &Path) -> Result<PathBuf, DictError> {
    let target = data_dir.join(KANJIVG_FILENAME);
    download_to(KANJIVG_URL, &target)?;
    Ok(target)
}

fn read_gz(path: &Path) -> Result<String, DictError> {
    let file = std::fs::File::open(path)?;
    let mut out = String::new();
    flate2::read::GzDecoder::new(file).read_to_string(&mut out)?;
    Ok(out)
}

/// Parse the full KANJIDIC2 + KanjiVG archives into joined entries.
pub fn load_kanji(
    kanjidic2_gz: &Path,
    kanjivg_gz: &Path,
) -> Result<Vec<KanjiEntry>, DictError> {
    let mut entries = parse_kanjidic2(&read_gz(kanjidic2_gz)?)?;
    let strokes = parse_kanjivg(&read_gz(kanjivg_gz)?)?;
    for entry in &mut entries {
        if let Some(c) = entry.literal.chars().next() {
            if let Some(paths) = strokes.get(&(c as u32)) {
                entry.strokes = paths.clone();
            }
        }
    }
    Ok(entries)
}

/// Streaming parse of the KANJIDIC2 XML (tolerates its large DOCTYPE).
pub fn parse_kanjidic2(xml: &str) -> Result<Vec<KanjiEntry>, DictError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut current: Option<KanjiEntry> = None;
    // Attributes of the element whose text we're about to read.
    let mut tag = String::new();
    let mut attr: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                attr = None;
                match tag.as_str() {
                    "character" => current = Some(KanjiEntry::default()),
                    "reading" => {
                        attr = attr_value(&e, b"r_type");
                    }
                    "meaning" => {
                        attr = attr_value(&e, b"m_lang");
                    }
                    "variant" => {
                        attr = attr_value(&e, b"var_type");
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                let Some(entry) = current.as_mut() else { continue };
                let text = t.decode().unwrap_or_default().into_owned();
                match tag.as_str() {
                    "literal" => entry.literal = text,
                    "grade" => entry.grade = text.parse().ok(),
                    // First stroke_count is the accepted one; later
                    // values are common miscounts.
                    "stroke_count" if entry.stroke_count == 0 => {
                        entry.stroke_count = text.parse().unwrap_or(0);
                    }
                    "jlpt" => entry.jlpt = text.parse().ok(),
                    "freq" => entry.freq = text.parse().ok(),
                    "reading" => match attr.as_deref() {
                        Some("ja_on") => entry.on_readings.push(text),
                        Some("ja_kun") => entry.kun_readings.push(text),
                        _ => {}
                    },
                    // No m_lang attribute means English.
                    "meaning" if attr.is_none() => entry.meanings.push(text),
                    "nanori" => entry.nanori.push(text),
                    "variant" => {
                        if attr.as_deref() == Some("ucs") {
                            if let Some(c) = u32::from_str_radix(&text, 16)
                                .ok()
                                .and_then(char::from_u32)
                            {
                                entry.variants.push(c.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"character" {
                    if let Some(entry) = current.take() {
                        if !entry.literal.is_empty() {
                            entries.push(entry);
                        }
                    }
                }
                tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DictError::Parse(format!("kanjidic2: {e}"))),
            _ => {}
        }
    }
    Ok(entries)
}

/// Streaming parse of the KanjiVG combined XML: codepoint → stroke path
/// data in stroke order (document order of `<path>` elements).
pub fn parse_kanjivg(xml: &str) -> Result<HashMap<u32, Vec<String>>, DictError> {
    let mut reader = Reader::from_str(xml);
    let mut out: HashMap<u32, Vec<String>> = HashMap::new();
    let mut current: Option<(u32, Vec<String>)> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"kanji" => {
                    // id="kvg:kanji_XXXXX", XXXXX = zero-padded hex.
                    let id = attr_value(&e, b"id").unwrap_or_default();
                    let code = id
                        .rsplit('_')
                        .next()
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok());
                    current = code.map(|c| (c, Vec::new()));
                }
                b"path" => {
                    if let (Some((_, strokes)), Some(d)) =
                        (current.as_mut(), attr_value(&e, b"d"))
                    {
                        strokes.push(d);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"kanji" {
                    if let Some((code, strokes)) = current.take() {
                        if !strokes.is_empty() {
                            out.insert(code, strokes);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DictError::Parse(format!("kanjivg: {e}"))),
            _ => {}
        }
    }
    Ok(out)
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.try_get_attribute(name)
        .ok()
        .flatten()
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KANJIDIC_SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kanjidic2>
<header><file_version>4</file_version></header>
<character>
<literal>亜</literal>
<codepoint><cp_value cp_type="ucs">4e9c</cp_value></codepoint>
<radical><rad_value rad_type="classical">7</rad_value></radical>
<misc>
<grade>8</grade>
<stroke_count>7</stroke_count>
<variant var_type="ucs">4e9e</variant>
<variant var_type="jis208">1-48-19</variant>
<freq>1509</freq>
<jlpt>1</jlpt>
</misc>
<reading_meaning>
<rmgroup>
<reading r_type="pinyin">ya4</reading>
<reading r_type="ja_on">ア</reading>
<reading r_type="ja_kun">つ.ぐ</reading>
<meaning>Asia</meaning>
<meaning m_lang="fr">Asie</meaning>
<meaning>rank next</meaning>
</rmgroup>
<nanori>や</nanori>
</reading_meaning>
</character>
<character>
<literal>无</literal>
<codepoint><cp_value cp_type="ucs">65e0</cp_value></codepoint>
<misc><stroke_count>4</stroke_count><stroke_count>5</stroke_count></misc>
</character>
</kanjidic2>"#;

    #[test]
    fn parses_kanjidic2_fields() {
        let entries = parse_kanjidic2(KANJIDIC_SAMPLE).unwrap();
        assert_eq!(entries.len(), 2);
        let a = &entries[0];
        assert_eq!(a.literal, "亜");
        assert_eq!(a.grade, Some(8));
        assert_eq!(a.stroke_count, 7);
        assert_eq!(a.jlpt, Some(1));
        assert_eq!(a.freq, Some(1509));
        assert_eq!(a.on_readings, vec!["ア"]);
        assert_eq!(a.kun_readings, vec!["つ.ぐ"]);
        assert_eq!(a.nanori, vec!["や"]);
        // French filtered out; only English meanings survive.
        assert_eq!(a.meanings, vec!["Asia", "rank next"]);
        // Only the ucs variant resolves to a character.
        assert_eq!(a.variants, vec!["亞"]);

        // Entry without reading_meaning still parses; first stroke
        // count wins.
        let mu = &entries[1];
        assert_eq!(mu.literal, "无");
        assert_eq!(mu.stroke_count, 4);
        assert!(mu.meanings.is_empty());
    }

    #[test]
    fn parses_kanjivg_strokes_in_order() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<kanjivg xmlns:kvg='http://kanjivg.tagaini.net'>
<kanji id="kvg:kanji_04f55">
<g id="kvg:04f55" kvg:element="何">
<g id="kvg:04f55-g1" kvg:element="亻">
<path id="kvg:04f55-s1" kvg:type="㇒" d="M32.5,13.75c0.23,2.1"/>
<path id="kvg:04f55-s2" kvg:type="㇑" d="M26.76,36.5c1.24,0.5"/>
</g>
<g id="kvg:04f55-g2" kvg:element="可">
<path id="kvg:04f55-s3" kvg:type="㇐" d="M40,20l5,5"/>
</g>
</g>
</kanji>
<kanji id="kvg:kanji_00021">
<g id="kvg:00021"><path id="kvg:00021-s1" d="M5,5"/></g>
</kanji>
</kanjivg>"#;
        let map = parse_kanjivg(xml).unwrap();
        let strokes = &map[&0x4f55];
        assert_eq!(strokes.len(), 3);
        assert!(strokes[0].starts_with("M32.5"));
        assert!(strokes[2].starts_with("M40"));
        assert!(map.contains_key(&0x21));
    }

    #[test]
    fn join_attaches_strokes_by_codepoint() {
        // Simulated join without files.
        let mut entries = parse_kanjidic2(KANJIDIC_SAMPLE).unwrap();
        let mut strokes = HashMap::new();
        strokes.insert('亜' as u32, vec!["M1,1".to_string()]);
        for e in &mut entries {
            if let Some(c) = e.literal.chars().next() {
                if let Some(s) = strokes.get(&(c as u32)) {
                    e.strokes = s.clone();
                }
            }
        }
        assert_eq!(entries[0].strokes.len(), 1);
        assert!(entries[1].strokes.is_empty());
    }
}
