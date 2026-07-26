//! Per-language book-source knowledge.
//!
//! Two things live here: the bundled catalog of free, legal digital
//! libraries (`free_digital_libraries_by_language.json`, surfaced in the
//! book-search "Libraries" tab), and the [`BookLangProfile`] that tells
//! the app, for a given language code, which Wikisource subdomain and
//! which Project Gutenberg / Gutendex language filter to use.
//!
//! Book search is per-language: the active language decides which
//! Wikisource wiki is queried, whether Gutendex is offered, and which
//! suggested libraries and user OPDS distributors apply.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// The bundled catalog, embedded at build time. Source of the
/// "Libraries" tab and of the coverage notes shown per language.
const CATALOG_JSON: &str = include_str!("../assets/free_digital_libraries_by_language.json");

/// One library/collection entry, as presented to the user. Only the
/// human-facing fields are deserialized; the catalog's machine fields
/// (`api`, `bulk_download`, …) are intentionally ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub description: String,
    /// "free", "free_with_registration", "freemium".
    #[serde(default)]
    pub access: String,
    #[serde(default)]
    pub search_details: String,
    #[serde(default)]
    pub notes: String,
    /// "high", "medium", "low".
    #[serde(default)]
    pub confidence: String,
}

/// A per-language catalog section: either a list of libraries or a
/// cross-reference to another section (e.g. Classical Chinese points at
/// the Chinese section).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Section {
    List(Vec<Library>),
    Ref {
        cross_reference: String,
    },
    /// Any other shape, kept so one unexpected section can't fail the
    /// whole catalog parse. Its contents are unused.
    #[allow(dead_code)]
    Other(serde_json::Value),
}

impl Section {
    fn libraries(&self) -> &[Library] {
        match self {
            Section::List(v) => v,
            _ => &[],
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawCatalog {
    #[serde(default)]
    multilingual_general: Vec<Library>,
    #[serde(default)]
    modern_languages: HashMap<String, Section>,
    #[serde(default)]
    classical_and_ancient_languages: HashMap<String, Section>,
}

impl RawCatalog {
    fn section(&self, key: &str) -> Option<&Section> {
        self.modern_languages
            .get(key)
            .or_else(|| self.classical_and_ancient_languages.get(key))
    }
}

fn catalog() -> &'static RawCatalog {
    static CATALOG: OnceLock<RawCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(CATALOG_JSON).unwrap_or_else(|e| {
            eprintln!("shiori: bundled book catalog failed to parse: {e}");
            RawCatalog::default()
        })
    })
}

/// How to search books in one language: which Wikisource wiki, which
/// Gutendex language filter, and which section of the bundled catalog
/// lists its dedicated libraries.
#[derive(Debug, Clone, Default)]
pub struct BookLangProfile {
    /// Wikisource subdomain (`fr` → fr.wikisource.org). `None` when the
    /// language has no useful Wikisource.
    pub wikisource_subdomain: Option<String>,
    /// Project Gutenberg language code for the Gutendex `languages`
    /// filter (ISO 639-1). `None` disables the Gutenberg tab.
    pub gutendex_lang: Option<String>,
    /// Key into the bundled catalog's per-language sections.
    pub catalog_section: Option<&'static str>,
}

/// Strip a region/script subtag: `pt-BR` → `pt`, `zh-Hant` → `zh`.
fn base_code(code: &str) -> &str {
    code.split(['-', '_']).next().unwrap_or(code)
}

/// The book-source profile for a language code.
pub fn book_lang_profile(code: &str) -> BookLangProfile {
    let base = base_code(code);
    BookLangProfile {
        wikisource_subdomain: wikisource_subdomain(code).map(str::to_string),
        gutendex_lang: gutendex_lang(code).map(str::to_string),
        catalog_section: catalog_section(base),
    }
}

/// Wikisource subdomain for a language, or `None`. Subdomains track the
/// language code for every modern Wikisource (verified: en, fr, de, …,
/// la, sa all use `<code>.wikisource.org`); the exceptions are ancient
/// and script languages that either share a wiki or have none.
fn wikisource_subdomain(code: &str) -> Option<&'static str> {
    // Match on the base code so region/script subtags (e.g. `nn-NO`) route
    // the same as the bare code.
    match base_code(code) {
        // Biblical Hebrew texts sit on the Hebrew Wikisource.
        "hbo" => Some("he"),
        // Classical/Literary Chinese is hosted on the Chinese Wikisource.
        "lzh" => Some("zh"),
        // Norwegian Bokmål/Nynorsk share the Norwegian wiki (there is no
        // nn.wikisource.org).
        "nb" | "nn" | "no" => Some("no"),
        // Koine Greek keeps its own resources: the only Greek Wikisource is
        // Modern Greek's (`el`), which is not Koine-specific, so it is left
        // to Modern Greek and Koine relies on Project Gutenberg and its
        // Libraries directory (Perseus, First1KGreek, …) instead. The rest
        // are scripts/languages with no usable Wikisource.
        "grc" | "akk" | "sux" | "cop" | "syc" | "gez" | "pi" | "bo" | "egy" | "" => None,
        // Modern languages: subdomain == code.
        base => WIKISOURCE_SUBDOMAINS.iter().copied().find(|s| *s == base),
    }
}

/// Every modern Wikisource subdomain we recognize by bare language code
/// (a `<code>.wikisource.org` exists and is active). Keeping this as an
/// allow-list means an arbitrary pack code can't send a request to a
/// nonexistent subdomain. Norwegian variants (`nb`/`nn`/`no`) are handled
/// by the explicit arm above, not here.
const WIKISOURCE_SUBDOMAINS: &[&str] = &[
    "en", "fr", "de", "es", "pt", "it", "nl", "sv", "da", "fi", "pl", "cs", "hu", "el", "ru", "uk",
    "tr", "ar", "fa", "he", "hi", "ur", "sa", "zh", "ja", "ko", "vi", "id", "la", "bn", "ca", "ro",
    "sl", "sr", "hr", "sk", "et", "lt", "lv", "is",
];

/// Project Gutenberg / Gutendex language filter (ISO 639-1) for a code,
/// or `None` to hide the Gutenberg tab. Gutenberg indexes with 639-1
/// codes; Latin (`la`) is present. Ancient/classical languages without a
/// Gutenberg presence are omitted so the tab does not show empty results.
fn gutendex_lang(code: &str) -> Option<&'static str> {
    match base_code(code) {
        "hbo" => Some("he"),
        "nb" | "nn" | "no" => Some("no"),
        // No meaningful Gutenberg corpus / no supported filter.
        "lzh" | "akk" | "sux" | "cop" | "syc" | "gez" | "pi" | "bo" | "egy" => None,
        // Koine Greek: Gutenberg tags its Ancient Greek works `grc`
        // (3-letter tags are honored despite the docs' "two-character"
        // wording), keeping Koine's results Koine-specific.
        "grc" => Some("grc"),
        base => GUTENDEX_LANGS.iter().copied().find(|g| *g == base),
    }
}

/// Language codes Project Gutenberg indexes and Gutendex can filter by
/// (the widely-populated subset; `la` = Latin is included). Gutendex
/// accepts other Gutenberg tags too, but the tab is only offered where a
/// real corpus exists so searches don't come back empty. Norwegian
/// (`no`/`nb`/`nn`) is handled by the explicit arm above, not here.
const GUTENDEX_LANGS: &[&str] = &[
    "en", "fr", "de", "es", "pt", "it", "nl", "sv", "da", "fi", "pl", "cs", "hu", "el", "ru", "uk",
    "tr", "ar", "fa", "he", "hi", "zh", "ja", "ko", "vi", "id", "la", "ca", "eo", "ro", "sr", "sl",
    "et", "lt", "lv", "is", "cy", "br", "eu", "gl", "tl",
];

/// Map a language code to the bundled catalog's section key.
fn catalog_section(code: &str) -> Option<&'static str> {
    Some(match code {
        "en" => "english",
        "fr" => "french",
        "de" => "german",
        "es" => "spanish",
        "pt" => "portuguese",
        "it" => "italian",
        "nl" => "dutch",
        "sv" | "da" | "no" | "nb" | "nn" | "fi" | "is" => "nordic_languages",
        "pl" => "polish",
        "cs" => "czech",
        "hu" => "hungarian",
        "el" => "modern_greek",
        "ru" => "russian",
        "uk" => "ukrainian",
        "tr" => "turkish",
        "ar" => "arabic",
        "fa" => "persian",
        "he" => "hebrew_modern",
        "yi" => "yiddish",
        "hi" | "ur" => "hindi_urdu",
        "sa" => "sanskrit_and_indic",
        "zh" => "chinese",
        "ja" => "japanese",
        "ko" => "korean",
        "vi" => "vietnamese",
        "th" => "thai",
        "id" => "indonesian",
        // Classical / ancient.
        "grc" => "koine_greek",
        "la" => "latin",
        "hbo" => "biblical_hebrew",
        "pi" => "pali",
        "bo" => "tibetan_classical",
        "cop" => "coptic",
        "syc" => "syriac",
        "gez" => "geez_ethiopic",
        "akk" | "sux" => "sumerian_akkadian_cuneiform",
        "lzh" => "classical_chinese",
        _ => return None,
    })
}

/// Libraries the catalog lists as dedicated to `code`'s language (empty
/// when the language has no dedicated section).
pub fn suggested_libraries(code: &str) -> &'static [Library] {
    catalog_section(base_code(code))
        .and_then(|key| catalog().section(key))
        .map(Section::libraries)
        .unwrap_or(&[])
}

/// A cross-reference note for `code`'s section, when the catalog points
/// elsewhere instead of listing libraries (e.g. Classical Chinese →
/// Chinese).
pub fn cross_reference(code: &str) -> Option<&'static str> {
    match catalog_section(base_code(code)).and_then(|key| catalog().section(key)) {
        Some(Section::Ref { cross_reference }) => Some(cross_reference.as_str()),
        _ => None,
    }
}

/// General multilingual libraries that serve every language (Internet
/// Archive, Open Library, HathiTrust, …).
pub fn multilingual_libraries() -> &'static [Library] {
    &catalog().multilingual_general
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_has_entries() {
        assert!(
            !multilingual_libraries().is_empty(),
            "bundled catalog should parse and list multilingual libraries"
        );
        assert!(multilingual_libraries()
            .iter()
            .any(|l| l.name.contains("Internet Archive")));
    }

    #[test]
    fn profiles_map_modern_languages() {
        let fr = book_lang_profile("fr");
        assert_eq!(fr.wikisource_subdomain.as_deref(), Some("fr"));
        assert_eq!(fr.gutendex_lang.as_deref(), Some("fr"));
        assert_eq!(fr.catalog_section, Some("french"));
        assert!(!suggested_libraries("fr").is_empty());

        let ja = book_lang_profile("ja");
        assert_eq!(ja.wikisource_subdomain.as_deref(), Some("ja"));
        assert_eq!(ja.gutendex_lang.as_deref(), Some("ja"));
        assert!(suggested_libraries("ja")
            .iter()
            .any(|l| l.name.contains("Wikisource")
                || l.name.contains("Aozora")
                || !l.name.is_empty()));
    }

    #[test]
    fn koine_and_modern_greek_are_separate_languages() {
        // Koine (Ancient) Greek: Koine-specific resources only — its own
        // Gutenberg tag and catalog section, and no Modern Greek Wikisource.
        let grc = book_lang_profile("grc");
        assert_eq!(grc.wikisource_subdomain, None);
        assert_eq!(grc.gutendex_lang.as_deref(), Some("grc"));
        assert_eq!(grc.catalog_section, Some("koine_greek"));

        // Modern Greek: its own wiki, Gutenberg filter, and section — it
        // shares nothing with Koine (like Turkish vs. French).
        let el = book_lang_profile("el");
        assert_eq!(el.wikisource_subdomain.as_deref(), Some("el"));
        assert_eq!(el.gutendex_lang.as_deref(), Some("el"));
        assert_eq!(el.catalog_section, Some("modern_greek"));
    }

    #[test]
    fn latin_has_wikisource_and_gutenberg() {
        let la = book_lang_profile("la");
        assert_eq!(la.wikisource_subdomain.as_deref(), Some("la"));
        assert_eq!(la.gutendex_lang.as_deref(), Some("la"));
    }

    #[test]
    fn region_subtags_are_stripped() {
        let pt = book_lang_profile("pt-BR");
        assert_eq!(pt.wikisource_subdomain.as_deref(), Some("pt"));
        assert_eq!(pt.gutendex_lang.as_deref(), Some("pt"));
        assert_eq!(pt.catalog_section, Some("portuguese"));

        // Norwegian variants (bare and subtagged) route to the shared wiki,
        // never a nonexistent nn.wikisource.org.
        for code in ["no", "nn", "nb", "nn-NO", "nb-NO"] {
            let p = book_lang_profile(code);
            assert_eq!(p.wikisource_subdomain.as_deref(), Some("no"), "{code}");
            assert_eq!(p.gutendex_lang.as_deref(), Some("no"), "{code}");
        }
    }

    #[test]
    fn unknown_code_yields_empty_profile() {
        let p = book_lang_profile("xx");
        assert_eq!(p.wikisource_subdomain, None);
        assert_eq!(p.gutendex_lang, None);
        assert_eq!(p.catalog_section, None);
        assert!(suggested_libraries("xx").is_empty());
    }
}
