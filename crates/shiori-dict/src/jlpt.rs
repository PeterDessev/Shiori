//! JLPT vocabulary lists, for grading the user's comfortable level.
//!
//! Official JLPT lists stopped being published in 2010; these are the
//! community-maintained lists from stephenmk/yomitan-jlpt-vocab
//! (CC BY-SA 4.0, over Jonathan Waller's CC BY data), pinned to a
//! commit so the data is immutable.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::DictError;

const PIN: &str = "b062d4e38c4bdd0950ae1d4ec55f04b176182e03";

pub const JLPT_FILENAME: &str = "jlpt_vocab.csv";

/// One JLPT vocabulary item.
#[derive(Debug, Clone)]
pub struct JlptWord {
    /// 5 (easiest) … 1 (hardest).
    pub level: u8,
    /// Kanji form; empty for kana-only words.
    pub word: String,
    pub kana: String,
}

fn level_url(level: u8) -> String {
    format!(
        "https://raw.githubusercontent.com/stephenmk/yomitan-jlpt-vocab/{PIN}/original_data/n{level}.csv"
    )
}

/// Ensure a merged local copy of all five level lists exists; returns
/// its path. The merged file is `level,kana,kanji` lines, UTF-8.
pub fn ensure_jlpt_lists(data_dir: &Path) -> Result<PathBuf, DictError> {
    let target = data_dir.join(JLPT_FILENAME);
    if target.exists() {
        return Ok(target);
    }
    std::fs::create_dir_all(data_dir)?;
    let agent = ureq::AgentBuilder::new()
        .user_agent("shiori/0.1")
        .build();
    let mut merged = String::from("level,kana,kanji\n");
    for level in 1..=5u8 {
        let response = agent.get(&level_url(level)).call()?;
        let mut csv_text = String::new();
        response.into_reader().read_to_string(&mut csv_text)?;
        for word in parse_level_csv(level, &csv_text)? {
            // Quote-free merge: kana/kanji columns never contain commas
            // or quotes in this dataset; definitions (which do) are not
            // kept.
            merged.push_str(&format!("{},{},{}\n", word.level, word.kana, word.word));
        }
    }
    let tmp = target.with_extension("part");
    std::fs::write(&tmp, merged)?;
    std::fs::rename(&tmp, &target)?;
    Ok(target)
}

/// Parse one upstream level CSV (`jmdict_seq,kana,kanji,definition`).
fn parse_level_csv(level: u8, csv_text: &str) -> Result<Vec<JlptWord>, DictError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_text.as_bytes());
    let mut out = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| DictError::Parse(format!("jlpt csv: {e}")))?;
        let kana = record.get(1).unwrap_or("").trim().to_string();
        let word = record.get(2).unwrap_or("").trim().to_string();
        if kana.is_empty() && word.is_empty() {
            continue;
        }
        out.push(JlptWord { level, word, kana });
    }
    Ok(out)
}

/// Load the merged local file.
pub fn load_jlpt_lists(path: &Path) -> Result<Vec<JlptWord>, DictError> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let mut parts = line.splitn(3, ',');
        let (Some(level), Some(kana), Some(word)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(level) = level.parse() else { continue };
        out.push(JlptWord {
            level,
            word: word.to_string(),
            kana: kana.to_string(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upstream_format() {
        let csv = "jmdict_seq,kana,kanji,waller_definition\n\
                   1591110,きく,聞く,\"to hear, to listen\"\n\
                   1577100,あう,,\"to meet\"\n\
                   ,,,\n";
        let words = parse_level_csv(5, csv).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "聞く");
        assert_eq!(words[0].kana, "きく");
        assert_eq!(words[0].level, 5);
        assert_eq!(words[1].word, "", "kana-only word keeps empty kanji");
    }

    #[test]
    fn merged_file_roundtrip() {
        let dir = std::env::temp_dir().join("jrc-jlpt-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(JLPT_FILENAME);
        std::fs::write(&path, "level,kana,kanji\n5,きく,聞く\n4,あう,\n").unwrap();
        let words = load_jlpt_lists(&path).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].level, 5);
        assert_eq!(words[1].kana, "あう");
        std::fs::remove_dir_all(&dir).ok();
    }
}
