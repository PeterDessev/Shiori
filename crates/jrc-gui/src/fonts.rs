//! Load a Japanese-capable font from the operating system.
//!
//! egui's bundled fonts have no CJK coverage, so without this every kanji
//! renders as a placeholder box. Nothing is bundled; we pick up whatever
//! the OS provides.
//!
//! Latin text keeps egui's own (crisp) default font. The Japanese font is
//! a *fallback* with a vertical-offset tweak: fallback glyphs are laid out
//! with their own font's ascent, and Japanese fonts — Yu Gothic
//! notoriously so — carry large ascents that make kana/kanji float above
//! the Latin baseline and clip in text inputs. The tweak pushes them back
//! down onto the baseline. Meiryo is preferred over Yu Gothic because its
//! vertical metrics are far better behaved.

use std::sync::Arc;

use eframe::egui;

/// Downward shift applied to the Japanese fallback font, as a fraction of
/// the font size. Compensates for the oversized ascent of Japanese system
/// fonts relative to egui's Latin font.
const JP_Y_OFFSET_FACTOR: f32 = 0.09;

/// Common Japanese font locations, in preference order.
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

/// Install the first available Japanese font as a tweaked fallback for
/// both proportional and monospace text.
pub fn install_japanese_fonts(ctx: &egui::Context) {
    let Some((path, bytes)) = candidate_paths()
        .into_iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (p, b)))
    else {
        eprintln!("warning: no Japanese system font found; CJK text will not render");
        return;
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
    ctx.set_fonts(fonts);
    eprintln!("loaded Japanese font: {path}");
}
