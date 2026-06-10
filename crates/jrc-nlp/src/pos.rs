//! Mapping from IPADIC part-of-speech details to the coarse workspace POS.

use jrc_core::PartOfSpeech;

/// Map IPADIC's 品詞 (major) and 品詞細分類1 (sub) fields.
pub fn map_pos(major: &str, sub: &str) -> PartOfSpeech {
    match major {
        "名詞" => match sub {
            "固有名詞" => PartOfSpeech::ProperNoun,
            "代名詞" => PartOfSpeech::Pronoun,
            "数" => PartOfSpeech::Number,
            "接尾" => PartOfSpeech::Suffix,
            "形容動詞語幹" | "ナイ形容詞語幹" => PartOfSpeech::AdjectivalNoun,
            _ => PartOfSpeech::Noun,
        },
        // 非自立 verbs (e.g. いる in 〜ている) function as auxiliaries; they
        // would otherwise flood vocabulary mining with false "unknown" hits.
        "動詞" => match sub {
            "非自立" => PartOfSpeech::AuxiliaryVerb,
            _ => PartOfSpeech::Verb,
        },
        "形容詞" => PartOfSpeech::Adjective,
        "副詞" => PartOfSpeech::Adverb,
        "助詞" => PartOfSpeech::Particle,
        "助動詞" => PartOfSpeech::AuxiliaryVerb,
        "接続詞" => PartOfSpeech::Conjunction,
        "連体詞" => PartOfSpeech::Prenominal,
        "感動詞" | "フィラー" => PartOfSpeech::Interjection,
        "接頭詞" => PartOfSpeech::Prefix,
        "記号" => PartOfSpeech::Symbol,
        _ => PartOfSpeech::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_major_categories() {
        assert_eq!(map_pos("名詞", "一般"), PartOfSpeech::Noun);
        assert_eq!(map_pos("名詞", "固有名詞"), PartOfSpeech::ProperNoun);
        assert_eq!(map_pos("名詞", "代名詞"), PartOfSpeech::Pronoun);
        assert_eq!(map_pos("名詞", "数"), PartOfSpeech::Number);
        assert_eq!(map_pos("名詞", "形容動詞語幹"), PartOfSpeech::AdjectivalNoun);
        assert_eq!(map_pos("動詞", "自立"), PartOfSpeech::Verb);
        assert_eq!(map_pos("動詞", "非自立"), PartOfSpeech::AuxiliaryVerb);
        assert_eq!(map_pos("形容詞", "自立"), PartOfSpeech::Adjective);
        assert_eq!(map_pos("助詞", "係助詞"), PartOfSpeech::Particle);
        assert_eq!(map_pos("助動詞", "*"), PartOfSpeech::AuxiliaryVerb);
        assert_eq!(map_pos("記号", "句点"), PartOfSpeech::Symbol);
        assert_eq!(map_pos("謎の品詞", "*"), PartOfSpeech::Unknown);
    }
}
