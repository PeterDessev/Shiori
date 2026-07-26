//! A reusable centered modal: a borderless, non-resizable window centred in
//! a given area with an even margin, a compact header (a caller-drawn title
//! plus a ✕ button), and dismissal on a click outside it or Escape. The
//! dictionary's word-detail popup is the first user; other screens (e.g. a
//! word panel in the reader) can reuse it.

use eframe::egui;

/// Show a modal centred in `area` — pass a panel's rect (not the whole
/// screen) so the modal leaves the nav rail clickable and centres within the
/// content region. `id` must be unique among visible modals.
///
/// `just_opened` suppresses click-away for the frame the modal opens, so the
/// click that opened it is not immediately read as a dismissal; callers
/// should pass a flag they set the frame they populate the modal and clear
/// here.
///
/// `header` draws the title row (laid out left-to-right, left of the ✕);
/// `body` draws the content below a separator. Returns `true` when the modal
/// should close this frame — via the ✕, a press outside the window that
/// still lands inside `area`, or Escape.
pub(crate) fn centered_modal(
    ctx: &egui::Context,
    area: egui::Rect,
    id: &str,
    just_opened: bool,
    header: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    // Centre the window in `area` with an even margin on every side, sized to
    // most of the area but not less than a usable minimum.
    let margin = 44.0;
    let size = egui::vec2(
        (area.width() - 2.0 * margin).max(240.0),
        (area.height() - 2.0 * margin).max(240.0),
    );
    // Anchor the centre to the centre of `area`: the screen centre shifted by
    // whatever offset the nav rail (and any banners) introduce.
    let anchor_offset = area.center() - ctx.screen_rect().center();

    let mut close = false;
    let win = egui::Window::new(id)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::CENTER_CENTER, anchor_offset)
        .fixed_size(size)
        .show(ctx, |ui| {
            // Compact header standing in for a title bar: caller content on
            // the left, a close button pinned to the right.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✕").on_hover_text("Close (Esc)").clicked() {
                        close = true;
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), header);
                });
            });
            ui.separator();
            body(ui);
        });

    // A press outside the window that still lands inside `area` (never the nav
    // rail, so switching tabs leaves the modal open) dismisses it.
    if !just_opened {
        if let Some(win) = &win {
            let rect = win.response.rect;
            let clicked_away = ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer
                        .press_origin()
                        .is_some_and(|p| area.contains(p) && !rect.contains(p))
            });
            if clicked_away {
                close = true;
            }
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    close
}
