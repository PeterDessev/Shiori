//! Wāpuro rōmaji → kana transliteration for the dictionary search box.
//!
//! Lower-case rōmaji becomes hiragana and capitalised rōmaji becomes
//! katakana, so `neko` → ねこ while `Neko` → ネコ (handy for looking up
//! loanwords). The mapping is deliberately forgiving: it transliterates as
//! much as it can syllable by syllable and drops a trailing lone consonant,
//! so a half-typed word in a live search box (`tabe`, `tabem…`) still
//! yields a usable kana prefix.

use crate::kana::{contains_kanji, hiragana_to_katakana};

/// Convert wāpuro rōmaji to kana, or return `None` when the input is not
/// rōmaji at all — it already contains Japanese, or has no Latin letters.
///
/// Katakana is produced when the first Latin letter is upper-case.
pub fn romaji_to_kana(input: &str) -> Option<String> {
    let first = input.chars().find(|c| c.is_ascii_alphabetic())?;
    // Already (partly) Japanese — leave it for the literal search path.
    if contains_kanji(input) || input.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF)) {
        return None;
    }
    let katakana = first.is_ascii_uppercase();
    let hira = transliterate(&input.to_ascii_lowercase());
    if hira.is_empty() {
        return None;
    }
    Some(if katakana {
        hiragana_to_katakana(&hira)
    } else {
        hira
    })
}

/// Walk already-lower-cased ASCII, emitting hiragana syllable by syllable.
fn transliterate(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if !c.is_ascii_alphabetic() {
            // Separators (space, hyphen) and stray punctuation are dropped;
            // the apostrophe in n' is consumed by the ん branch below.
            i += 1;
            continue;
        }

        // Syllabic ん: an `n` that does not start a na/ni/.../nya syllable.
        if c == b'n' {
            let next = b.get(i + 1).copied();
            let starts_syllable = matches!(next, Some(b'a' | b'i' | b'u' | b'e' | b'o' | b'y'));
            if !starts_syllable {
                out.push('ん');
                i += 1;
                if next == Some(b'\'') {
                    i += 1; // n' is an explicit way to write ん
                }
                continue;
            }
        }
        // m before a labial (shimbun, gambaru) is also ん.
        if c == b'm' && matches!(b.get(i + 1), Some(b'b' | b'p')) {
            out.push('ん');
            i += 1;
            continue;
        }

        // Sokuon っ: a doubled consonant (kk, ss, pp…) or the tch cluster.
        if is_consonant(c) {
            if let Some(&next) = b.get(i + 1) {
                if next == c || (c == b't' && next == b'c') {
                    out.push('っ');
                    i += 1;
                    continue;
                }
            }
        }

        // Longest-match syllable: 3, then 2, then 1 characters.
        let mut matched = false;
        for len in (1..=3).rev() {
            if i + len <= b.len() {
                if let Some(kana) = syllable(&s[i..i + len]) {
                    out.push_str(kana);
                    i += len;
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            // Unconvertible letter — e.g. a trailing lone consonant while
            // the word is still being typed. Skip it.
            i += 1;
        }
    }
    out
}

fn is_consonant(c: u8) -> bool {
    c.is_ascii_alphabetic() && !matches!(c, b'a' | b'i' | b'u' | b'e' | b'o' | b'n')
}

/// One romaji syllable to its hiragana, or `None` if unrecognized.
fn syllable(r: &str) -> Option<&'static str> {
    Some(match r {
        "a" => "あ",
        "i" => "い",
        "u" => "う",
        "e" => "え",
        "o" => "お",
        "ka" => "か",
        "ki" => "き",
        "ku" => "く",
        "ke" => "け",
        "ko" => "こ",
        "ga" => "が",
        "gi" => "ぎ",
        "gu" => "ぐ",
        "ge" => "げ",
        "go" => "ご",
        "sa" => "さ",
        "shi" => "し",
        "si" => "し",
        "su" => "す",
        "se" => "せ",
        "so" => "そ",
        "za" => "ざ",
        "ji" => "じ",
        "zi" => "じ",
        "zu" => "ず",
        "ze" => "ぜ",
        "zo" => "ぞ",
        "ta" => "た",
        "chi" => "ち",
        "ti" => "ち",
        "tsu" => "つ",
        "tu" => "つ",
        "te" => "て",
        "to" => "と",
        "da" => "だ",
        "di" => "ぢ",
        "du" => "づ",
        "de" => "で",
        "do" => "ど",
        "na" => "な",
        "ni" => "に",
        "nu" => "ぬ",
        "ne" => "ね",
        "no" => "の",
        "ha" => "は",
        "hi" => "ひ",
        "fu" => "ふ",
        "hu" => "ふ",
        "he" => "へ",
        "ho" => "ほ",
        "ba" => "ば",
        "bi" => "び",
        "bu" => "ぶ",
        "be" => "べ",
        "bo" => "ぼ",
        "pa" => "ぱ",
        "pi" => "ぴ",
        "pu" => "ぷ",
        "pe" => "ぺ",
        "po" => "ぽ",
        "ma" => "ま",
        "mi" => "み",
        "mu" => "む",
        "me" => "め",
        "mo" => "も",
        "ya" => "や",
        "yu" => "ゆ",
        "yo" => "よ",
        "ra" => "ら",
        "ri" => "り",
        "ru" => "る",
        "re" => "れ",
        "ro" => "ろ",
        "wa" => "わ",
        "wo" => "を",
        "wi" => "うぃ",
        "we" => "うぇ",
        "fa" => "ふぁ",
        "fi" => "ふぃ",
        "fe" => "ふぇ",
        "fo" => "ふぉ",
        "va" => "ゔぁ",
        "vi" => "ゔぃ",
        "vu" => "ゔ",
        "ve" => "ゔぇ",
        "vo" => "ゔぉ",
        // Palatalized digraphs.
        "kya" => "きゃ",
        "kyu" => "きゅ",
        "kyo" => "きょ",
        "gya" => "ぎゃ",
        "gyu" => "ぎゅ",
        "gyo" => "ぎょ",
        "sha" => "しゃ",
        "shu" => "しゅ",
        "sho" => "しょ",
        "sya" => "しゃ",
        "syu" => "しゅ",
        "syo" => "しょ",
        "ja" => "じゃ",
        "ju" => "じゅ",
        "jo" => "じょ",
        "jya" => "じゃ",
        "jyu" => "じゅ",
        "jyo" => "じょ",
        "zya" => "じゃ",
        "zyu" => "じゅ",
        "zyo" => "じょ",
        "cha" => "ちゃ",
        "chu" => "ちゅ",
        "cho" => "ちょ",
        "tya" => "ちゃ",
        "tyu" => "ちゅ",
        "tyo" => "ちょ",
        "cya" => "ちゃ",
        "cyu" => "ちゅ",
        "cyo" => "ちょ",
        "nya" => "にゃ",
        "nyu" => "にゅ",
        "nyo" => "にょ",
        "hya" => "ひゃ",
        "hyu" => "ひゅ",
        "hyo" => "ひょ",
        "bya" => "びゃ",
        "byu" => "びゅ",
        "byo" => "びょ",
        "pya" => "ぴゃ",
        "pyu" => "ぴゅ",
        "pyo" => "ぴょ",
        "mya" => "みゃ",
        "myu" => "みゅ",
        "myo" => "みょ",
        "rya" => "りゃ",
        "ryu" => "りゅ",
        "ryo" => "りょ",
        "dya" => "ぢゃ",
        "dyu" => "ぢゅ",
        "dyo" => "ぢょ",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_syllables_become_hiragana() {
        assert_eq!(romaji_to_kana("neko").as_deref(), Some("ねこ"));
        assert_eq!(romaji_to_kana("sakura").as_deref(), Some("さくら"));
        assert_eq!(romaji_to_kana("ringo").as_deref(), Some("りんご"));
    }

    #[test]
    fn capitalized_romaji_becomes_katakana() {
        assert_eq!(romaji_to_kana("Neko").as_deref(), Some("ネコ"));
        assert_eq!(romaji_to_kana("TEREBI").as_deref(), Some("テレビ"));
        assert_eq!(romaji_to_kana("Koohii").as_deref(), Some("コオヒイ"));
    }

    #[test]
    fn handles_sokuon_and_syllabic_n() {
        assert_eq!(romaji_to_kana("gakkou").as_deref(), Some("がっこう"));
        assert_eq!(romaji_to_kana("kissaten").as_deref(), Some("きっさてん"));
        assert_eq!(romaji_to_kana("konnichiwa").as_deref(), Some("こんにちわ"));
        assert_eq!(romaji_to_kana("shinbun").as_deref(), Some("しんぶん"));
        assert_eq!(romaji_to_kana("shimbun").as_deref(), Some("しんぶん"));
        assert_eq!(romaji_to_kana("hon'ya").as_deref(), Some("ほんや"));
        assert_eq!(romaji_to_kana("honya").as_deref(), Some("ほにゃ"));
    }

    #[test]
    fn digraphs_and_conjugations() {
        assert_eq!(romaji_to_kana("tokyo").as_deref(), Some("ときょ"));
        assert_eq!(romaji_to_kana("matcha").as_deref(), Some("まっちゃ"));
        assert_eq!(romaji_to_kana("tabemashita").as_deref(), Some("たべました"));
        assert_eq!(romaji_to_kana("yonde").as_deref(), Some("よんで"));
    }

    #[test]
    fn partial_input_keeps_what_converts() {
        // A half-typed word leaves a usable kana prefix.
        assert_eq!(romaji_to_kana("tabe").as_deref(), Some("たべ"));
        assert_eq!(romaji_to_kana("tabem").as_deref(), Some("たべ"));
    }

    #[test]
    fn rejects_non_romaji() {
        assert_eq!(romaji_to_kana("猫"), None);
        assert_eq!(romaji_to_kana("ねこ"), None);
        assert_eq!(romaji_to_kana("ネコ"), None);
        assert_eq!(romaji_to_kana("食べるneko"), None); // mixed → leave alone
        assert_eq!(romaji_to_kana(""), None);
        assert_eq!(romaji_to_kana("123"), None);
    }
}
