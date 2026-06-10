//! Plain-text extraction from the file formats users actually have books
//! in: .txt (UTF-8 or Shift_JIS), HTML (including Aozora Bunko pages with
//! ruby furigana), EPUB, and PDF.

use std::path::Path;

use jrc_core::DocumentMeta;

use crate::{AppError, Result};

/// Text plus whatever descriptive metadata the format provided.
#[derive(Debug, Clone, Default)]
pub struct ExtractedDoc {
    pub text: String,
    /// Title/author/… with empty strings where the format had nothing.
    pub meta: DocumentMeta,
}

/// Extract readable Japanese text from a file, chosen by extension.
pub fn extract_text(path: &Path) -> Result<String> {
    Ok(extract_document(path)?.text)
}

/// Extract text and metadata from a file, chosen by extension.
pub fn extract_document(path: &Path) -> Result<ExtractedDoc> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "md" | "text" | "" => {
            let text = decode_bytes(&std::fs::read(path)?);
            let meta = txt_metadata(&text);
            Ok(ExtractedDoc { text, meta })
        }
        "html" | "htm" | "xhtml" => {
            let html = decode_bytes(&std::fs::read(path)?);
            let meta = DocumentMeta {
                title: html_title(&html).unwrap_or_default(),
                ..Default::default()
            };
            Ok(ExtractedDoc {
                text: strip_html(&html),
                meta,
            })
        }
        "epub" => epub_document(path),
        // PDF metadata is rarely populated and often mojibake; the title
        // falls back to the filename at the import layer.
        "pdf" => Ok(ExtractedDoc {
            text: pdf_text(path)?,
            meta: DocumentMeta::default(),
        }),
        other => Err(AppError::Invalid(format!(
            "unsupported file type .{other} (supported: txt, md, html, epub, pdf)"
        ))),
    }
}

/// Aozora Bunko plain-text convention: line 1 is the title, line 2 the
/// author, then a blank line before the body. Only trust it when the
/// lines are short enough to plausibly be a heading — this is a prefill
/// suggestion, not gospel; the import form remains editable.
fn txt_metadata(text: &str) -> DocumentMeta {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let (first, second) = (lines.next(), lines.next());
    let mut meta = DocumentMeta::default();
    if let Some(first) = first {
        if first.chars().count() <= 40 && !first.ends_with('。') {
            meta.title = first.to_string();
            if let Some(second) = second {
                if second.chars().count() <= 20 && !second.ends_with('。') {
                    meta.author = second.to_string();
                }
            }
        }
    }
    meta
}

/// Contents of the first `<title>` element, entity-decoded.
fn html_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let open_end = start + lower[start..].find('>')?;
    let close = open_end + lower[open_end..].find("</title")?;
    let raw = &html[open_end + 1..close];
    let title = strip_html(raw).trim().to_string();
    (!title.is_empty()).then_some(title)
}

/// Decode text bytes: UTF-8 (with or without BOM) when valid, otherwise
/// Shift_JIS — the encoding of virtually all legacy Japanese text files
/// (Aozora Bunko ships Shift_JIS). Japanese Shift_JIS is essentially never
/// valid UTF-8, so the check is a reliable discriminator.
pub fn decode_bytes(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (text, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
            text.into_owned()
        }
    }
}

/// Reduce HTML to readable text.
///
/// - `<rt>`/`<rp>` (furigana readings) are dropped so ruby annotations do
///   not pollute the text: 漢字(かんじ) stays 漢字.
/// - `<script>`, `<style>`, `<head>` contents are dropped.
/// - `<br>` and closing block tags become newlines (paragraph structure
///   survives into the importer's paragraph splitting).
/// - Common entities are decoded.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut chars = html.char_indices().peekable();
    // When set, drop text until this closing tag is seen.
    let mut skip_until: Option<&'static str> = None;

    while let Some((i, c)) = chars.next() {
        if c != '<' {
            if skip_until.is_none() {
                match c {
                    '\n' | '\r' => push_newline(&mut out),
                    '&' => {
                        let rest = &html[i..];
                        let (decoded, len) = decode_entity(rest);
                        out.push_str(&decoded);
                        for _ in 0..len.saturating_sub(1) {
                            chars.next();
                        }
                    }
                    _ => out.push(c),
                }
            }
            continue;
        }

        // Collect the tag up to '>'.
        let mut tag = String::new();
        for (_, tc) in chars.by_ref() {
            if tc == '>' {
                break;
            }
            tag.push(tc);
        }
        // Comments: <!-- ... --> ('>' inside is rare; good enough here).
        if tag.starts_with("!--") {
            continue;
        }
        let tag_lower = tag.to_lowercase();
        let closing = tag_lower.starts_with('/');
        let name: String = tag_lower
            .trim_start_matches('/')
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric())
            .collect();

        if let Some(until) = skip_until {
            if closing && name == until {
                skip_until = None;
            }
            continue;
        }
        match name.as_str() {
            "script" | "style" | "head" | "rt" | "rp" if !closing => {
                // Self-closing (<rt/>) cannot happen in practice; ignore.
                skip_until = Some(match name.as_str() {
                    "script" => "script",
                    "style" => "style",
                    "head" => "head",
                    "rt" => "rt",
                    _ => "rp",
                });
            }
            "br" => push_newline(&mut out),
            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "tr"
            | "blockquote" | "section" | "article"
                if closing =>
            {
                push_newline(&mut out)
            }
            _ => {}
        }
    }

    // Tidy: drop whitespace-only lines, collapse blank runs.
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if matches!(lines.last(), Some(l) if !l.is_empty()) {
                lines.push("");
            }
        } else {
            lines.push(trimmed);
        }
    }
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines.join("\n")
}

/// Append a line break unless we just emitted one — `</p><br>` and similar
/// stacks collapse to a single break (the importer's paragraph splitter
/// treats any newline run as a paragraph boundary anyway).
fn push_newline(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Decode one HTML entity at the start of `s` (which begins with `&`).
/// Returns the decoded string and the number of bytes consumed.
fn decode_entity(s: &str) -> (String, usize) {
    let end = match s[..s.len().min(12)].find(';') {
        Some(e) => e,
        None => return ("&".to_string(), 1),
    };
    let body = &s[1..end];
    let decoded = match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = body.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    };
    match decoded {
        Some(ch) => (ch.to_string(), end + 1),
        None => ("&".to_string(), 1),
    }
}

/// Concatenate the spine documents of an EPUB, stripped to text, plus its
/// Dublin Core metadata.
fn epub_document(path: &Path) -> Result<ExtractedDoc> {
    let mut doc = epub::doc::EpubDoc::new(path)
        .map_err(|e| AppError::Invalid(format!("could not open epub: {e}")))?;

    let dc = |name: &str| {
        doc.mdata(name)
            .map(|item| item.value.trim().to_string())
            .unwrap_or_default()
    };
    let meta = DocumentMeta {
        title: dc("title"),
        author: dc("creator"),
        publisher: dc("publisher"),
        published: dc("date"),
    };

    let idrefs: Vec<String> = doc.spine.iter().map(|item| item.idref.clone()).collect();
    let mut parts = Vec::new();
    for idref in idrefs {
        if let Some((content, _mime)) = doc.get_resource_str(&idref) {
            let text = strip_html(&content);
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }
    if parts.is_empty() {
        return Err(AppError::Invalid(
            "no readable text found in the epub".into(),
        ));
    }
    Ok(ExtractedDoc {
        text: parts.join("\n\n"),
        meta,
    })
}

/// Extract text from a PDF. pdf-extract can panic on malformed files, so
/// the call is unwind-guarded — a bad PDF becomes an error, not a crash.
fn pdf_text(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    let result = std::panic::catch_unwind(move || pdf_extract::extract_text(&path));
    match result {
        Ok(Ok(text)) if !text.trim().is_empty() => Ok(text),
        Ok(Ok(_)) => Err(AppError::Invalid(
            "no extractable text in the PDF (it may be a scan; try OCR first)".into(),
        )),
        Ok(Err(e)) => Err(AppError::Invalid(format!("could not read PDF: {e}"))),
        Err(_) => Err(AppError::Invalid(
            "the PDF could not be parsed (unsupported or corrupt file)".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_annotations_are_stripped() {
        let html = "<p>その<ruby><rb>漢字</rb><rp>（</rp><rt>かんじ</rt><rp>）</rp></ruby>は難しい。</p>";
        assert_eq!(strip_html(html), "その漢字は難しい。");
        // EPUB3 style without rb.
        let html = "<ruby>本<rt>ほん</rt></ruby>を読む";
        assert_eq!(strip_html(html), "本を読む");
    }

    #[test]
    fn block_tags_become_paragraph_breaks() {
        let html = "<html><head><title>x</title></head><body>\
                    <p>一行目。</p><p>二行目。</p><br>三行目。</body></html>";
        assert_eq!(strip_html(html), "一行目。\n二行目。\n三行目。");
    }

    #[test]
    fn scripts_styles_and_entities() {
        let html = "<style>body{color:red}</style><script>var x=1;</script>\
                    A &amp; B &lt;C&gt; &#x732B; &#29356;";
        assert_eq!(strip_html(html), "A & B <C> 猫 犬");
    }

    #[test]
    fn malformed_entities_pass_through() {
        assert_eq!(strip_html("R&D and &unknown; stay"), "R&D and &unknown; stay");
    }

    #[test]
    fn decodes_utf8_and_shift_jis() {
        assert_eq!(decode_bytes("日本語".as_bytes()), "日本語");
        assert_eq!(decode_bytes(b"\xEF\xBB\xBF\xE7\x8C\xAB"), "猫");
        // 日本語 in Shift_JIS.
        assert_eq!(decode_bytes(b"\x93\xFA\x96\x7B\x8C\xEA"), "日本語");
    }

    #[test]
    fn unsupported_extension_is_a_clear_error() {
        let err = extract_text(Path::new("book.docx")).unwrap_err();
        assert!(err.to_string().contains("unsupported file type"));
    }

    #[test]
    fn epub_spine_is_extracted_in_order() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        // Build a minimal EPUB in memory.
        let dir = std::env::temp_dir().join("jrc-test-epub");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut z = zip::ZipWriter::new(file);
        let opts: SimpleFileOptions = SimpleFileOptions::default();

        z.start_file("mimetype", opts).unwrap();
        z.write_all(b"application/epub+zip").unwrap();
        z.start_file("META-INF/container.xml", opts).unwrap();
        z.write_all(
            br#"<?xml version="1.0"?>
            <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
              <rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
            </container>"#,
        )
        .unwrap();
        z.start_file("content.opf", opts).unwrap();
        z.write_all(
            r#"<?xml version="1.0"?>
            <package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
              <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
                <dc:title>テスト本</dc:title><dc:identifier id="id">x</dc:identifier>
                <dc:creator>テスト著者</dc:creator>
                <dc:language>ja</dc:language>
              </metadata>
              <manifest>
                <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
                <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
              </manifest>
              <spine><itemref idref="c1"/><itemref idref="c2"/></spine>
            </package>"#
                .as_bytes(),
        )
        .unwrap();
        z.start_file("c1.xhtml", opts).unwrap();
        z.write_all(
            "<html><body><p>第一章。<ruby>猫<rt>ねこ</rt></ruby>がいた。</p></body></html>"
                .as_bytes(),
        )
        .unwrap();
        z.start_file("c2.xhtml", opts).unwrap();
        z.write_all("<html><body><p>第二章。犬もいた。</p></body></html>".as_bytes())
            .unwrap();
        z.finish().unwrap();

        let doc = extract_document(&path).unwrap();
        let first = doc
            .text
            .find("第一章。猫がいた。")
            .expect("chapter 1 with ruby stripped");
        let second = doc.text.find("第二章。犬もいた。").expect("chapter 2");
        assert!(first < second, "spine order preserved");
        assert_eq!(doc.meta.title, "テスト本");
        assert_eq!(doc.meta.author, "テスト著者");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn aozora_txt_heading_becomes_metadata() {
        let text = "走れメロス\n太宰治\n\nメロスは激怒した。必ず、かの邪智暴虐の王を除かなければならぬと決意した。";
        let meta = txt_metadata(text);
        assert_eq!(meta.title, "走れメロス");
        assert_eq!(meta.author, "太宰治");

        // Ordinary prose must not be mistaken for a heading.
        let prose = "メロスは激怒した。\n必ず、かの邪智暴虐の王を除かなければならぬと決意した。";
        let meta = txt_metadata(prose);
        assert_eq!(meta.title, "");
        assert_eq!(meta.author, "");
    }

    #[test]
    fn html_title_is_extracted() {
        let html = "<html><head><title>太宰治 走れメロス</title></head><body>x</body></html>";
        assert_eq!(html_title(html).as_deref(), Some("太宰治 走れメロス"));
        assert_eq!(html_title("<html><body>no title</body></html>"), None);
    }
}
