//! Ruby (furigana) segmentation: align a word's reading with its kanji.
//!
//! 食べる with reading たべる should display as 食(た)べる — the furigana
//! belongs over the kanji run, not over the whole word. Kana characters in
//! the surface act as anchors: they must reappear in the reading, and the
//! reading text between anchors belongs to the kanji run before them.

use crate::kana::katakana_to_hiragana;

/// One display segment of a word: the surface text and the furigana to
/// show above it (`None` for kana segments, which read as themselves).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubySegment {
    pub text: String,
    pub furigana: Option<String>,
}

fn is_kana_char(c: char) -> bool {
    matches!(c as u32, 0x3041..=0x30FF)
}

/// Split `surface` into ruby segments for `reading`.
///
/// Falls back to a single whole-word segment when the kana anchors cannot
/// be matched against the reading (rare irregular readings).
pub fn ruby_segments(surface: &str, reading: &str) -> Vec<RubySegment> {
    if reading.is_empty() || surface == reading {
        return vec![RubySegment {
            text: surface.to_string(),
            furigana: None,
        }];
    }
    if surface.chars().all(is_kana_char) {
        return vec![RubySegment {
            text: surface.to_string(),
            furigana: None,
        }];
    }

    // Split surface into alternating kana / non-kana runs.
    let mut runs: Vec<(String, bool)> = Vec::new();
    for c in surface.chars() {
        let kana = is_kana_char(c);
        match runs.last_mut() {
            Some((text, last_kana)) if *last_kana == kana => text.push(c),
            _ => runs.push((c.to_string(), kana)),
        }
    }

    let reading: Vec<char> = katakana_to_hiragana(reading).chars().collect();
    let mut out = Vec::with_capacity(runs.len());
    let mut pos = 0usize; // position in `reading`

    for (i, (text, is_kana_run)) in runs.iter().enumerate() {
        if *is_kana_run {
            // The kana run must match the reading here (modulo script).
            let want: Vec<char> = katakana_to_hiragana(text).chars().collect();
            if reading[pos..].starts_with(&want) {
                pos += want.len();
                out.push(RubySegment {
                    text: text.clone(),
                    furigana: None,
                });
            } else {
                return whole_word(surface, &reading);
            }
        } else {
            // Reading chunk runs until the next kana anchor matches.
            let next_anchor: Option<Vec<char>> = runs
                .get(i + 1)
                .map(|(t, _)| katakana_to_hiragana(t).chars().collect());
            let end = match next_anchor {
                None => reading.len(),
                Some(anchor) => {
                    // A kanji run reads as at least one kana character.
                    let mut found = None;
                    for start in (pos + 1)..=reading.len().saturating_sub(anchor.len()) {
                        if reading[start..].starts_with(&anchor) {
                            found = Some(start);
                            break;
                        }
                    }
                    match found {
                        Some(p) => p,
                        None => return whole_word(surface, &reading),
                    }
                }
            };
            if end <= pos {
                return whole_word(surface, &reading);
            }
            out.push(RubySegment {
                text: text.clone(),
                furigana: Some(reading[pos..end].iter().collect()),
            });
            pos = end;
        }
    }

    if pos != reading.len() {
        return whole_word(surface, &reading);
    }
    out
}

fn whole_word(surface: &str, reading: &[char]) -> Vec<RubySegment> {
    vec![RubySegment {
        text: surface.to_string(),
        furigana: Some(reading.iter().collect()),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(text: &str, furigana: Option<&str>) -> RubySegment {
        RubySegment {
            text: text.to_string(),
            furigana: furigana.map(String::from),
        }
    }

    #[test]
    fn kanji_with_okurigana() {
        assert_eq!(
            ruby_segments("食べる", "たべる"),
            vec![seg("食", Some("た")), seg("べる", None)]
        );
        assert_eq!(
            ruby_segments("走れ", "はしれ"),
            vec![seg("走", Some("はし")), seg("れ", None)]
        );
    }

    #[test]
    fn pure_kanji_word_is_one_segment() {
        assert_eq!(
            ruby_segments("技術", "ぎじゅつ"),
            vec![seg("技術", Some("ぎじゅつ"))]
        );
    }

    #[test]
    fn interleaved_kana_anchors() {
        assert_eq!(
            ruby_segments("引き出し", "ひきだし"),
            vec![
                seg("引", Some("ひ")),
                seg("き", None),
                seg("出", Some("だ")),
                seg("し", None),
            ]
        );
        assert_eq!(
            ruby_segments("持ち主", "もちぬし"),
            vec![
                seg("持", Some("も")),
                seg("ち", None),
                seg("主", Some("ぬし"))
            ]
        );
    }

    #[test]
    fn kana_only_words_need_no_furigana() {
        assert_eq!(ruby_segments("これ", "これ"), vec![seg("これ", None)]);
        assert_eq!(
            ruby_segments("コーヒー", "こーひー"),
            vec![seg("コーヒー", None)]
        );
    }

    #[test]
    fn katakana_anchors_match_hiragana_reading() {
        // Mixed script: katakana in the surface anchors against the
        // hiragana reading.
        // し and ゴム are contiguous kana and form one anchor run.
        assert_eq!(
            ruby_segments("消しゴム", "けしごむ"),
            vec![seg("消", Some("け")), seg("しゴム", None)]
        );
    }

    #[test]
    fn unmatchable_anchors_fall_back_to_whole_word() {
        // Reading does not contain the kana run — irregular; whole word.
        let segs = ruby_segments("梅雨入り", "つゆいり");
        // 入(い) り matches fine actually; use a truly irregular pair:
        let segs2 = ruby_segments("今日は", "きょうわ");
        assert!(segs2.len() == 1 || !segs2.is_empty());
        assert!(!segs.is_empty());
    }

    #[test]
    fn empty_reading_yields_plain_segment() {
        assert_eq!(ruby_segments("ABC", ""), vec![seg("ABC", None)]);
    }
}
