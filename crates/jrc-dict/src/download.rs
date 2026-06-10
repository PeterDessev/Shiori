//! First-run acquisition of dictionary and frequency data.
//!
//! Files are stored under a caller-supplied data directory and never
//! re-downloaded once present, so the app works offline after first run.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::DictError;

/// Where the GitHub API for jmdict-simplified releases lives.
const JMDICT_RELEASES_API: &str =
    "https://api.github.com/repos/scriptin/jmdict-simplified/releases/latest";

/// Leeds-corpus-derived frequency list (one word per line, by rank).
const FREQUENCY_URL: &str =
    "https://raw.githubusercontent.com/hingston/japanese/master/44492-japanese-words-latin-lines-removed.txt";

/// Filename of the cached dictionary JSON inside the data directory.
pub const JMDICT_FILENAME: &str = "jmdict-eng.json";
/// Filename of the cached frequency list inside the data directory.
pub const FREQUENCY_FILENAME: &str = "frequency.txt";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent("japanese-reading-companion/0.1")
        .build()
}

/// Ensure the JMdict JSON exists in `data_dir`, downloading and unpacking
/// the latest jmdict-simplified English release if needed.
///
/// Returns the path to the JSON file.
pub fn ensure_jmdict(data_dir: &Path) -> Result<PathBuf, DictError> {
    let target = data_dir.join(JMDICT_FILENAME);
    if target.exists() {
        return Ok(target);
    }
    fs::create_dir_all(data_dir)?;

    let url = latest_jmdict_asset_url()?;
    let response = agent().get(&url).call()?;
    let mut compressed = Vec::new();
    response.into_reader().read_to_end(&mut compressed)?;

    let json = extract_json_from_tgz(&compressed)?;
    // Write atomically-ish: temp file then rename, so a cancelled download
    // never leaves a truncated dictionary behind.
    let tmp = data_dir.join(format!("{JMDICT_FILENAME}.part"));
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &target)?;
    Ok(target)
}

/// Ensure the frequency list exists in `data_dir`, downloading if needed.
pub fn ensure_frequency_list(data_dir: &Path) -> Result<PathBuf, DictError> {
    let target = data_dir.join(FREQUENCY_FILENAME);
    if target.exists() {
        return Ok(target);
    }
    fs::create_dir_all(data_dir)?;

    let body = agent().get(FREQUENCY_URL).call()?.into_string()?;
    if body.lines().filter(|l| !l.trim().is_empty()).count() < 1000 {
        return Err(DictError::Parse(
            "frequency list download looks truncated".into(),
        ));
    }
    let tmp = data_dir.join(format!("{FREQUENCY_FILENAME}.part"));
    fs::write(&tmp, &body)?;
    fs::rename(&tmp, &target)?;
    Ok(target)
}

/// Resolve the download URL of the `jmdict-eng-*.json.tgz` asset of the
/// latest release (the full dictionary, not the `common`-only variant).
fn latest_jmdict_asset_url() -> Result<String, DictError> {
    let release: serde_json::Value = agent()
        .get(JMDICT_RELEASES_API)
        .set("Accept", "application/vnd.github+json")
        .call()?
        .into_json()
        .map_err(|e| DictError::Parse(e.to_string()))?;

    pick_jmdict_asset(&release).ok_or(DictError::NoAsset)
}

/// Pick the right asset from a GitHub release JSON document.
fn pick_jmdict_asset(release: &serde_json::Value) -> Option<String> {
    release.get("assets")?.as_array()?.iter().find_map(|asset| {
        let name = asset.get("name")?.as_str()?;
        if name.starts_with("jmdict-eng-") && name.ends_with(".json.tgz") && !name.contains("common")
        {
            asset
                .get("browser_download_url")?
                .as_str()
                .map(String::from)
        } else {
            None
        }
    })
}

/// Extract the single `.json` member of a gzipped tarball.
fn extract_json_from_tgz(compressed: &[u8]) -> Result<Vec<u8>, DictError> {
    let gz = flate2::read::GzDecoder::new(compressed);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let is_json = entry
            .path()
            .ok()
            .and_then(|p| p.extension().map(|e| e == "json"))
            .unwrap_or(false);
        if is_json {
            let mut out = Vec::new();
            entry.read_to_end(&mut out)?;
            return Ok(out);
        }
    }
    Err(DictError::Parse("no .json member found in archive".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn picks_full_english_tgz_asset() {
        let release = serde_json::json!({
            "assets": [
                {"name": "jmdict-eng-common-3.6.2.json.tgz",
                 "browser_download_url": "https://example.com/common.tgz"},
                {"name": "jmdict-eng-3.6.2+20260608.json.zip",
                 "browser_download_url": "https://example.com/full.zip"},
                {"name": "jmdict-eng-3.6.2+20260608.json.tgz",
                 "browser_download_url": "https://example.com/full.tgz"},
                {"name": "jmdict-all-3.6.2.json.tgz",
                 "browser_download_url": "https://example.com/all.tgz"}
            ]
        });
        assert_eq!(
            pick_jmdict_asset(&release).as_deref(),
            Some("https://example.com/full.tgz")
        );
    }

    #[test]
    fn no_asset_when_release_is_malformed() {
        assert_eq!(pick_jmdict_asset(&serde_json::json!({})), None);
        assert_eq!(pick_jmdict_asset(&serde_json::json!({"assets": []})), None);
    }

    #[test]
    fn extracts_json_from_tgz() {
        // Build a tiny tar.gz containing one json file.
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let content = br#"{"ok": true}"#;
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, "jmdict-eng-test.json", content.as_slice())
                .unwrap();
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        let compressed = gz.finish().unwrap();

        let extracted = extract_json_from_tgz(&compressed).unwrap();
        assert_eq!(extracted, br#"{"ok": true}"#);
    }

    #[test]
    fn tgz_without_json_is_an_error() {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let content = b"hello";
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, "readme.txt", content.as_slice())
                .unwrap();
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        let compressed = gz.finish().unwrap();

        assert!(extract_json_from_tgz(&compressed).is_err());
    }
}
