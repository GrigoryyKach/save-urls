//! NIMVORO shared module: progress of a self-update in the shared egui style.
//!
//! Copied byte for byte into the egui tools. Draw it where the "Update" button lives:
//!
//! ```ignore
//! if update_ui::progress(ui, &self.updater, self.lang, p) { return; } // busy or failed: the bar replaces the button
//! ```

use crate::i18n::Lang;
use crate::nimvoro::{self, Updater};
use crate::selfupdate::Phase;
use crate::theme::{self, Btn, Pal};
use eframe::egui::{self, Align, Layout, Sense, Stroke, Ui, vec2};

/// Draws the progress bar (or the error with "Try again") while a self-update is running or has failed.
/// Returns `true` if it drew something, so the caller can skip its own "Update" button.
pub fn progress(ui: &mut Ui, updater: &Updater, lang: Lang, p: &Pal) -> bool {
    let phase = updater.phase();
    if phase == Phase::Idle {
        return false;
    }
    let t = nimvoro::strings(lang);
    let text = updater.phase_text(lang).unwrap_or_default();
    match phase {
        Phase::Failed(_) => {
            ui.add(egui::Label::new(egui::RichText::new(text).font(theme::reg(12.0)).color(p.negative)).wrap());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if theme::button(ui, t.try_again, Btn::Secondary, p).clicked() {
                    updater.dismiss_error();
                    updater.apply();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::button(ui, t.download_manually, Btn::Link, p).clicked() {
                        updater.open_release_page();
                    }
                });
            });
        }
        _ => {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            let frac = match phase {
                Phase::Downloading { done, total } if total > 0 => (done as f32 / total as f32).clamp(0.0, 1.0),
                Phase::Downloading { .. } => 0.0,
                _ => 1.0,
            };
            theme::hint(ui, &text, p);
            ui.add_space(6.0);
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
            let r = 2.0;
            ui.painter().rect_filled(rect, r, p.raised);
            ui.painter().rect_stroke(rect, r, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
            let mut fill = rect;
            fill.set_width((rect.width() * frac).max(if frac > 0.0 { 4.0 } else { 0.0 }));
            ui.painter().rect_filled(fill, r, p.accent);
        }
    }
    true
}
