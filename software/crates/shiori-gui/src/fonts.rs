//! Japanese font loading: the system font or a downloaded Noto font.
//!
//! egui's bundled fonts have no CJK coverage, so without this every kanji
//! renders as a placeholder box. The system option picks up whatever the
//! OS provides; the Noto options are fetched once into `<data>/fonts` and
//! cached (the binary ships no fonts).
//!
//! Latin text keeps egui's own (crisp) default font. The Japanese font is
//! a *fallback* with a vertical-offset tweak: fallback glyphs are laid out
//! with their own font's ascent, and Japanese fonts — Yu Gothic
//! notoriously so — carry large ascents that make kana/kanji float above
//! the Latin baseline and clip in text inputs. The tweak pushes them back
//! down onto the baseline. Meiryo is preferred over Yu Gothic because its
//! vertical metrics are far better behaved.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui;

use crate::settings::ReaderFont;

/// Downward shift applied to the Japanese fallback font, as a fraction of
/// the font size. Compensates for the oversized ascent of Japanese fonts
/// relative to egui's Latin font.
const JP_Y_OFFSET_FACTOR: f32 = 0.09;

/// Static-instance Regular TTFs served by Google Fonts. The variable
/// `[wght]` builds on github.com/google/fonts are unsuitable: their fvar
/// default weight is Thin/ExtraLight, which is what egui would render.
const NOTO_SANS_URL: &str =
    "https://fonts.gstatic.com/s/notosansjp/v56/-F6jfjtqLzI2JPCgQBnw7HFyzSD-AsregP8VFBEj75s.ttf";
const NOTO_SERIF_URL: &str =
    "https://fonts.gstatic.com/s/notoserifjp/v33/xn71YHs72GKoTvER4Gn3b5eMRtWGkp6o7MjQ2bwxOubA.ttf";

/// Common Japanese system font locations, in preference order.
fn candidate_paths() -> Vec<&'static str> {
    vec![
        // Windows — Meiryo first: Yu Gothic's vertical metrics sit glyphs
        // far above the baseline in most non-DirectWrite renderers.
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        // macOS
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/Library/Fonts/Osaka.ttf",
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
    ]
}

/// Local cache path of a downloadable font; `None` for the system font.
pub fn font_cache_path(data_dir: &Path, font: ReaderFont) -> Option<PathBuf> {
    let name = match font {
        ReaderFont::System => return None,
        ReaderFont::NotoSans => "NotoSansJP.ttf",
        ReaderFont::NotoSerif => "NotoSerifJP.ttf",
    };
    Some(data_dir.join("fonts").join(name))
}

/// Whether the font can be installed right now, without a download.
pub fn font_available(data_dir: &Path, font: ReaderFont) -> bool {
    match font_cache_path(data_dir, font) {
        None => true,
        Some(path) => path.exists(),
    }
}

/// Download a Noto font into the cache. Blocking — run on a worker
/// thread. No-op for the system font or an already-cached file.
pub fn download_font(data_dir: &Path, font: ReaderFont) -> Result<(), String> {
    let Some(target) = font_cache_path(data_dir, font) else {
        return Ok(());
    };
    if target.exists() {
        return Ok(());
    }
    let url = match font {
        ReaderFont::NotoSans => NOTO_SANS_URL,
        ReaderFont::NotoSerif => NOTO_SERIF_URL,
        ReaderFont::System => return Ok(()),
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let agent = ureq::AgentBuilder::new().user_agent("shiori/0.1").build();
    let response = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut response.into_reader(), &mut bytes)
        .map_err(|e| e.to_string())?;
    // TrueType magic check before trusting the cache with it.
    if bytes.len() < 4 || bytes[..4] != [0x00, 0x01, 0x00, 0x00] {
        return Err("downloaded file is not a TrueType font".into());
    }
    // Write-then-rename so a cancelled download never half-caches.
    let tmp = target.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok(())
}

/// Install the chosen Japanese font as a tweaked fallback for both
/// proportional and monospace text. Returns `false` when the choice is a
/// Noto font that has not been downloaded yet (caller should download
/// and retry); the system option always succeeds.
pub fn install_japanese_fonts(ctx: &egui::Context, data_dir: &Path, font: ReaderFont) -> bool {
    let bytes = match font_cache_path(data_dir, font) {
        Some(path) => match std::fs::read(&path) {
            Ok(bytes) => {
                eprintln!("loaded Japanese font: {}", path.display());
                Some(bytes)
            }
            Err(_) => return false,
        },
        None => candidate_paths().into_iter().find_map(|p| {
            std::fs::read(p).ok().inspect(|_| {
                eprintln!("loaded Japanese font: {p}");
            })
        }),
    };
    let Some(bytes) = bytes else {
        eprintln!("warning: no Japanese system font found; CJK text will not render");
        return true;
    };

    let mut fonts = egui::FontDefinitions::default();
    let font_data = egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
        y_offset_factor: JP_Y_OFFSET_FACTOR,
        ..Default::default()
    });
    fonts
        .font_data
        .insert("japanese".to_owned(), Arc::new(font_data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("japanese".to_owned());
    }
    push_script_fallback(&mut fonts);
    ctx.set_fonts(fonts);
    true
}

/// System fonts with broad non-CJK script coverage, in preference order.
/// Segoe UI covers polytonic Greek (Greek Extended) completely; the
/// Japanese system fonts cover almost none of it.
fn script_fallback_paths() -> Vec<&'static str> {
    vec![
        // A pack-downloaded Gentium would be picked up here first once
        // font downloads land; system fonts carry Greek until then.
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ]
}

/// Append a wide-coverage fallback font so non-CJK scripts (polytonic
/// Greek for the Koine pack, Cyrillic…) render instead of tofu. Sits
/// after the Japanese fallback, so CJK glyph resolution is unchanged.
fn push_script_fallback(fonts: &mut egui::FontDefinitions) {
    let Some(bytes) = script_fallback_paths()
        .into_iter()
        .find_map(|p| std::fs::read(p).ok())
    else {
        return;
    };
    fonts.font_data.insert(
        "script-fallback".to_owned(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("script-fallback".to_owned());
    }
}
