//! The hosted pack catalog: the `catalog.json` document the app's
//! browse section consumes and `shiori-packc catalog` produces. Both
//! sides share this schema so a generated catalog can never drift from
//! what the app parses.

use serde::{Deserialize, Serialize};

use crate::{PackError, Result};

/// Catalog schema version this crate reads and writes.
pub const CATALOG_SCHEMA: u32 = 1;

/// One downloadable pack in a hosted catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackCatalogEntry {
    /// BCP-47-ish code the pack installs as ("grc", "es").
    pub lang: String,
    /// English display name ("Koine Greek").
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    /// Download URL of the pack zip.
    pub url: String,
    /// Hex SHA-256 the download must match; empty skips verification.
    #[serde(default)]
    pub sha256: String,
    /// Zip size in bytes, for display only.
    #[serde(default)]
    pub size_bytes: u64,
    /// Pack version string, shown as-is ("2026.07").
    #[serde(default)]
    pub version: String,
}

/// `catalog.json`, top level.
#[derive(Debug, Serialize, Deserialize)]
pub struct PackCatalogFile {
    /// Schema version; readers accept [`CATALOG_SCHEMA`].
    pub catalog: u32,
    pub packs: Vec<PackCatalogEntry>,
}

/// Whether a language code is safe to use as a path component (under
/// the app's `packs/` directory, or as a zip name in a catalog): short,
/// ASCII letters/digits/hyphens only — no separators, no "..", no
/// absolute paths, nothing a hostile manifest or catalog could use to
/// point file operations elsewhere.
pub fn is_safe_lang_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 24
        && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Parse a catalog document into its browsable entries: schema check,
/// then entries missing a code, name, or URL — and any with an unsafe
/// code or claiming the built-in language — are dropped. Entries come
/// back sorted by name.
pub fn parse_pack_catalog(json: &str) -> Result<Vec<PackCatalogEntry>> {
    let file: PackCatalogFile = serde_json::from_str(json)
        .map_err(|e| PackError::Data(format!("bad pack catalog: {e}")))?;
    if file.catalog != CATALOG_SCHEMA {
        return Err(PackError::Data(format!(
            "unsupported pack catalog version {} (this build understands {CATALOG_SCHEMA})",
            file.catalog
        )));
    }
    let mut packs: Vec<PackCatalogEntry> = file
        .packs
        .into_iter()
        .filter(|p| {
            is_safe_lang_code(&p.lang) && !p.name.is_empty() && !p.url.is_empty() && p.lang != "ja"
        })
        .collect();
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_lang_codes() {
        assert!(is_safe_lang_code("grc"));
        assert!(is_safe_lang_code("zh-hant"));
        for bad in [
            "..",
            "../x",
            "a/b",
            "a\\b",
            "C:\\Windows",
            "/etc",
            "",
            "x".repeat(25).as_str(),
        ] {
            assert!(!is_safe_lang_code(bad), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn catalog_parses_filters_and_sorts() {
        let json = r#"{
            "catalog": 1,
            "packs": [
                {"lang": "la", "name": "Latin", "url": "https://x/la.zip"},
                {"lang": "grc", "name": "Koine Greek", "description": "GNT",
                 "license": "CC BY-SA 4.0", "url": "https://x/grc.zip",
                 "sha256": "abc", "size_bytes": 123, "version": "2026.07"},
                {"lang": "", "name": "broken", "url": "https://x/b.zip"},
                {"lang": "xx", "name": "no url", "url": ""},
                {"lang": "../up", "name": "hostile", "url": "https://x/up.zip"},
                {"lang": "ja", "name": "Japanese", "url": "https://x/ja.zip"}
            ]
        }"#;
        let packs = parse_pack_catalog(json).unwrap();
        // Invalid, hostile, and built-in entries dropped; sorted by name.
        assert_eq!(packs.len(), 2);
        assert_eq!(packs[0].name, "Koine Greek");
        assert_eq!(packs[0].sha256, "abc");
        assert_eq!(packs[0].size_bytes, 123);
        assert_eq!(packs[1].name, "Latin");
        assert_eq!(packs[1].version, "");

        // Wrong schema version and garbage are clean errors.
        assert!(parse_pack_catalog(r#"{"catalog": 9, "packs": []}"#).is_err());
        assert!(parse_pack_catalog("not json").is_err());
    }

    #[test]
    fn entries_serialize_round_trip() {
        let file = PackCatalogFile {
            catalog: CATALOG_SCHEMA,
            packs: vec![PackCatalogEntry {
                lang: "grc".into(),
                name: "Koine Greek".into(),
                description: "GNT".into(),
                license: "CC BY-SA 4.0".into(),
                url: "https://x/grc.zip".into(),
                sha256: "abc".into(),
                size_bytes: 123,
                version: "2026.07".into(),
            }],
        };
        let json = serde_json::to_string_pretty(&file).unwrap();
        let parsed = parse_pack_catalog(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Koine Greek");
    }
}
