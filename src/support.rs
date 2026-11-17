//! NIMVORO shared module: the "Support the author" block.
//!
//! Copied byte for byte into every Rust (egui) tool of the suite. The strings live in this file
//! (three languages), the QR code is `assets/support-qr.png` embedded in the exe.
//!
//! ```ignore
//! mod support;
//! // In the settings page, near the bottom, above "About":
//! support::support_section(ui, lang);
//! // In a tray menu:
//! support::tray_label(lang);   // "Support the author ♥"
//! support::open();             // opens the link in the default browser
//! ```

#![allow(dead_code)] // tray_label() is only used by apps with a tray

use crate::i18n::Lang;
use crate::theme::{self, Pal};
use eframe::egui::{self, Align2, Color32, ColorImage, CursorIcon, Id, Sense, TextureHandle, TextureOptions, Ui, vec2};
use std::os::windows::process::CommandExt;
use std::process::Command;

/// The donation page. The only place where the link is written.
pub const URL: &str = "https://donatello.to/MicsGame";

const QR_PNG: &[u8] = include_bytes!("../assets/support-qr.png");
/// Logical size of the QR code on screen.
const QR_SIZE: f32 = 136.0;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Strings {
    title: &'static str,
    text: &'static str,
    button: &'static str,
    scan: &'static str,
    tray: &'static str,
}

const EN: Strings = Strings {
    title: "Support the author",
    text: "If you find this app useful, you can support its development.",
    button: "Support on Donatello",
    scan: "Scan with your phone",
    tray: "Support the author ♥",
};
const RU: Strings = Strings {
    title: "Поддержать автора",
    text: "Если программа вам полезна, вы можете поддержать её разработку.",
    button: "Поддержать на Donatello",
    scan: "Отсканируйте телефоном",
    tray: "Поддержать автора ♥",
};
const UK: Strings = Strings {
    title: "Підтримати автора",
    text: "Якщо програма вам корисна, ви можете підтримати її розробку.",
    button: "Підтримати на Donatello",
    scan: "Відскануйте телефоном",
    tray: "Підтримати автора ♥",
};

fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Ru => &RU,
        Lang::Uk => &UK,
    }
}

/// Tray menu label.
pub fn tray_label(lang: Lang) -> &'static str {
    strings(lang).tray
}

/// Opens the donation page in the default browser.
pub fn open() {
    let _ = Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", URL]).creation_flags(CREATE_NO_WINDOW).spawn();
}

/// Box-filter resample of an RGBA image to `n`×`n` (keeps the modules crisp, unlike GPU scaling).
fn resample(src: &image_data::Rgba, n: usize) -> ColorImage {
    let mut px = Vec::with_capacity(n * n);
    let scale = src.w as f32 / n as f32;
    for y in 0..n {
        for x in 0..n {
            let (x0, x1) = (x as f32 * scale, (x + 1) as f32 * scale);
            let (y0, y1) = (y as f32 * scale, (y + 1) as f32 * scale);
            let (mut acc, mut wsum) = ([0.0f32; 3], 0.0f32);
            let mut sy = y0.floor() as usize;
            while (sy as f32) < y1 && sy < src.h {
                let wy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                let mut sx = x0.floor() as usize;
                while (sx as f32) < x1 && sx < src.w {
                    let wx = (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                    let i = (sy * src.w + sx) * 4;
                    let a = src.data[i + 3] as f32 / 255.0;
                    // Composite on white: the QR sits on a white card anyway.
                    for c in 0..3 {
                        acc[c] += (src.data[i + c] as f32 * a + 255.0 * (1.0 - a)) * wx * wy;
                    }
                    wsum += wx * wy;
                    sx += 1;
                }
                sy += 1;
            }
            let w = wsum.max(1e-6);
            px.push(Color32::from_rgb((acc[0] / w).round() as u8, (acc[1] / w).round() as u8, (acc[2] / w).round() as u8));
        }
    }
    ColorImage { size: [n, n], source_size: vec2(n as f32, n as f32), pixels: px }
}

mod image_data {
    pub struct Rgba {
        pub w: usize,
        pub h: usize,
        pub data: Vec<u8>,
    }
}

/// The QR texture, built once per pixel density (so one texel is one physical pixel).
fn qr_texture(ui: &Ui) -> Option<TextureHandle> {
    let ctx = ui.ctx();
    let ppp = ctx.pixels_per_point();
    let n = (QR_SIZE * ppp).round().max(16.0) as usize;
    let id = Id::NULL.with("nimvoro-support-qr");
    if let Some((cached_n, tex)) = ctx.data(|d| d.get_temp::<(usize, TextureHandle)>(id)) {
        if cached_n == n {
            return Some(tex);
        }
    }
    let icon = eframe::icon_data::from_png_bytes(QR_PNG).ok()?;
    let src = image_data::Rgba { w: icon.width as usize, h: icon.height as usize, data: icon.rgba };
    let tex = ctx.load_texture("support-qr", resample(&src, n), TextureOptions::NEAREST);
    ctx.data_mut(|d| d.insert_temp(id, (n, tex.clone())));
    Some(tex)
}

/// Title, text, the accent button and the QR code with its caption.
/// The caller supplies the surroundings (divider or card).
pub fn support_section(ui: &mut Ui, lang: Lang) {
    let p = Pal::of(ui.visuals().dark_mode);
    theme::caps(ui, strings(lang).title, &p);
    ui.add_space(2.0);
    support_body(ui, lang);
}

/// Section title, for apps whose settings put their own heading above a card.
pub fn title(lang: Lang) -> &'static str {
    strings(lang).title
}

/// Everything except the title: text, button and QR code.
pub fn support_body(ui: &mut Ui, lang: Lang) {
    let s = strings(lang);
    let p = Pal::of(ui.visuals().dark_mode);
    theme::hint(ui, s.text, &p);
    ui.add_space(8.0);
    let font = theme::semi(13.5);
    let btn_w = ui.painter().layout_no_wrap(s.button.to_string(), font, Color32::WHITE).size().x + 32.0;
    let qr_w = QR_SIZE + 14.0;
    // Side by side when there is room, stacked in a narrow side panel.
    if ui.available_width() >= btn_w + 14.0 + qr_w {
        ui.horizontal_top(|ui| {
            accent_button(ui, s.button, &p);
            ui.add_space(14.0);
            qr_block(ui, s.scan, &p);
        });
    } else {
        accent_button(ui, s.button, &p);
        ui.add_space(10.0);
        qr_block(ui, s.scan, &p);
    }
    ui.add_space(4.0);
}

/// Accent button (brand blue), sized to its text. Opens the donation page.
fn accent_button(ui: &mut Ui, label: &str, p: &Pal) {
    let font = theme::semi(13.5);
    let w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE).size().x;
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 32.0, 34.0), Sense::click());
    let fill = if resp.hovered() { theme::mix(p.accent, Color32::WHITE, 0.12) } else { p.accent };
    ui.painter().rect_filled(rect, 10.0, fill);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, Color32::WHITE);
    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        open();
    }
}

/// The QR code on a white rounded card (so it scans in dark mode too) with its caption.
fn qr_block(ui: &mut Ui, caption: &str, p: &Pal) {
    let Some(tex) = qr_texture(ui) else { return };
    ui.vertical(|ui| {
        let pad = 7.0;
        let (rect, _) = ui.allocate_exact_size(vec2(QR_SIZE + pad * 2.0, QR_SIZE + pad * 2.0), Sense::hover());
        ui.painter().rect_filled(rect, 10.0, Color32::WHITE);
        // Snap to whole physical pixels so no texel is stretched.
        let ppp = ui.ctx().pixels_per_point();
        let min = (rect.min + vec2(pad, pad)) * ppp;
        let min = egui::pos2(min.x.round(), min.y.round()) / ppp;
        let img = egui::Rect::from_min_size(min, vec2(QR_SIZE, QR_SIZE));
        ui.painter().image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        ui.add_space(2.0);
        ui.allocate_ui(vec2(QR_SIZE + pad * 2.0, 16.0), |ui| {
            ui.centered_and_justified(|ui| theme::hint(ui, caption, p));
        });
    });
}
