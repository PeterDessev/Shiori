//! SIAT — Shiori Annotated Text.
//!
//! The interchange format for pre-annotated texts: JSONL, one header
//! line then one line per sentence. Every token arrives carrying its
//! lemma, parse code, and gloss, so importing a SIAT file needs no
//! runtime analyzer — the annotations are the analysis.
//!
//! ```text
//! {"siat":1,"lang":"grc","title":"ΚΑΤΑ ΙΩΑΝΝΗΝ","quality":"gold",...}
//! {"p":0,"ref":"John.1.1","text":"Ἐν ἀρχῇ ἦν ὁ λόγος","tokens":[
//!     {"s":"Ἐν","l":"ἐν","m":"P","g":"in","start":0,"end":4}, …]}
//! ```
//!
//! Contract: a sentence's tokens appear in order and their `start..end`
//! byte ranges slice `text` to exactly the surface `s` — the reader
//! reconstructs running text from the stored offsets, so a file that
//! violates tiling is rejected at parse time, not rendered wrong later.
//!
//! Reserved (parsed and ignored until an engine implements them, so the
//! format won't need a breaking freeze): per-token `sub` (sub-token
//! lexical units — Hebrew prefixes, Latin enclitics), `layers`
//! (toggleable diacritic layers), header `dir` ("rtl") and structured
//! `citation_scheme`. Unknown fields are always ignored.
//!
//! MorphGNT's 7-column files convert losslessly (see
//! [`from_morphgnt`]); CoNLL-U and OSIS converters follow the same
//! pattern in `shiori-packc`.

use serde::{Deserialize, Serialize};

use crate::{PackError, Result};

/// Header line of a SIAT file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiatHeader {
    /// Format version; this parser accepts `1`.
    pub siat: u32,
    pub lang: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    /// "gold" (hand-verified) or "machine" (auto-tagged); surfaced in
    /// the reader so machine parses are never mistaken for verified ones.
    #[serde(default = "default_quality")]
    pub quality: String,
    /// Reserved: citation scheme description (e.g. "book.chapter.verse").
    #[serde(default)]
    pub citation_scheme: String,
    /// Reserved: text direction.
    #[serde(default)]
    pub dir: Option<String>,
}

fn default_quality() -> String {
    "gold".into()
}

/// One sentence line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiatSentence {
    /// Paragraph index (chapter, section…) the sentence belongs to.
    pub p: u32,
    /// Canonical citation ("John.1.1"); empty when the text has none.
    #[serde(default, rename = "ref")]
    pub reference: String,
    pub text: String,
    pub tokens: Vec<SiatToken>,
}

/// One token: surface, lemma, parse code, gloss, byte offsets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiatToken {
    /// Surface exactly as in `text[start..end]`.
    pub s: String,
    /// Lemma (dictionary form).
    pub l: String,
    /// Parse code in the pack's tagset (e.g. "V-PAI-3S"); empty for
    /// punctuation.
    #[serde(default)]
    pub m: String,
    /// Short gloss for the annotation layer; empty when none.
    #[serde(default)]
    pub g: String,
    pub start: usize,
    pub end: usize,
    /// Reserved: sub-token lexical units.
    #[serde(default)]
    pub sub: Vec<serde_json::Value>,
}

/// A parsed, validated SIAT document.
#[derive(Debug, Clone)]
pub struct SiatDoc {
    pub header: SiatHeader,
    pub sentences: Vec<SiatSentence>,
}

/// Parse and validate a SIAT JSONL document.
pub fn parse(jsonl: &str) -> Result<SiatDoc> {
    let mut lines = jsonl.lines().filter(|l| !l.trim().is_empty());
    let header_line = lines
        .next()
        .ok_or_else(|| PackError::Siat("empty file".into()))?;
    let header: SiatHeader = serde_json::from_str(header_line)
        .map_err(|e| PackError::Siat(format!("bad header: {e}")))?;
    if header.siat != 1 {
        return Err(PackError::Siat(format!(
            "unsupported SIAT version {} (this build understands 1)",
            header.siat
        )));
    }

    let mut sentences = Vec::new();
    for (i, line) in lines.enumerate() {
        let sentence: SiatSentence = serde_json::from_str(line)
            .map_err(|e| PackError::Siat(format!("bad sentence on line {}: {e}", i + 2)))?;
        validate_tiling(&sentence, i + 2)?;
        sentences.push(sentence);
    }
    Ok(SiatDoc { header, sentences })
}

/// The tiling contract: ordered, in-bounds token offsets that slice the
/// sentence text to exactly each token's surface.
fn validate_tiling(sentence: &SiatSentence, line_no: usize) -> Result<()> {
    let mut prev_end = 0usize;
    for token in &sentence.tokens {
        if token.start < prev_end || token.end < token.start {
            return Err(PackError::Siat(format!(
                "line {line_no}: token '{}' offsets out of order",
                token.s
            )));
        }
        match sentence.text.get(token.start..token.end) {
            Some(slice) if slice == token.s => {}
            _ => {
                return Err(PackError::Siat(format!(
                    "line {line_no}: token '{}' does not match text[{}..{}]",
                    token.s, token.start, token.end
                )));
            }
        }
        prev_end = token.end;
    }
    Ok(())
}

/// Serialize a document back to JSONL (packc's output path).
pub fn to_jsonl(doc: &SiatDoc) -> String {
    let mut out = serde_json::to_string(&doc.header).expect("header serializes");
    for sentence in &doc.sentences {
        out.push('\n');
        out.push_str(&serde_json::to_string(sentence).expect("sentence serializes"));
    }
    out.push('\n');
    out
}

/// Convert MorphGNT's 7-column format to SIAT sentences.
///
/// Columns: `BBCCVV part-of-speech parse-code text word normalized lemma`
/// (BB = book number). Verses become sentences (`p` = chapter); the
/// sentence text is rebuilt from the punctuation-bearing `text` column
/// joined with spaces. Glosses are not part of MorphGNT; pass a lookup
/// (lemma → short gloss, e.g. from a Dodson index) to fill them.
pub fn from_morphgnt(
    columns: &str,
    book_name: &str,
    title: &str,
    license: &str,
    gloss_of: &dyn Fn(&str) -> Option<String>,
) -> Result<SiatDoc> {
    struct Row {
        chapter: u32,
        verse: u32,
        pos: String,
        parse: String,
        text: String,
        lemma: String,
    }

    let mut rows = Vec::new();
    for (i, line) in jsonl_lines(columns).enumerate() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 7 {
            return Err(PackError::Siat(format!(
                "MorphGNT line {}: expected 7 columns, got {}",
                i + 1,
                fields.len()
            )));
        }
        let bccv = fields[0];
        if bccv.len() != 6 {
            return Err(PackError::Siat(format!(
                "MorphGNT line {}: bad book/chapter/verse '{bccv}'",
                i + 1
            )));
        }
        let chapter: u32 = bccv[2..4]
            .parse()
            .map_err(|_| PackError::Siat(format!("bad chapter in '{bccv}'")))?;
        let verse: u32 = bccv[4..6]
            .parse()
            .map_err(|_| PackError::Siat(format!("bad verse in '{bccv}'")))?;
        rows.push(Row {
            chapter,
            verse,
            pos: fields[1].trim_end_matches('-').to_string(),
            parse: fields[2].to_string(),
            text: fields[3].to_string(),
            lemma: fields[6].to_string(),
        });
    }

    let mut sentences: Vec<SiatSentence> = Vec::new();
    let mut current: Option<(u32, u32)> = None;
    for row in rows {
        let key = (row.chapter, row.verse);
        if current != Some(key) {
            current = Some(key);
            sentences.push(SiatSentence {
                p: row.chapter.saturating_sub(1),
                reference: format!("{book_name}.{}.{}", row.chapter, row.verse),
                text: String::new(),
                tokens: Vec::new(),
            });
        }
        let sentence = sentences.last_mut().expect("just pushed");
        if !sentence.text.is_empty() {
            sentence.text.push(' ');
        }
        let start = sentence.text.len();
        sentence.text.push_str(&row.text);
        let end = sentence.text.len();
        sentence.tokens.push(SiatToken {
            s: row.text,
            l: row.lemma.clone(),
            m: robinson_code(&row.pos, &row.parse),
            g: gloss_of(&row.lemma).unwrap_or_default(),
            start,
            end,
            sub: Vec::new(),
        });
    }

    Ok(SiatDoc {
        header: SiatHeader {
            siat: 1,
            lang: "grc".into(),
            title: title.into(),
            author: String::new(),
            license: license.into(),
            quality: "gold".into(),
            citation_scheme: "book.chapter.verse".into(),
            dir: None,
        },
        sentences,
    })
}

fn jsonl_lines(s: &str) -> impl Iterator<Item = &str> {
    s.lines().map(str::trim).filter(|l| !l.is_empty())
}

/// Join a MorphGNT positional parse column into Robinson-style dashed
/// segments: `V` + `3IAI-S--` → `V-IAI-3S`; `N` + `----DSF-` → `N-DSF`;
/// participles carry both: `V` + `-PAP-NSM-`-style codes → `V-PAP-NSM`.
///
/// Positions: person, tense, voice, mood, case, number, gender, degree.
pub fn robinson_code(pos: &str, parse: &str) -> String {
    let chars: Vec<char> = parse.chars().collect();
    let at = |i: usize| chars.get(i).copied().unwrap_or('-');
    let (person, tense, voice, mood) = (at(0), at(1), at(2), at(3));
    let (case, number, gender, degree) = (at(4), at(5), at(6), at(7));

    let mut segments = vec![pos.to_string()];
    if mood != '-' {
        segments.push(format!("{tense}{voice}{mood}"));
    }
    if person != '-' {
        segments.push(format!("{person}{number}"));
    }
    if case != '-' {
        segments.push(format!("{case}{number}{gender}"));
    }
    match degree {
        'C' => segments.push("COMP".to_string()),
        'S' => segments.push("SUPL".to_string()),
        _ => {}
    }
    segments.retain(|s| !s.is_empty());
    segments.join("-")
}

/// Coarse part of speech from a MorphGNT/Robinson-style parse code's
/// leading segment (the part before the first '-').
pub fn pos_from_morph(morph: &str) -> shiori_core::PartOfSpeech {
    use shiori_core::PartOfSpeech as P;
    let head = morph.split('-').next().unwrap_or("");
    match head {
        "N" => P::Noun,
        "V" => P::Verb,
        "A" => P::Adjective,
        "D" => P::Adverb,
        "C" => P::Conjunction,
        "P" => P::Preposition,
        "RA" | "T" => P::Article,
        "RP" | "RR" | "RD" | "RI" | "R" | "K" | "Q" | "F" | "S" => P::Pronoun,
        "X" | "PRT" => P::Particle,
        "I" | "INJ" => P::Interjection,
        "M" | "ARAM" | "HEB" => P::Numeral,
        "" => P::Symbol,
        _ => P::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the John 1:1 sample with machine-computed byte offsets
    /// (polytonic Greek is 2–3 bytes per character; never hand-count).
    fn sample_with(extra_token_field: Option<(&str, serde_json::Value)>) -> String {
        let words = [
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
            let end = text.len();
            let mut token = serde_json::json!({
                "s": s, "l": l, "m": m, "g": g, "start": start, "end": end
            });
            if let Some((key, value)) = &extra_token_field {
                token[*key] = value.clone();
            }
            tokens.push(token);
        }
        let header = serde_json::json!({
            "siat": 1, "lang": "grc", "title": "John", "license": "CC BY 4.0",
            "quality": "gold", "citation_scheme": "book.chapter.verse"
        });
        let sentence = serde_json::json!({
            "p": 0, "ref": "John.1.1", "text": text, "tokens": tokens
        });
        format!("{header}\n{sentence}\n")
    }

    fn sample() -> String {
        sample_with(None)
    }

    #[test]
    fn parses_and_validates_the_sample() {
        let doc = parse(&sample()).unwrap();
        assert_eq!(doc.header.lang, "grc");
        assert_eq!(doc.header.quality, "gold");
        assert_eq!(doc.sentences.len(), 1);
        let s = &doc.sentences[0];
        assert_eq!(s.reference, "John.1.1");
        assert_eq!(s.tokens.len(), 5);
        assert_eq!(s.tokens[4].l, "λόγος");
        assert_eq!(s.tokens[4].m, "N-NSM");
        // Tiling: every token slices out of the text.
        for t in &s.tokens {
            assert_eq!(&s.text[t.start..t.end], t.s);
        }
    }

    #[test]
    fn tiling_violations_are_rejected() {
        let bad = sample().replace(r#""start":0"#, r#""start":1"#);
        let err = parse(&bad).unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn round_trips_through_jsonl() {
        let doc = parse(&sample()).unwrap();
        let doc2 = parse(&to_jsonl(&doc)).unwrap();
        assert_eq!(doc2.sentences[0].text, doc.sentences[0].text);
        assert_eq!(
            doc2.sentences[0].tokens.len(),
            doc.sentences[0].tokens.len()
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // A future pack declaring diacritic layers still loads today.
        let with_future = sample_with(Some(("layers", serde_json::json!({"macron": "λόγος"}))));
        assert!(parse(&with_future).is_ok());
    }

    #[test]
    fn converts_morphgnt_columns() {
        // John 1:1a in MorphGNT's format (abridged).
        let morphgnt = "\
040101 P- -------- Ἐν Ἐν ἐν ἐν
040101 N- ----DSF- ἀρχῇ ἀρχῇ ἀρχῇ ἀρχή
040101 V- 3IAI-S-- ἦν ἦν ἦν εἰμί
040101 RA ----NSM- ὁ ὁ ὁ ὁ
040101 N- ----NSM- λόγος λόγος λόγος λόγος
040102 N- ----NSM- οὗτος οὗτος οὗτος οὗτος
";
        let gloss = |lemma: &str| match lemma {
            "λόγος" => Some("word".to_string()),
            _ => None,
        };
        let doc = from_morphgnt(morphgnt, "John", "ΚΑΤΑ ΙΩΑΝΝΗΝ", "CC BY-SA", &gloss).unwrap();
        assert_eq!(doc.sentences.len(), 2, "two verses, two sentences");
        let v1 = &doc.sentences[0];
        assert_eq!(v1.reference, "John.1.1");
        assert_eq!(v1.text, "Ἐν ἀρχῇ ἦν ὁ λόγος");
        assert_eq!(v1.tokens.len(), 5);
        assert_eq!(v1.tokens[1].m, "N-DSF");
        assert_eq!(v1.tokens[2].m, "V-IAI-3S");
        assert_eq!(v1.tokens[3].m, "RA-NSM");
        assert_eq!(v1.tokens[4].g, "word");
        // The converted output passes its own validator.
        parse(&to_jsonl(&doc)).unwrap();
    }

    #[test]
    fn robinson_codes_decompose_participles_and_degrees() {
        // Present active participle, nominative singular masculine:
        // both the tense/voice/mood and case/number/gender slots fire.
        assert_eq!(robinson_code("V", "-PAPNSM-"), "V-PAP-NSM");
        // Comparative adjective.
        assert_eq!(robinson_code("A", "----NSM-"), "A-NSM");
        assert_eq!(robinson_code("A", "----NSMC"), "A-NSM-COMP");
        // Bare preposition.
        assert_eq!(robinson_code("P", "--------"), "P");
    }
}
