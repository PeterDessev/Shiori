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

/// Install the first available Japanese font as a fallback for both
/// proportional and monospace families. Logs to stderr and continues if
/// none is found (the UI still works, kanji will show as boxes).
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
