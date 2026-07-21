//! OPDS catalog parsing — the network-free half of OPDS support.
//!
//! Handles OPDS 1.x (Atom XML), OpenSearch description documents, OPDS
//! 2.0 (JSON), URI-template expansion, and relative-href resolution. The
//! `App` methods that actually fetch feeds and import books live in
//! [`crate::sources`]; everything here operates on already-fetched
//! strings so it can be unit-tested without a network.
//!
//! Matching is deliberately namespace-agnostic: real feeds declare the
//! `opds` namespace under different URIs, so links are classified by
//! their `rel`/`type` attribute *strings* and elements by local name.

use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::Value;
use url::Url;

use crate::sources::{urlencode, OpdsHit};

/// The `rel` prefix shared by every OPDS acquisition link.
const ACQUISITION_REL: &str = "http://opds-spec.org/acquisition";

/// A link from a feed or entry, with its href resolved to absolute.
#[derive(Debug, Clone)]
pub(crate) struct FeedLink {
    pub rel: String,
    pub mime: String,
    pub href: String,
}

impl FeedLink {
    fn is_acquisition(&self) -> bool {
        self.rel.starts_with(ACQUISITION_REL)
    }

    fn is_navigation(&self) -> bool {
        self.mime.contains("profile=opds-catalog")
    }
}

/// One catalog entry (a book or a subfeed).
#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedEntry {
    pub title: String,
    pub author: String,
    pub summary: String,
    pub links: Vec<FeedLink>,
}

impl ParsedEntry {
    /// The book this entry represents, if it has any acquisition links.
    pub fn to_hit(&self) -> Option<OpdsHit> {
        let links: Vec<(String, String)> = self
            .links
            .iter()
            .filter(|l| l.is_acquisition())
            .map(|l| (l.mime.clone(), l.href.clone()))
            .collect();
        if links.is_empty() {
            return None;
        }
        Some(OpdsHit {
            title: self.title.clone(),
            author: self.author.clone(),
            summary: self.summary.clone(),
            links,
        })
    }

    /// The URL of the subfeed this navigation entry points at, if any.
    pub fn navigation_href(&self) -> Option<&str> {
        self.links
            .iter()
            .find(|l| l.is_navigation() || l.rel == "subsection")
            .map(|l| l.href.as_str())
    }
}

/// A parsed feed: its search link (if advertised) and its entries.
#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedFeed {
    pub search: Option<FeedLink>,
    pub entries: Vec<ParsedEntry>,
}

/// Resolve a (non-template) href against the feed's base URL.
pub(crate) fn resolve(base: &Url, href: &str) -> String {
    base.join(href)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| href.to_string())
}

/// Resolve an href that may contain a `{...}` URI template, without
/// percent-encoding the template braces. Only the part before the first
/// brace is resolved against `base`.
fn resolve_template(base: &Url, href: &str) -> String {
    match href.find('{') {
        Some(idx) => {
            let (prefix, tmpl) = href.split_at(idx);
            let resolved = base
                .join(prefix)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| prefix.to_string());
            format!("{resolved}{tmpl}")
        }
        None => resolve(base, href),
    }
}

fn local(name: &[u8]) -> &[u8] {
    match name.iter().position(|&b| b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

/// Text-capture target while streaming XML.
#[derive(PartialEq, Clone, Copy)]
enum Target {
    None,
    Title,
    Author,
    Summary,
}

/// Parse an OPDS 1.x Atom feed. `base` is the URL the feed was fetched
/// from (used to resolve relative hrefs); a `<feed xml:base>` overrides it.
pub(crate) fn parse_atom(xml: &str, base: &Url) -> ParsedFeed {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut base = base.clone();
    let mut feed = ParsedFeed::default();
    let mut feed_links: Vec<FeedLink> = Vec::new();

    let mut in_entry = false;
    let mut in_author = false;
    let mut entry = ParsedEntry::default();
    let mut target = Target::None;

    loop {
        match reader.read_event() {
            // Opening tags may set text-capture state or start an entry.
            Ok(Event::Start(e)) => {
                match local(e.name().as_ref()) {
                    b"feed" => {
                        if let Some(b) = attr(&e, b"base") {
                            if let Ok(u) = Url::parse(&b) {
                                base = u;
                            }
                        }
                    }
                    b"entry" => {
                        in_entry = true;
                        entry = ParsedEntry::default();
                    }
                    b"link" => push_link(&e, &base, in_entry, &mut entry, &mut feed_links),
                    b"author" if in_entry => in_author = true,
                    b"title" if in_entry => target = Target::Title,
                    b"name" if in_entry && in_author => target = Target::Author,
                    b"summary" | b"content" if in_entry => target = Target::Summary,
                    _ => {}
                }
            }
            // Self-closing tags carry no text, so they must not set a
            // capture target (which would misattribute later text). Only
            // `<link/>` matters here — its data is all in attributes.
            Ok(Event::Empty(e)) => {
                if local(e.name().as_ref()) == b"link" {
                    push_link(&e, &base, in_entry, &mut entry, &mut feed_links);
                }
            }
            Ok(Event::Text(t)) => {
                let decoded = t.decode().unwrap_or_default();
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| decoded.into_owned());
                append(&mut entry, target, &unescaped);
            }
            // CDATA is literal text (no entity unescaping); a feed may wrap
            // a title or summary in it.
            Ok(Event::CData(t)) => {
                let text = String::from_utf8_lossy(&t.into_inner()).into_owned();
                append(&mut entry, target, &text);
            }
            Ok(Event::End(e)) => match local(e.name().as_ref()) {
                b"entry" => {
                    in_entry = false;
                    target = Target::None;
                    feed.entries.push(std::mem::take(&mut entry));
                }
                b"author" => {
                    in_author = false;
                    if target == Target::Author {
                        target = Target::None;
                    }
                }
                b"title" | b"name" | b"summary" | b"content" => target = Target::None,
                _ => {}
            },
            Ok(Event::Eof) => break,
            // Ignore <?xml-stylesheet?> PIs, declarations, comments.
            Ok(_) => {}
            Err(_) => break,
        }
    }

    feed.search = pick_search_link(feed_links);
    feed
}

/// Build a `<link>` and push it to the entry (or the feed's links).
fn push_link(
    e: &quick_xml::events::BytesStart,
    base: &Url,
    in_entry: bool,
    entry: &mut ParsedEntry,
    feed_links: &mut Vec<FeedLink>,
) {
    let rel = attr(e, b"rel").unwrap_or_default();
    let mime = attr(e, b"type").unwrap_or_default();
    let href = attr(e, b"href").unwrap_or_default();
    if href.is_empty() {
        return;
    }
    let link = FeedLink {
        href: if rel == "search" {
            resolve_template(base, &href)
        } else {
            resolve(base, &href)
        },
        rel,
        mime,
    };
    if in_entry {
        entry.links.push(link);
    } else {
        feed_links.push(link);
    }
}

/// Append captured text to the current target field of `entry`.
fn append(entry: &mut ParsedEntry, target: Target, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let slot = match target {
        Target::Title => &mut entry.title,
        Target::Author => &mut entry.author,
        Target::Summary => &mut entry.summary,
        Target::None => return,
    };
    if slot.is_empty() {
        *slot = text.to_string();
    } else {
        slot.push(' ');
        slot.push_str(text);
    }
}

/// Choose the best search link: prefer an OpenSearch description, else
/// any `rel="search"` link.
fn pick_search_link(links: Vec<FeedLink>) -> Option<FeedLink> {
    let searches: Vec<FeedLink> = links.into_iter().filter(|l| l.rel == "search").collect();
    searches
        .iter()
        .find(|l| l.mime.contains("opensearchdescription"))
        .cloned()
        .or_else(|| searches.into_iter().next())
}

/// Attribute value (unescaped) by local name.
fn attr(e: &quick_xml::events::BytesStart, want: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if local(attr.key.as_ref()) == want {
            return Some(
                attr.unescape_value()
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| String::from_utf8_lossy(&attr.value).into_owned()),
            );
        }
    }
    None
}

/// Extract the OPDS/Atom search template from an OpenSearch description
/// document, resolved (minus its template braces) against `base`.
pub(crate) fn parse_osd(xml: &str, base: &Url) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut fallback: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if local(e.name().as_ref()) == b"Url" {
                    let mime = attr(&e, b"type").unwrap_or_default();
                    let template = attr(&e, b"template").unwrap_or_default();
                    let rel = attr(&e, b"rel").unwrap_or_default();
                    if template.is_empty() || rel == "suggestions" {
                        continue;
                    }
                    // Prefer an Atom/OPDS result template; keep the first
                    // usable one as a fallback.
                    if mime.contains("atom") || mime.contains("opds") {
                        return Some(resolve_template(base, &template));
                    }
                    if fallback.is_none() && !mime.contains("suggestions") {
                        fallback = Some(resolve_template(base, &template));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    fallback
}

/// Expand a URI/OpenSearch template with the user's `query`. Handles the
/// OpenSearch `{searchTerms}` token and RFC 6570 form-style query
/// expansions like `{?query}`; unknown tokens are dropped.
pub(crate) fn expand_template(template: &str, query: &str) -> String {
    let enc = urlencode(query);
    let mut out = String::with_capacity(template.len() + enc.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            // Unterminated brace: emit the remainder verbatim.
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &rest[start + 1..start + end];
        out.push_str(&expand_token(token, &enc));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

fn is_query_var(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "searchterms" | "query" | "q" | "search" | "kw" | "keywords" | "keyword"
    )
}

/// Expand one `{...}` token's contents.
fn expand_token(token: &str, enc: &str) -> String {
    // Literal OpenSearch search-terms token.
    if token == "searchTerms" {
        return enc.to_string();
    }
    // RFC 6570 form-style query expansion: {?a,b} or {&a,b}.
    if let Some(vars) = token.strip_prefix('?').or_else(|| token.strip_prefix('&')) {
        let sep = if token.starts_with('?') { '?' } else { '&' };
        let names: Vec<&str> = vars.split(',').map(str::trim).filter(|v| !v.is_empty()).collect();
        // A single-variable template (e.g. `{?query}`, `{?q}`, `{?text}`)
        // is the search parameter whatever it's named; with several, only
        // fill the ones that clearly carry the query.
        let single = names.len() == 1;
        let parts: Vec<String> = names
            .iter()
            .filter(|v| single || is_query_var(v))
            .map(|v| format!("{v}={enc}"))
            .collect();
        if parts.is_empty() {
            return String::new();
        }
        return format!("{sep}{}", parts.join("&"));
    }
    // Bare query variable, e.g. {searchTerms} handled above, {q}.
    if is_query_var(token) {
        return enc.to_string();
    }
    // Unknown optional token: drop it.
    String::new()
}

// ── OPDS 2.0 (JSON) ─────────────────────────────────────────────────

/// Does a JSON `rel` field (string or array) contain `needle`?
fn json_rel_contains(link: &Value, needle: &str) -> bool {
    match &link["rel"] {
        Value::String(s) => s.contains(needle),
        Value::Array(a) => a.iter().any(|r| r.as_str().is_some_and(|s| s.contains(needle))),
        _ => false,
    }
}

/// Normalize an OPDS 2.0 author field (string | {name} | array) to text.
fn json_author(meta: &Value) -> String {
    fn one(v: &Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("name").and_then(|n| n.as_str()).map(str::to_string),
            _ => None,
        }
    }
    match &meta["author"] {
        Value::Array(a) => a.iter().filter_map(one).collect::<Vec<_>>().join(", "),
        other => one(other).unwrap_or_default(),
    }
}

/// Parse an OPDS 2.0 JSON feed into the shared [`ParsedFeed`] shape.
pub(crate) fn parse_opds2(v: &Value, base: &Url) -> ParsedFeed {
    let mut feed = ParsedFeed::default();

    if let Some(links) = v["links"].as_array() {
        if let Some(l) = links.iter().find(|l| json_rel_contains(l, "search")) {
            if let Some(href) = l["href"].as_str() {
                feed.search = Some(FeedLink {
                    rel: "search".into(),
                    mime: l["type"].as_str().unwrap_or_default().to_string(),
                    href: resolve_template(base, href),
                });
            }
        }
    }

    if let Some(pubs) = v["publications"].as_array() {
        for p in pubs {
            let meta = &p["metadata"];
            let title = meta["title"].as_str().unwrap_or_default().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let summary = meta["description"]
                .as_str()
                .or_else(|| meta["subtitle"].as_str())
                .unwrap_or_default()
                .to_string();
            let mut links = Vec::new();
            if let Some(ls) = p["links"].as_array() {
                for l in ls {
                    if json_rel_contains(l, ACQUISITION_REL) || json_rel_contains(l, "acquisition") {
                        if let Some(href) = l["href"].as_str() {
                            links.push(FeedLink {
                                rel: ACQUISITION_REL.into(),
                                mime: l["type"].as_str().unwrap_or_default().to_string(),
                                href: resolve(base, href),
                            });
                        }
                    }
                }
            }
            feed.entries.push(ParsedEntry {
                title,
                author: json_author(meta),
                summary,
                links,
            });
        }
    }
    feed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.org/opds/").unwrap()
    }

    #[test]
    fn expand_opensearch_and_rfc6570_templates() {
        assert_eq!(
            expand_template("http://m.gutenberg.org/ebooks/search.opds/?query={searchTerms}", "war peace"),
            "http://m.gutenberg.org/ebooks/search.opds/?query=war%20peace"
        );
        assert_eq!(
            expand_template("https://openlibrary.org/opds/search{?query}", "hugo"),
            "https://openlibrary.org/opds/search?query=hugo"
        );
        // Unknown tokens are dropped.
        assert_eq!(
            expand_template("/s?q={searchTerms}&i={startIndex}", "x"),
            "/s?q=x&i="
        );
        // A single-variable form template is the query, whatever its name.
        assert_eq!(
            expand_template("https://x/opds/search{?text}", "hugo"),
            "https://x/opds/search?text=hugo"
        );
    }

    #[test]
    fn self_closing_and_cdata_do_not_corrupt_titles() {
        // A self-closed <title/> and an out-of-line <content src=.../> must
        // not capture the following <id>/<updated> text.
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom">
          <entry>
            <title/>
            <id>urn:uuid:123</id>
            <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/a.epub"/>
          </entry>
          <entry>
            <title><![CDATA[Les Misérables]]></title>
            <content type="application/pdf" src="/b.pdf"/>
            <id>urn:uuid:abc</id>
            <link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="/b.epub"/>
          </entry>
        </feed>"#;
        let feed = parse_atom(xml, &base());
        // First entry: empty title stays empty, not "urn:uuid:123".
        assert_eq!(feed.entries[0].title, "");
        // Second entry: CDATA title captured; <content src/> didn't leak
        // the <id> into the summary.
        assert_eq!(feed.entries[1].title, "Les Misérables");
        assert_eq!(feed.entries[1].summary, "");
    }

    #[test]
    fn parses_atom_acquisition_entry() {
        let xml = r#"<?xml version="1.0"?>
        <?xml-stylesheet href="style.xsl" type="text/xsl"?>
        <feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
          <link rel="search" type="application/opensearchdescription+xml" href="/osd.xml"/>
          <entry>
            <title>Alice's Adventures in Wonderland</title>
            <author><name>Carroll, Lewis</name></author>
            <summary>A classic.</summary>
            <link type="application/epub+zip" rel="http://opds-spec.org/acquisition" href="/books/11.epub"/>
            <link type="image/jpeg" rel="http://opds-spec.org/image" href="data:image/png;base64,AAAA"/>
          </entry>
          <entry>
            <title>A Section</title>
            <link type="application/atom+xml;profile=opds-catalog" rel="subsection" href="/section/1"/>
          </entry>
        </feed>"#;
        let feed = parse_atom(xml, &base());
        assert!(feed.search.is_some());
        let search = feed.search.unwrap();
        assert_eq!(search.href, "https://example.org/osd.xml");
        assert!(search.mime.contains("opensearchdescription"));
        assert_eq!(feed.entries.len(), 2);

        let alice = feed.entries[0].to_hit().unwrap();
        assert_eq!(alice.title, "Alice's Adventures in Wonderland");
        assert_eq!(alice.author, "Carroll, Lewis");
        // Only the acquisition link is kept (image dropped).
        assert_eq!(alice.links.len(), 1);
        assert_eq!(alice.links[0].1, "https://example.org/books/11.epub");
        assert!(alice.best_link().unwrap().0.contains("epub"));

        // The navigation entry is not a book.
        assert!(feed.entries[1].to_hit().is_none());
        assert_eq!(
            feed.entries[1].navigation_href(),
            Some("https://example.org/section/1")
        );
    }

    #[test]
    fn parses_opensearch_description() {
        let xml = r#"<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
           <Url type="text/html" template="http://x/html?q={searchTerms}"/>
           <Url type="application/atom+xml" template="http://x/atom?query={searchTerms}"/>
           <Url type="application/x-suggestions+json" rel="suggestions" template="http://x/sug?q={searchTerms}"/>
        </OpenSearchDescription>"#;
        let tmpl = parse_osd(xml, &base()).unwrap();
        assert_eq!(tmpl, "http://x/atom?query={searchTerms}");
    }

    #[test]
    fn parses_opds2_publications() {
        let json = r#"{
          "metadata": {"title": "Feed"},
          "links": [
            {"rel": "self", "href": "/opds", "type": "application/opds+json"},
            {"rel": "search", "href": "https://openlibrary.org/opds/search{?query}", "type": "application/opds+json", "templated": true}
          ],
          "publications": [
            {
              "metadata": {"title": "Voyage au centre de la Terre", "author": "Jules Verne", "language": "fr"},
              "links": [
                {"rel": "http://opds-spec.org/acquisition/open-access", "href": "/assets/file.epub", "type": "application/epub+zip"}
              ]
            }
          ]
        }"#;
        let v: Value = serde_json::from_str(json).unwrap();
        let feed = parse_opds2(&v, &base());
        assert_eq!(
            feed.search.as_ref().unwrap().href,
            "https://openlibrary.org/opds/search{?query}"
        );
        let hit = feed.entries[0].to_hit().unwrap();
        assert_eq!(hit.title, "Voyage au centre de la Terre");
        assert_eq!(hit.author, "Jules Verne");
        assert_eq!(hit.links[0].1, "https://example.org/assets/file.epub");
    }
}
