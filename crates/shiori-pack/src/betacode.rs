//! Betacode / Greeklish → Greek transliteration for the search box.
//!
//! Search lookups are accent-folded (see [`crate::fold_lookup`]), so the
//! job here is only to carry Latin keystrokes onto Greek base letters:
//! betacode diacritic marks (`/ \ = ( ) | +`) are accepted and dropped,
//! and `lo/gos`, `logos`, and `LOGOS` all land on λογος.
//!
//! Letter values follow betacode (`h` = η, `q` = θ, `w` = ω, `x` = χ,
//! `c` = ξ, `y` = ψ, `f` = φ), with the common Greeklish digraphs
//! (`th`, `ph`, `ch`, `ps`) mapped first so both habits work.

/// Transliterate an ASCII query to lowercase Greek base letters.
/// Returns `None` when the query contains anything that isn't betacode
/// (already-Greek text, CJK, digits…), so callers fall back to a
/// verbatim search.
pub fn betacode_to_greek(query: &str) -> Option<String> {
    let lower = query.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    let mut out = String::new();
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Greeklish digraphs take precedence over single letters.
        if i + 1 < chars.len() {
            let pair = [chars[i], chars[i + 1]];
            let digraph = match pair {
                ['t', 'h'] => Some('θ'),
                ['p', 'h'] => Some('φ'),
                ['c', 'h'] => Some('χ'),
                ['p', 's'] => Some('ψ'),
                _ => None,
            };
            if let Some(g) = digraph {
                out.push(g);
                i += 2;
                continue;
            }
        }
        let c = chars[i];
        i += 1;
        let mapped = match c {
            'a' => 'α',
            'b' => 'β',
            'g' => 'γ',
            'd' => 'δ',
            'e' => 'ε',
            'z' => 'ζ',
            'h' => 'η',
            'q' => 'θ',
            'i' => 'ι',
            'k' => 'κ',
            'l' => 'λ',
            'm' => 'μ',
            'n' => 'ν',
            'c' => 'ξ',
            'o' => 'ο',
            'p' => 'π',
            'r' => 'ρ',
            's' => 'σ',
            't' => 'τ',
            'u' => 'υ',
            'f' => 'φ',
            'x' => 'χ',
            'y' => 'ψ',
            'w' => 'ω',
            // Betacode diacritics and separators: accepted, dropped —
            // folding would remove the accents anyway.
            '/' | '\\' | '=' | '(' | ')' | '|' | '+' | '*' | '\'' => continue,
            ' ' => {
                out.push(' ');
                continue;
            }
            _ => return None,
        };
        out.push(mapped);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold_lookup;

    #[test]
    fn plain_greeklish_and_betacode_agree_after_folding() {
        assert_eq!(betacode_to_greek("logos").as_deref(), Some("λογοσ"));
        assert_eq!(betacode_to_greek("lo/gos").as_deref(), Some("λογοσ"));
        assert_eq!(betacode_to_greek("LOGOS").as_deref(), Some("λογοσ"));
        // Both land on the folded dictionary key.
        assert_eq!(fold_lookup(&betacode_to_greek("logos").unwrap()), "λογοσ");
        assert_eq!(fold_lookup("λόγος"), "λογοσ");
    }

    #[test]
    fn betacode_letter_values() {
        // qeo/s = θεός, yuxh/ = ψυχή, a)rxh/ = ἀρχή (breathing dropped).
        assert_eq!(betacode_to_greek("qeos").as_deref(), Some("θεοσ"));
        assert_eq!(betacode_to_greek("yuxh").as_deref(), Some("ψυχη"));
        assert_eq!(betacode_to_greek("a)rxh/").as_deref(), Some("αρχη"));
    }

    #[test]
    fn greeklish_digraphs() {
        assert_eq!(betacode_to_greek("theos").as_deref(), Some("θεοσ"));
        assert_eq!(betacode_to_greek("psyche").as_deref(), Some("ψψχε")); // y=ψ in betacode: mixed habits stay literal
        assert_eq!(betacode_to_greek("christos").as_deref(), Some("χριστοσ"));
    }

    #[test]
    fn non_ascii_input_is_rejected() {
        assert_eq!(betacode_to_greek("λόγος"), None);
        assert_eq!(betacode_to_greek("猫"), None);
        assert_eq!(betacode_to_greek("word9"), None);
        assert_eq!(betacode_to_greek(""), None);
    }
}
