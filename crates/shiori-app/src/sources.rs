//! Internet sources: search and import works from Aozora Bunko and
//! Japanese Wikisource.
//!
//! Aozora's catalog CSV is fetched once (from the project's GitHub
//! mirror, per their politeness guidance) and cached in the data
//! directory; searching is local. Wikisource goes through the MediaWiki
//! API with the required descriptive User-Agent.

use std::io::Read;
use std::path::{Path, PathBuf};

use shiori_core::DocumentMeta;

use crate::{App, AppError, Result};

const AOZORA_CATALOG_URL: &str = "https://raw.githubusercontent.com/aozorabunko/aozorabunko/master/index_pages/list_person_all_extended_utf8.zip";
const AOZORA_SITE_PREFIX: &str = "https://www.aozora.gr.jp/";
const AOZORA_MIRROR_PREFIX: &str =
    "https://raw.githubusercontent.com/aozorabunko/aozorabunko/master/";
pub const AOZORA_CATALOG_FILENAME: &str = "aozora_catalog.zip";

const WIKISOURCE_API: &str = "https://ja.wikisource.org/w/api.php";
const WIKISOURCE_REST: &str = "https://ja.wikisource.org/w/rest.php/v1/page";

const USER_AGENT: &str =
    "Shiori/0.1 (https://github.com/; local desktop app) ureq/2";

/// One public-domain work from the Aozora catalog.
#[derive(Debug, Clone)]
pub struct AozoraWork {
    pub id: String,
    pub title: String,
    pub title_reading: String,
    pub author: String,
    pub xhtml_url: String,
    /// Value of the XHTML encoding column ("ShiftJIS", "UTF-8", or "").
    pub xhtml_encoding: String,
    pub orthography: String,
}

/// One Wikisource search hit.
#[derive(Debug, Clone)]
pub struct WikisourceHit {
    pub title: String,
    pub snippet: String,
    pub wordcount: u64,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let response = agent()
        .get(url)
        .call()
        .map_err(|e| AppError::Invalid(format!("download failed: {e}")))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| AppError::Invalid(format!("download failed: {e}")))?;
    Ok(bytes)
}

impl App {
    /// Ensure the Aozora catalog zip is cached, optionally forcing a
    /// fresh download (the reload button). Returns its path.
    pub fn ensure_aozora_catalog(&self, force: bool) -> Result<PathBuf> {
        let target = self.data_dir().join(AOZORA_CATALOG_FILENAME);
        if force && target.exists() {
            std::fs::remove_file(&target)?;
        }
        if !target.exists() {
            let bytes = fetch_bytes(AOZORA_CATALOG_URL)?;
            let tmp = target.with_extension("part");
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, &target)?;
        }
        Ok(target)
    }

    /// Parse the cached catalog into deduplicated, importable works:
    /// public-domain only, hosted on aozora.gr.jp, author rows only.
    pub fn load_aozora_catalog(&self, path: &Path) -> Result<Vec<AozoraWork>> {
        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::Invalid(format!("bad catalog zip: {e}")))?;
        // Single CSV member with an unpredictable exact name.
        let index = (0..archive.len())
            .find(|&i| {
                archive
                    .by_index(i)
                    .map(|f| f.name().ends_with(".csv"))
                    .unwrap_or(false)
            })
            .ok_or_else(|| AppError::Invalid("no CSV in catalog zip".into()))?;
        let mut csv_bytes = Vec::new();
        archive
            .by_index(index)
            .map_err(|e| AppError::Invalid(format!("bad catalog zip: {e}")))?
            .read_to_end(&mut csv_bytes)?;
        // UTF-8 with BOM.
        let start = if csv_bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(&csv_bytes[start..]);
        let mut works = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for record in reader.records() {
            let Ok(record) = record else { continue };
            let field = |i: usize| record.get(i).unwrap_or("").to_string();
            // Rows are work × person pairs: keep the 著者 row per work.
            if field(23) != "著者" || !seen.insert(field(0)) {
                continue;
            }
            // Public domain, hosted on aozora.gr.jp only.
            if field(10) != "なし" || !field(50).starts_with(AOZORA_SITE_PREFIX) {
                continue;
            }
            works.push(AozoraWork {
                id: field(0),
                title: field(1),
                title_reading: field(2),
                author: format!("{} {}", field(15), field(16)).trim().to_string(),
                xhtml_url: field(50),
                xhtml_encoding: field(52),
                orthography: field(9),
            });
        }
        Ok(works)
    }

    /// Download an Aozora work (via the GitHub mirror, falling back to
    /// the site) and import it through the HTML pipeline.
    pub fn import_aozora_work(&self, work: &AozoraWork) -> Result<shiori_core::DocumentId> {
        let mirror = work
            .xhtml_url
            .replace(AOZORA_SITE_PREFIX, AOZORA_MIRROR_PREFIX);
        let bytes = fetch_bytes(&mirror).or_else(|_| fetch_bytes(&work.xhtml_url))?;
        // The HTTP headers carry no charset; the catalog column does.
        let html = match work.xhtml_encoding.as_str() {
            "UTF-8" => String::from_utf8_lossy(&bytes).into_owned(),
            _ => {
                let (text, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes);
                text.into_owned()
            }
        };
        let text = crate::extract::strip_html(&html);
        self.import_text_meta(
            DocumentMeta {
                title: work.title.clone(),
                author: work.author.clone(),
                publisher: "青空文庫".into(),
                published: String::new(),
            },
            &text,
        )
    }

    /// Full-text search on Japanese Wikisource (mainspace).
    pub fn search_wikisource(&self, query: &str) -> Result<Vec<WikisourceHit>> {
        let response = agent()
            .get(WIKISOURCE_API)
            .query("action", "query")
            .query("list", "search")
            .query("srsearch", query)
            .query("srnamespace", "0")
            .query("srlimit", "20")
            .query("srprop", "size|wordcount|snippet")
            .query("maxlag", "5")
            .query("format", "json")
            .query("formatversion", "2")
            .call()
            .map_err(|e| AppError::Invalid(format!("Wikisource search failed: {e}")))?;
        let json: serde_json::Value = response
            .into_json()
            .map_err(|e| AppError::Invalid(format!("Wikisource search failed: {e}")))?;
        let hits = json["query"]["search"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(hits
            .iter()
            .filter_map(|h| {
                Some(WikisourceHit {
                    title: h["title"].as_str()?.to_string(),
                    snippet: strip_tags(h["snippet"].as_str().unwrap_or("")),
                    wordcount: h["wordcount"].as_u64().unwrap_or(0),
                })
            })
            .collect())
    }

    /// Download a Wikisource page as rendered HTML and import it.
    pub fn import_wikisource_page(&self, title: &str) -> Result<shiori_core::DocumentId> {
        let encoded = urlencode(&title.replace(' ', "_"));
        let url = format!("{WIKISOURCE_REST}/{encoded}/html");
        let bytes = fetch_bytes(&url)?;
        let html = String::from_utf8_lossy(&bytes).into_owned();
        let text = crate::extract::strip_html(&html);
        self.import_text_meta(
            DocumentMeta {
                title: title.to_string(),
                author: String::new(),
                publisher: "Wikisource".into(),
                published: String::new(),
            },
            &text,
        )
    }
}

/// Remove HTML tags from a search snippet.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Percent-encode a path segment (RFC 3986 unreserved characters pass).
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_tags_are_stripped() {
        assert_eq!(
            strip_tags("<span class=\"searchmatch\">星</span>の界 底本"),
            "星の界 底本"
        );
        assert_eq!(strip_tags("no tags"), "no tags");
    }

    #[test]
    fn urlencode_keeps_unreserved_and_encodes_utf8() {
        assert_eq!(urlencode("abc-123_~."), "abc-123_~.");
        assert_eq!(urlencode("星の界"), "%E6%98%9F%E3%81%AE%E7%95%8C");
    }

    #[test]
    fn catalog_parsing_filters_and_dedupes() {
        let app =
            App::with_db(shiori_db::Db::open_in_memory().unwrap(), std::env::temp_dir()).unwrap();
        // Build a tiny catalog zip in memory: header + 4 rows exercising
        // dedupe (translator row), copyright filter, and host filter.
        let header: Vec<String> = (0..55).map(|i| format!("c{i}")).collect();
        let mut row = |id: &str, title: &str, role: &str, copyright: &str, url: &str| {
            let mut fields = vec![String::new(); 55];
            fields[0] = id.into();
            fields[1] = title.into();
            fields[2] = "よみ".into();
            fields[9] = "新字新仮名".into();
            fields[10] = copyright.into();
            fields[15] = "夏目".into();
            fields[16] = "漱石".into();
            fields[23] = role.into();
            fields[50] = url.into();
            fields[52] = "ShiftJIS".into();
            fields.join(",")
        };
        let csv = format!(
            "{}\r\n{}\r\n{}\r\n{}\r\n{}\r\n",
            header.join(","),
            row("000001", "こころ", "著者", "なし", "https://www.aozora.gr.jp/cards/x/files/1_2.html"),
            row("000001", "こころ", "翻訳者", "なし", "https://www.aozora.gr.jp/cards/x/files/1_2.html"),
            row("000002", "著作権あり", "著者", "あり", "https://www.aozora.gr.jp/cards/y/files/3_4.html"),
            row("000003", "外部", "著者", "なし", "https://example.com/foo.html"),
        );
        let mut zip_bytes = Vec::new();
        {
            let mut writer =
                zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let mut content = vec![0xEF, 0xBB, 0xBF];
            content.extend_from_slice(csv.as_bytes());
            zip::write::FileOptions::<()>::default();
            writer
                .start_file::<_, ()>("list.csv", Default::default())
                .unwrap();
            std::io::Write::write_all(&mut writer, &content).unwrap();
            writer.finish().unwrap();
        }
        let path = std::env::temp_dir().join("jrc-aozora-test.zip");
        std::fs::write(&path, &zip_bytes).unwrap();

        let works = app.load_aozora_catalog(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(works.len(), 1, "dedupe + copyright + host filters");
        assert_eq!(works[0].title, "こころ");
        assert_eq!(works[0].author, "夏目 漱石");
        assert_eq!(works[0].xhtml_encoding, "ShiftJIS");
    }
}
