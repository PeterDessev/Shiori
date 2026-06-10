//! Load a Japanese-capable font from the operating system.
//!
//! egui's bundled fonts have no CJK coverage, so without this every kanji
//! renders as a placeholder box. Nothing is bundled; we pick up whatever
//! the OS provides.

use std::sync::Arc;

use eframe::egui;

/// Common Japanese font locations, in preference order.
fn candidate_paths() -> Vec<&'static str> {
    vec![
        // Windows
        "C:\\Windows\\Fonts\\YuGothM.ttc",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
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

/// Install the first available Japanese font.
///
/// The Japanese font is made the *primary* proportional font (its Latin
/// glyphs are used too). When CJK glyphs come from a fallback font, egui
/// lays each glyph out with its own font's ascent, so kana/kanji sit
/// visibly above the Latin baseline and get clipped in text inputs.
/// Driving the whole row from one font's metrics fixes both.
pub fn install_japanese_fonts(ctx: &egui::Context) {
    let Some((path, bytes)) = candidate_paths()
        .into_iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (p, b)))
    else {
        eprintln!("warning: no Japanese system font found; CJK text will not render");
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("japanese".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "japanese".to_owned());
    // Monospace keeps its default first (fixed-width Latin) with Japanese
    // as fallback; the app renders no Japanese in monospace.
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("japanese".to_owned());
    ctx.set_fonts(fonts);
    eprintln!("loaded Japanese font: {path}");
}
