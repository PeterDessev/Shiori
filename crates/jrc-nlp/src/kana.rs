//! Kana script utilities.

/// Convert katakana characters to hiragana, leaving everything else as-is.
///
/// The prolonged-sound mark ー and half-width katakana are not converted;
/// IPADIC readings only use full-width katakana, which this covers.
pub fn katakana_to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| {
            let u = c as u32;
            // ァ (30A1) ..= ヶ (30F6) maps directly onto ぁ (3041) ..= ゖ (3096).
            if (0x30A1..=0x30F6).contains(&u) {
                char::from_u32(u - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Whether the string consists entirely of kana (and the prolonged-sound mark).
pub fn is_kana_only(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            let u = c as u32;
            (0x3041..=0x309F).contains(&u) // hiragana
                || (0x30A0..=0x30FF).contains(&u) // katakana + ー
        })
}

/// Whether the string contains at least one kanji.
pub fn contains_kanji(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u) || u == 0x3005 // 々
    })
}

/// Whether the string contains any Japanese script at all (kanji or kana).
pub fn is_japanese(s: &str) -> bool {
    contains_kanji(s) || s.chars().any(|c| matches!(c as u32, 0x3041..=0x30FF))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_katakana_to_hiragana() {
        assert_eq!(katakana_to_hiragana("タベル"), "たべる");
        assert_eq!(katakana_to_hiragana("ガッコウ"), "がっこう");
        assert_eq!(katakana_to_hiragana("キャベツ"), "きゃべつ");
    }

    #[test]
    fn keeps_prolonged_sound_mark_and_non_kana() {
        assert_eq!(katakana_to_hiragana("コーヒー"), "こーひー");
        assert_eq!(katakana_to_hiragana("abc食べる"), "abc食べる");
        assert_eq!(katakana_to_hiragana("たべる"), "たべる");
    }

    #[test]
    fn kana_only_detection() {
        assert!(is_kana_only("たべる"));
        assert!(is_kana_only("タベル"));
        assert!(is_kana_only("コーヒー"));
        assert!(!is_kana_only("食べる"));
        assert!(!is_kana_only(""));
        assert!(!is_kana_only("abc"));
    }

    #[test]
    fn kanji_detection() {
        assert!(contains_kanji("食べる"));
        assert!(contains_kanji("人々")); // iteration mark counts
        assert!(!contains_kanji("たべる"));
        assert!(is_japanese("たべる"));
        assert!(!is_japanese("hello 123"));
    }
}
