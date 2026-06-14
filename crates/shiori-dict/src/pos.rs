//! Human-readable labels for JMdict part-of-speech tag codes.
//!
//! JMdict marks each sense with terse codes — `v5r`, `vt`, `adj-i`, `n` —
//! that encode the word class, the conjugation paradigm of verbs, and
//! transitivity. This module turns those codes into short English labels
//! for display ("Godan verb (-ru)", "transitive verb", "i-adjective"). The
//! jmdict-simplified distribution ships full descriptions in its `tags`
//! map, but the database stores only the per-word entries, so these labels
//! travel with the code itself.

/// Concise label for a JMdict part-of-speech code, or `None` if the code
/// is not one this module names (callers fall back to the raw code).
pub fn pos_label(code: &str) -> Option<&'static str> {
    Some(match code {
        // Nouns and nominal classes.
        "n" => "noun",
        "n-adv" => "adverbial noun",
        "n-t" => "temporal noun",
        "n-suf" => "noun suffix",
        "n-pref" => "noun prefix",
        "pn" => "pronoun",
        "num" => "numeric",
        "ctr" => "counter",
        // Adjectives.
        "adj-i" => "i-adjective",
        "adj-ix" => "i-adjective (よい/いい)",
        "adj-na" => "na-adjective",
        "adj-no" => "no-adjective",
        "adj-pn" => "pre-noun adjectival",
        "adj-t" => "taru-adjective",
        "adj-f" => "prenominal",
        // Adverbs.
        "adv" => "adverb",
        "adv-to" => "adverb (taking と)",
        // Verbs: class first, then transitivity.
        "v1" => "Ichidan verb",
        "v1-s" => "Ichidan verb (kureru special)",
        "v5u" => "Godan verb (-u)",
        "v5k" => "Godan verb (-ku)",
        "v5g" => "Godan verb (-gu)",
        "v5s" => "Godan verb (-su)",
        "v5t" => "Godan verb (-tsu)",
        "v5n" => "Godan verb (-nu)",
        "v5b" => "Godan verb (-bu)",
        "v5m" => "Godan verb (-mu)",
        "v5r" => "Godan verb (-ru)",
        "v5r-i" => "Godan verb (-ru, irregular)",
        "v5k-s" => "Godan verb (iku/yuku)",
        "v5u-s" => "Godan verb (-u, special)",
        "v5aru" => "Godan verb (-aru special)",
        "vk" => "Kuru verb",
        "vs" => "suru verb",
        "vs-s" => "suru verb (-suru)",
        "vs-i" => "suru verb (included)",
        "vs-c" => "suru verb (precursor)",
        "vz" => "zuru verb",
        "vn" => "irregular nu verb",
        "vr" => "irregular ru verb",
        "vi" => "intransitive verb",
        "vt" => "transitive verb",
        "aux" => "auxiliary",
        "aux-v" => "auxiliary verb",
        "aux-adj" => "auxiliary adjective",
        // Function words and the rest.
        "conj" => "conjunction",
        "cop" | "cop-da" => "copula",
        "exp" => "expression",
        "int" => "interjection",
        "prt" => "particle",
        "pref" => "prefix",
        "suf" => "suffix",
        "unc" => "unclassified",
        // Classical Japanese paradigms, collapsed.
        "adj-nari" | "adj-ku" | "adj-shiku" | "adj-kari" => "classical adjective",
        c if c.starts_with("v2") || c.starts_with("v4") => "classical verb",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_common_codes() {
        assert_eq!(pos_label("n"), Some("noun"));
        assert_eq!(pos_label("v1"), Some("Ichidan verb"));
        assert_eq!(pos_label("v5r"), Some("Godan verb (-ru)"));
        assert_eq!(pos_label("vt"), Some("transitive verb"));
        assert_eq!(pos_label("vi"), Some("intransitive verb"));
        assert_eq!(pos_label("adj-i"), Some("i-adjective"));
        assert_eq!(pos_label("adj-na"), Some("na-adjective"));
        assert_eq!(pos_label("exp"), Some("expression"));
    }

    #[test]
    fn collapses_classical_paradigms() {
        assert_eq!(pos_label("v2a-s"), Some("classical verb"));
        assert_eq!(pos_label("v4r"), Some("classical verb"));
        assert_eq!(pos_label("adj-nari"), Some("classical adjective"));
    }

    #[test]
    fn unknown_codes_are_none() {
        assert_eq!(pos_label("definitely-not-a-code"), None);
    }
}
