//! Internet sources: search and import books, per language.
//!
//! Aozora Bunko (Japanese) caches its catalog CSV once and searches it
//! locally. Wikisource goes through the MediaWiki API on the active
//! language's subdomain. Project Gutenberg is searched through the
//! Gutendex JSON API, filtered to the active language. OPDS distributors
//! the user adds are searched through their Atom (or OPDS 2.0) feeds. All
//! run with the descriptive User-Agent the sites require, and every
//! import lands under the active language through the normal pipeline.

use std::io::Read;
use std::path::{Path, PathBuf};

use shiori_core::DocumentMeta;
use url::Url;

use crate::{books, opds};
use crate::{App, AppError, Result};

const AOZORA_CATALOG_URL: &str = "https://raw.githubusercontent.com/aozorabunko/aozorabunko/master/index_pages/list_person_all_extended_utf8.zip";
const AOZORA_SITE_PREFIX: &str = "https://www.aozora.gr.jp/";
const AOZORA_MIRROR_PREFIX: &str =
    "https://raw.githubusercontent.com/aozorabunko/aozorabunko/master/";
pub const AOZORA_CATALOG_FILENAME: &str = "aozora_catalog.zip";

// Trailing slash avoids a 301 (Gutendex's own paging links use it too).
const GUTENDEX_API: &str = "https://gutendex.com/books/";

const USER_AGENT: &str =
    "Shiori/0.2.0 (https://github.com/PeterDessev/Shiori; peter.dessev@gmail.com) ureq/2";

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

/// One Project Gutenberg book from a Gutendex search.
#[derive(Debug, Clone)]
pub struct GutendexHit {
    pub id: u64,
    pub title: String,
    pub author: String,
    pub languages: Vec<String>,
    pub download_count: u64,
    /// Best plain-text download URL, when Gutenberg offers one.
    pub text_url: Option<String>,
    pub html_url: Option<String>,
    pub epub_url: Option<String>,
}

impl GutendexHit {
    /// Whether any importable format is available.
    pub fn is_importable(&self) -> bool {
        self.text_url.is_some() || self.html_url.is_some() || self.epub_url.is_some()
    }
}

/// One book from an OPDS catalog feed.
#[derive(Debug, Clone)]
pub struct OpdsHit {
    pub title: String,
    pub author: String,
    pub summary: String,
    /// Acquisition links: (MIME type, absolute URL), best format first.
    pub links: Vec<(String, String)>,
}

impl OpdsHit {
    /// The acquisition link this app can import, preferring EPUB, then
    /// plain text / HTML, then PDF.
    pub fn best_link(&self) -> Option<&(String, String)> {
        let rank = |mime: &str| -> u8 {
            let m = mime.to_ascii_lowercase();
            if m.contains("epub") {
                0
            } else if m.contains("text/plain") {
                1
            } else if m.contains("html") {
                2
            } else if m.contains("pdf") {
                3
            } else {
                9
            }
        };
        self.links
            .iter()
            .filter(|(mime, _)| rank(mime) < 9)
            .min_by_key(|(mime, _)| rank(mime))
    }
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
}

pub(crate) fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
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

/// Fetch a URL for OPDS: negotiate OPDS JSON or Atom, and return the
/// response `Content-Type` alongside the body so the caller can pick the
/// right parser. Reads raw bytes (no 10 MiB cap) and decodes UTF-8,
/// falling back to Windows-1252 for the occasional legacy feed rather
/// than erroring on non-UTF-8 input.
pub(crate) fn fetch_opds(url: &str) -> Result<(String, String)> {
    let response = agent()
        .get(url)
        .set(
            "Accept",
            "application/opds+json, application/atom+xml;q=0.9, */*;q=0.5",
        )
        .call()
        .map_err(|e| AppError::Invalid(format!("OPDS request failed: {e}")))?;
    let content_type = response.content_type().to_string();
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| AppError::Invalid(format!("OPDS request failed: {e}")))?;
    let body = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => encoding_rs::WINDOWS_1252
            .decode(e.as_bytes())
            .0
            .into_owned(),
    };
    Ok((content_type, body))
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
        let start = if csv_bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            3
        } else {
            0
        };

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

    /// The Wikisource subdomain for the active language, or an error when
    /// the language has no Wikisource.
    fn wikisource_subdomain(&self) -> Result<String> {
        books::book_lang_profile(self.active_lang())
            .wikisource_subdomain
            .ok_or_else(|| {
                AppError::Invalid(format!(
                    "no Wikisource is available for language '{}'",
                    self.active_lang()
                ))
            })
    }

    /// Whether the active language can be searched on Wikisource.
    pub fn has_wikisource(&self) -> bool {
        books::book_lang_profile(self.active_lang())
            .wikisource_subdomain
            .is_some()
    }

    /// Full-text search on the active language's Wikisource (mainspace).
    pub fn search_wikisource(&self, query: &str) -> Result<Vec<WikisourceHit>> {
        let sub = self.wikisource_subdomain()?;
        let api = format!("https://{sub}.wikisource.org/w/api.php");
        let response = agent()
            .get(&api)
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

    /// Download a Wikisource page as rendered HTML and import it under the
    /// active language.
    pub fn import_wikisource_page(&self, title: &str) -> Result<shiori_core::DocumentId> {
        let sub = self.wikisource_subdomain()?;
        // Subpages are common on Wikisource (Book/Chapter_1); encode '/'.
        let encoded = urlencode(&title.replace(' ', "_"));
        let url = format!("https://{sub}.wikisource.org/w/rest.php/v1/page/{encoded}/html");
        let bytes = fetch_bytes(&url)?;
        let html = String::from_utf8_lossy(&bytes).into_owned();
        let text = crate::extract::strip_html(&html);
        self.import_text_meta(
            DocumentMeta {
                title: title.to_string(),
                author: String::new(),
                publisher: format!("{sub}.wikisource.org"),
                published: String::new(),
            },
            &text,
        )
    }

    /// Whether Project Gutenberg (via Gutendex) covers the active language.
    pub fn has_gutenberg(&self) -> bool {
        books::book_lang_profile(self.active_lang())
            .gutendex_lang
            .is_some()
    }

    /// Search Project Gutenberg through the Gutendex API, filtered to the
    /// active language when Gutenberg indexes it.
    pub fn search_gutendex(&self, query: &str) -> Result<Vec<GutendexHit>> {
        let mut req = agent().get(GUTENDEX_API).query("search", query);
        if let Some(lang) = books::book_lang_profile(self.active_lang()).gutendex_lang {
            req = req.query("languages", &lang);
        }
        let json: serde_json::Value = req
            .call()
            .map_err(|e| AppError::Invalid(format!("Gutenberg search failed: {e}")))?
            .into_json()
            .map_err(|e| AppError::Invalid(format!("Gutenberg search failed: {e}")))?;
        let results = json["results"].as_array().cloned().unwrap_or_default();
        Ok(results.iter().filter_map(parse_gutendex_book).collect())
    }

    /// Download a Gutenberg book and import it under the active language,
    /// preferring plain text (boilerplate stripped), then HTML, then EPUB.
    pub fn import_gutendex_book(&self, hit: &GutendexHit) -> Result<shiori_core::DocumentId> {
        let author = hit.author.clone();
        let meta = |title: String| DocumentMeta {
            title,
            author: author.clone(),
            publisher: "Project Gutenberg".into(),
            published: String::new(),
        };
        if let Some(url) = &hit.text_url {
            let bytes = fetch_bytes(url)?;
            // The utf-8 endpoint is UTF-8; the us-ascii fallback is often
            // Latin-1. Decode strictly, then fall back to Windows-1252.
            let raw = match std::str::from_utf8(&bytes) {
                Ok(s) => s.to_string(),
                Err(_) => encoding_rs::WINDOWS_1252.decode(&bytes).0.into_owned(),
            };
            let raw = raw.replace("\r\n", "\n");
            let text = strip_gutenberg_boilerplate(&raw);
            return self.import_text_meta(meta(hit.title.clone()), &text);
        }
        if let Some(url) = &hit.html_url {
            let bytes = fetch_bytes(url)?;
            let html = String::from_utf8_lossy(&bytes).into_owned();
            let text = crate::extract::strip_html(&html);
            return self.import_text_meta(meta(hit.title.clone()), &text);
        }
        if let Some(url) = &hit.epub_url {
            return self.import_download_as_file(url, &format!("gutenberg-{}.epub", hit.id));
        }
        Err(AppError::Invalid(
            "this Gutenberg book has no importable format".into(),
        ))
    }

    /// Download `url` to a temporary file named `filename` and import it
    /// through the file pipeline (EPUB/PDF extraction, copied into the
    /// library). The temporary file is removed afterward.
    fn import_download_as_file(
        &self,
        url: &str,
        filename: &str,
    ) -> Result<shiori_core::DocumentId> {
        let bytes = fetch_bytes(url)?;
        let tmp = self.data_dir().join(format!("download-{filename}"));
        std::fs::write(&tmp, &bytes)?;
        let result = self.import_file(&tmp);
        std::fs::remove_file(&tmp).ok();
        result
    }

    /// Search one OPDS catalog feed. When the feed advertises search (an
    /// OpenSearch description for OPDS 1.x, or a templated `search` link
    /// for OPDS 2.0), the query runs on the server; otherwise the feed's
    /// own entries are fetched and filtered locally. Feeds whose results
    /// are navigation-only (e.g. Project Gutenberg) are followed one hop
    /// to reach the acquisition links, up to a bounded number.
    pub fn search_opds(&self, catalog_url: &str, query: &str) -> Result<Vec<OpdsHit>> {
        let base = Url::parse(catalog_url)
            .map_err(|e| AppError::Invalid(format!("invalid OPDS URL: {e}")))?;
        let query = query.trim();
        let (ctype, body) = fetch_opds(catalog_url)?;
        let feed = self.parse_opds_response(&ctype, &body, &base);

        // If the feed advertises search and we have a query, run it on
        // the server (following an OpenSearch description if needed).
        if !query.is_empty() {
            if let Some(search) = &feed.search {
                let template = if search.mime.contains("opensearchdescription") {
                    let osd_base = Url::parse(&search.href).unwrap_or_else(|_| base.clone());
                    let (_c, osd) = fetch_opds(&search.href)?;
                    opds::parse_osd(&osd, &osd_base).ok_or_else(|| {
                        AppError::Invalid("OPDS search description had no usable template".into())
                    })?
                } else {
                    search.href.clone()
                };
                let target = opds::expand_template(&template, query);
                let target_base = Url::parse(&target).unwrap_or_else(|_| base.clone());
                let (ctype2, body2) = fetch_opds(&target)?;
                let results = self.parse_opds_response(&ctype2, &body2, &target_base);
                return self.opds_hits(results, &target_base, None);
            }
        }

        // No server search: list (and, with a query, locally filter) the
        // feed's own entries.
        let filter = (!query.is_empty()).then_some(query);
        self.opds_hits(feed, &base, filter)
    }

    /// Detect OPDS 2.0 (JSON) vs 1.x (Atom) from the content type/body and
    /// parse accordingly.
    fn parse_opds_response(&self, ctype: &str, body: &str, base: &Url) -> opds::ParsedFeed {
        let looks_json = ctype.contains("json") || body.trim_start().starts_with('{');
        if looks_json {
            // Fall through to Atom on malformed JSON.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
                return opds::parse_opds2(&v, base);
            }
        }
        opds::parse_atom(body, base)
    }

    /// Turn a parsed feed into book hits: keep entries with acquisition
    /// links, follow navigation-only entries one hop when nothing has a
    /// direct link, and apply an optional case-insensitive filter.
    fn opds_hits(
        &self,
        feed: opds::ParsedFeed,
        base: &Url,
        filter: Option<&str>,
    ) -> Result<Vec<OpdsHit>> {
        const MAX_HITS: usize = 50;
        // Number of navigation entries to follow when a feed lists only
        // subfeeds (bounded so a big catalog can't fan out into hundreds
        // of requests).
        const MAX_FOLLOW: usize = 10;

        let mut hits: Vec<OpdsHit> = feed.entries.iter().filter_map(|e| e.to_hit()).collect();

        if hits.is_empty() {
            for entry in feed.entries.iter().take(MAX_FOLLOW) {
                let Some(href) = entry.navigation_href() else {
                    continue;
                };
                if let Ok((c, b)) = fetch_opds(href) {
                    let sub_base = Url::parse(href).unwrap_or_else(|_| base.clone());
                    let sub = self.parse_opds_response(&c, &b, &sub_base);
                    hits.extend(sub.entries.iter().filter_map(|e| e.to_hit()));
                }
                if hits.len() >= MAX_HITS {
                    break;
                }
            }
        }

        if let Some(q) = filter {
            let q = q.to_lowercase();
            hits.retain(|h| {
                h.title.to_lowercase().contains(&q) || h.author.to_lowercase().contains(&q)
            });
        }
        hits.truncate(MAX_HITS);
        Ok(hits)
    }

    /// Download one OPDS acquisition link and import it under the active
    /// language. EPUB/PDF go through the file pipeline; HTML and plain
    /// text are imported directly.
    pub fn import_opds(
        &self,
        url: &str,
        mime: &str,
        title: &str,
        author: &str,
    ) -> Result<shiori_core::DocumentId> {
        let mime = mime.to_ascii_lowercase();
        let lower_url = url.to_ascii_lowercase();
        let is = |m: &str, ext: &str| mime.contains(m) || lower_url.ends_with(ext);

        if is("epub", ".epub") {
            return self.import_download_as_file(url, &opds_filename(title, "epub"));
        }
        if is("pdf", ".pdf") {
            return self.import_download_as_file(url, &opds_filename(title, "pdf"));
        }

        let meta = DocumentMeta {
            title: title.to_string(),
            author: author.to_string(),
            publisher: Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "OPDS".into()),
            published: String::new(),
        };
        let bytes = fetch_bytes(url)?;
        if mime.contains("html") || lower_url.ends_with(".html") || lower_url.ends_with(".xhtml") {
            let html = String::from_utf8_lossy(&bytes).into_owned();
            return self.import_text_meta(meta, &crate::extract::strip_html(&html));
        }
        if mime.contains("text/plain") || lower_url.ends_with(".txt") {
            let text = match std::str::from_utf8(&bytes) {
                Ok(s) => s.to_string(),
                Err(_) => encoding_rs::WINDOWS_1252.decode(&bytes).0.into_owned(),
            };
            return self.import_text_meta(meta, &text);
        }
        // Unknown type: let the file pipeline sniff it by extension.
        let ext = lower_url.rsplit('.').next().filter(|e| e.len() <= 5).unwrap_or("epub");
        self.import_download_as_file(url, &opds_filename(title, ext))
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
pub(crate) fn urlencode(s: &str) -> String {
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

/// Turn one Gutendex `results[]` object into a [`GutendexHit`], choosing
/// the best plain-text, HTML, and EPUB download URLs from its `formats`
/// map. Returns `None` for entries without a title.
fn parse_gutendex_book(v: &serde_json::Value) -> Option<GutendexHit> {
    let title = v["title"].as_str()?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let author = v["authors"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|a| a["name"].as_str())
        .map(gutenberg_author_name)
        .unwrap_or_default();
    let languages = v["languages"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut text_url = None;
    let mut html_url = None;
    let mut epub_url = None;
    if let Some(formats) = v["formats"].as_object() {
        for (mime, url) in formats {
            let Some(url) = url.as_str() else { continue };
            // Gutenberg offers zipped variants of some formats; those
            // aren't directly importable here, so skip them.
            if url.ends_with(".zip") {
                continue;
            }
            let mime = mime.to_ascii_lowercase();
            if mime.starts_with("text/plain") {
                // Prefer a UTF-8 plain-text variant when several exist.
                if text_url.is_none() || mime.contains("utf-8") {
                    text_url = Some(url.to_string());
                }
            } else if mime.starts_with("application/epub+zip") {
                epub_url = Some(url.to_string());
            } else if mime.starts_with("text/html") && html_url.is_none() {
                html_url = Some(url.to_string());
            }
        }
    }

    Some(GutendexHit {
        id: v["id"].as_u64().unwrap_or(0),
        title,
        author,
        languages,
        download_count: v["download_count"].as_u64().unwrap_or(0),
        text_url,
        html_url,
        epub_url,
    })
}

/// Gutenberg records authors "Surname, Given"; flip to natural order for
/// display when there's exactly one comma.
fn gutenberg_author_name(name: &str) -> String {
    match name.split_once(',') {
        Some((last, first)) if !name.matches(',').nth(1).is_some() => {
            format!("{} {}", first.trim(), last.trim())
        }
        _ => name.to_string(),
    }
}

/// A safe temp-download filename for an OPDS acquisition, keyed on the
/// book title so concurrent imports of different books don't collide.
fn opds_filename(title: &str, ext: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(40)
        .collect();
    let stem = stem.trim_matches('_');
    let stem = if stem.is_empty() { "opds" } else { stem };
    format!("{stem}.{ext}")
}

/// Strip Project Gutenberg's license header and footer, keeping the work
/// itself. The markers have varied over the years ("THE"/"THIS"), so we
/// match the stable prefix and cut at the marker line.
fn strip_gutenberg_boilerplate(text: &str) -> String {
    const STARTS: [&str; 2] = [
        "*** START OF THE PROJECT GUTENBERG",
        "*** START OF THIS PROJECT GUTENBERG",
    ];
    const ENDS: [&str; 2] = [
        "*** END OF THE PROJECT GUTENBERG",
        "*** END OF THIS PROJECT GUTENBERG",
    ];
    let mut body = text;
    if let Some(pos) = STARTS.iter().filter_map(|m| body.find(m)).min() {
        // Drop everything through the end of the marker's own line.
        if let Some(nl) = body[pos..].find('\n') {
            body = &body[pos + nl + 1..];
        }
    }
    if let Some(pos) = ENDS.iter().filter_map(|m| body.find(m)).min() {
        body = &body[..pos];
    }
    body.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutenberg_boilerplate_is_stripped() {
        let raw = "The Project Gutenberg eBook of Foo\nLicense header...\n\
             *** START OF THE PROJECT GUTENBERG EBOOK FOO ***\n\
             Chapter 1.\nThe real text.\n\
             *** END OF THE PROJECT GUTENBERG EBOOK FOO ***\n\
             License footer, donations, etc.";
        assert_eq!(strip_gutenberg_boilerplate(raw), "Chapter 1.\nThe real text.");
        // Text without markers is returned trimmed but intact.
        assert_eq!(strip_gutenberg_boilerplate("  plain body  "), "plain body");
    }

    #[test]
    fn gutenberg_author_name_flips_single_comma() {
        assert_eq!(gutenberg_author_name("Dickens, Charles"), "Charles Dickens");
        assert_eq!(gutenberg_author_name("Anonymous"), "Anonymous");
        // Two commas (e.g. "Last, First, Jr.") are left as-is.
        assert_eq!(gutenberg_author_name("A, B, C"), "A, B, C");
    }

    #[test]
    fn parse_gutendex_picks_best_formats() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
              "id": 564,
              "title": "The Mystery of Edwin Drood",
              "authors": [{"name": "Dickens, Charles"}],
              "languages": ["en"],
              "download_count": 39676,
              "formats": {
                "text/html": "https://www.gutenberg.org/ebooks/564.html.images",
                "application/epub+zip": "https://www.gutenberg.org/ebooks/564.epub3.images",
                "image/jpeg": "https://www.gutenberg.org/cache/epub/564/cover.jpg",
                "application/octet-stream": "https://www.gutenberg.org/files/564/564-0.zip",
                "text/plain; charset=us-ascii": "https://www.gutenberg.org/files/564/564-0.txt",
                "text/plain; charset=utf-8": "https://www.gutenberg.org/ebooks/564.txt.utf-8"
              }
            }"#,
        )
        .unwrap();
        let hit = parse_gutendex_book(&v).unwrap();
        assert_eq!(hit.id, 564);
        assert_eq!(hit.author, "Charles Dickens");
        assert_eq!(hit.languages, vec!["en"]);
        // Prefers the UTF-8 plain-text variant; skips the .zip.
        assert_eq!(
            hit.text_url.as_deref(),
            Some("https://www.gutenberg.org/ebooks/564.txt.utf-8")
        );
        assert_eq!(
            hit.epub_url.as_deref(),
            Some("https://www.gutenberg.org/ebooks/564.epub3.images")
        );
        assert!(hit.html_url.is_some());
        assert!(hit.is_importable());
    }

    #[test]
    fn opds_filename_is_safe() {
        assert_eq!(opds_filename("Alice's Adventures!", "epub"), "Alice_s_Adventures.epub");
        assert_eq!(opds_filename("", "pdf"), "opds.pdf");
    }

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
        let app = App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap();
        // Build a tiny catalog zip in memory: header + 4 rows exercising
        // dedupe (translator row), copyright filter, and host filter.
        let header: Vec<String> = (0..55).map(|i| format!("c{i}")).collect();
        let row = |id: &str, title: &str, role: &str, copyright: &str, url: &str| {
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
            row(
                "000001",
                "こころ",
                "著者",
                "なし",
                "https://www.aozora.gr.jp/cards/x/files/1_2.html"
            ),
            row(
                "000001",
                "こころ",
                "翻訳者",
                "なし",
                "https://www.aozora.gr.jp/cards/x/files/1_2.html"
            ),
            row(
                "000002",
                "著作権あり",
                "著者",
                "あり",
                "https://www.aozora.gr.jp/cards/y/files/3_4.html"
            ),
            row(
                "000003",
                "外部",
                "著者",
                "なし",
                "https://example.com/foo.html"
            ),
        );
        let mut zip_bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
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
