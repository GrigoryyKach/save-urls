//! NIMVORO shared module: the suite's egui base. Graphite ramp (dark and light), IBM Plex type,
//! 4px spacing, hairlines instead of shadows, and the widgets built on them.
//!
//! Copied byte for byte into every Rust (egui) tool of the suite. The module knows nothing about the
//! app: the app's accent comes through [`set_accent`], which each app calls once before [`install`].
//! Fonts are `assets/fonts/{PlexSans-Regular,PlexSans-Medium,PlexMono-Regular}.ttf` (copied with the module).
//!
//! # Usage
//!
//! ```ignore
//! // Once, in the app constructor, before anything is drawn:
//! theme::set_accent(theme::Accent::new(0xec5b45, 0xc93a25, 0x141415, 0xffffff)); // dark, light, on-dark, on-light
//! theme::install(&cc.egui_ctx, cfg.theme);
//! // After the user changes the setting (System follows the OS live, nothing else to do):
//! theme::apply(ctx, mode);
//! // Every frame: pick the palette matching the active egui theme.
//! let p = Pal::of(ui.visuals().dark_mode);
//! ```
//!
//! # Rules the widgets follow
//!
//! * Only two weights: `reg` (400) and `medium` (500). `mono` is for figures, paths, versions and hotkeys.
//! * Text roles: `text`, `text2`, `muted`. One accent, solid, used for the primary action, the selection
//!   and the one figure that matters. No gradients, no glows, no coloured shadows.
//! * Depth in dark comes from surface steps (`sidebar` < `bg` < `card` < `raised` < `raised_hi`) and
//!   hairlines (`line`, `line_hi`). Light uses the same steps plus a small neutral shadow on floating things.
//! * Sizes on a 4px grid: control radius 8, card 12, window 16; rows 32 to 40; buttons 28/32/36/44.

#![allow(dead_code)] // every app uses its own subset of the helpers

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Frame, Id, Margin,
    Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextStyle, Theme, Ui, Vec2, epaint::Shadow, pos2,
    vec2,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};

/// Theme setting stored in the app config (`system`, `light`, `dark`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeMode::System => egui::ThemePreference::System,
            ThemeMode::Light => egui::ThemePreference::Light,
            ThemeMode::Dark => egui::ThemePreference::Dark,
        }
    }
}

// ───────────────────────── accent hook ─────────────────────────

/// The app's accent: a fill/text colour for each theme and the text colour that sits on that fill.
/// Light values are the darker shades, so the accent used as text on the light page keeps 4.5:1.
#[derive(Clone, Copy, Debug)]
pub struct Accent {
    pub dark: Color32,
    pub light: Color32,
    pub on_dark: Color32,
    pub on_light: Color32,
}

impl Accent {
    pub const fn new(dark: u32, light: u32, on_dark: u32, on_light: u32) -> Self {
        Self { dark: hex(dark), light: hex(light), on_dark: hex(on_dark), on_light: hex(on_light) }
    }
}

static ACCENT: OnceLock<Accent> = OnceLock::new();

/// Sets the app accent. Call once, before [`install`]; later calls are ignored.
pub fn set_accent(a: Accent) {
    let _ = ACCENT.set(a);
}

fn accent() -> Accent {
    // Fallback until the app sets its own: the NIMVORO cobalt.
    *ACCENT.get_or_init(|| Accent::new(0x7a8fff, 0x3f5bf0, 0x141415, 0xffffff))
}

// ───────────────────────── palette ─────────────────────────

#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    /// Deepest surface: side panels, title bars.
    pub sidebar: Color32,
    /// The page.
    pub bg: Color32,
    /// Cards and popups.
    pub card: Color32,
    /// Active row, secondary button, switch track.
    pub raised: Color32,
    /// One step above `raised`: hover.
    pub raised_hi: Color32,
    /// Text fields.
    pub input: Color32,
    /// Hairline: the text colour at 8%.
    pub line: Color32,
    /// Control outline and tracks: the text colour at 16%.
    pub line_hi: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub muted: Color32,
    pub accent: Color32,
    /// Text on a filled accent.
    pub on_accent: Color32,
    pub positive: Color32,
    pub negative: Color32,
    pub warning: Color32,
}

pub const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Pal {
    pub fn light() -> Self {
        let a = accent();
        Self {
            dark: false,
            sidebar: hex(0xf4f4f5),
            bg: hex(0xfafafa),
            card: hex(0xffffff),
            raised: hex(0xf4f4f5),
            raised_hi: hex(0xececee),
            input: hex(0xffffff),
            line: Color32::from_rgba_unmultiplied(23, 23, 23, 20),
            line_hi: Color32::from_rgba_unmultiplied(23, 23, 23, 41),
            text: hex(0x171717),
            text2: hex(0x525252),
            muted: hex(0x686868),
            accent: a.light,
            on_accent: a.on_light,
            positive: hex(0x16a34a),
            negative: hex(0xdc2626),
            warning: hex(0xb7791f),
        }
    }

    pub fn dark() -> Self {
        let a = accent();
        Self {
            dark: true,
            sidebar: hex(0x0f0f10),
            bg: hex(0x141415),
            card: hex(0x19191b),
            raised: hex(0x232325),
            raised_hi: hex(0x2c2c2f),
            input: hex(0x232325),
            line: Color32::from_rgba_unmultiplied(237, 237, 237, 20),
            line_hi: Color32::from_rgba_unmultiplied(237, 237, 237, 41),
            text: hex(0xededed),
            text2: hex(0xa1a1a6),
            muted: hex(0x949499),
            accent: a.dark,
            on_accent: a.on_dark,
            positive: hex(0x4cc38a),
            negative: hex(0xe5484d),
            warning: hex(0xf0b13a),
        }
    }

    pub fn of(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }

    /// Primary-button hover: the accent moved 8% towards white (dark) or black (light).
    pub fn accent_hover(&self) -> Color32 {
        mix(self.accent, if self.dark { Color32::WHITE } else { Color32::BLACK }, 0.08)
    }

    /// Soft top edge of a raised surface on dark (1px white at 12%). Fully transparent on light.
    pub fn top_light(&self) -> Color32 {
        if self.dark { Color32::from_white_alpha(31) } else { Color32::TRANSPARENT }
    }
}

/// Linear blend of two colours (premultiplied), t in [0, 1].
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (a, b) = (a.to_array(), b.to_array());
    let f = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(f(0), f(1), f(2), f(3))
}

/// The colour with its own alpha replaced (0..=255).
pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Relative luminance (WCAG) of an opaque colour.
fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

/// WCAG contrast ratio of two opaque colours (1.0 to 21.0).
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Contrast problems of the app accent in both themes (empty when fine). Apps assert this in a test:
/// the accent as text on every surface and `on_accent` on the filled accent must reach 4.5:1.
pub fn accent_problems() -> Vec<String> {
    let mut out = Vec::new();
    for p in [Pal::dark(), Pal::light()] {
        let theme = if p.dark { "dark" } else { "light" };
        for (name, s) in [("sidebar", p.sidebar), ("bg", p.bg), ("card", p.card)] {
            let c = contrast(p.accent, s);
            if c < 4.5 {
                out.push(format!("{theme}: accent text on {name} is {c:.2}"));
            }
        }
        let c = contrast(p.on_accent, p.accent);
        if c < 4.5 {
            out.push(format!("{theme}: on_accent on accent is {c:.2}"));
        }
    }
    out
}

// ───────────────────────── fonts ─────────────────────────

/// Interface text, regular (400).
pub fn reg(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
/// Interface text, medium (500): emphasis, buttons, titles. There is no bold.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}
/// IBM Plex Mono: figures, versions, paths, timestamps, hotkeys.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// IBM Plex (Latin and Cyrillic subset) in front, egui's own fonts behind it, and on Windows
/// Segoe UI Symbol/Emoji, so a glyph outside the subset does not turn into a square.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |key: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(key.into(), Arc::new(FontData::from_static(bytes)));
    };
    add("plex-sans", include_bytes!("../assets/fonts/PlexSans-Regular.ttf"));
    add("plex-sans-medium", include_bytes!("../assets/fonts/PlexSans-Medium.ttf"));
    add("plex-mono", include_bytes!("../assets/fonts/PlexMono-Regular.ttf"));

    #[cfg(windows)]
    let dir = std::path::Path::new(r"C:\Windows\Fonts");
    #[cfg(not(windows))]
    let dir = std::path::Path::new("/usr/share/fonts");
    let mut load = |key: &str, file: &str| -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    #[cfg(windows)]
    let (has_sym, has_emj) = (load("segsym", "seguisym.ttf"), load("segemj", "seguiemj.ttf"));
    // Linux: system fonts with wide symbol coverage are the fallback.
    #[cfg(not(windows))]
    let (has_sym, has_emj) = {
        let mut try_load = |key: &str, name: &str| find_font(dir, name, 4).is_some_and(|p| load(key, &p.to_string_lossy()));
        (try_load("segsym", "NotoSansSymbols2-Regular.ttf"), try_load("segemj", "DejaVuSans.ttf"))
    };

    let tail = |mut base: Vec<String>| {
        if has_sym {
            base.push("segsym".into());
        }
        if has_emj {
            base.push("segemj".into());
        }
        base
    };
    let prop_base = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_base = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let with = |front: &[&str], base: &[String]| {
        let mut v: Vec<String> = front.iter().map(|s| s.to_string()).collect();
        v.extend(base.iter().cloned());
        tail(v)
    };
    let prop = with(&["plex-sans"], &prop_base);
    let med = with(&["plex-sans-medium", "plex-sans"], &prop_base);
    let mono = with(&["plex-mono"], &mono_base);
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("medium".into()), med);
    fonts.families.insert(FontFamily::Monospace, mono);
    ctx.set_fonts(fonts);
}

/// Looks for a font file by name under `dir` (Linux: `/usr/share/fonts`, a few levels deep).
#[cfg(not(windows))]
fn find_font(dir: &std::path::Path, name: &str, depth: u8) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                if let Some(found) = find_font(&path, name, depth - 1) {
                    return Some(found);
                }
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

// ───────────────────────── visuals ─────────────────────────

/// Control radius.
pub const R_CONTROL: u8 = 8;
/// Card radius.
pub const R_CARD: u8 = 12;
/// Window and dialog radius.
pub const R_WINDOW: u8 = 16;

fn visuals(p: &Pal) -> egui::Visuals {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(R_CONTROL);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.line_hi);
    v.window_corner_radius = CornerRadius::same(R_WINDOW);
    v.menu_corner_radius = CornerRadius::same(R_CONTROL);
    v.extreme_bg_color = p.input;
    v.text_edit_bg_color = Some(p.input);
    v.faint_bg_color = p.card;
    v.hyperlink_color = p.accent;
    // Text selection and the picked item in a menu are a neutral step, never an accent wash.
    v.selection.bg_fill = mix(p.input, p.text, if p.dark { 0.18 } else { 0.14 });
    v.selection.stroke = Stroke::new(2.0, p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);
    // Depth: dark gets none (surface steps and hairlines do it); light gets a small neutral shadow.
    let (small, floating) = if p.dark {
        (Shadow::NONE, Shadow::NONE)
    } else {
        (
            Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(15) },
            Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(15) },
        )
    };
    v.window_shadow = floating;
    v.popup_shadow = small;
    let w = &mut v.widgets;
    for (wv, fill, stroke, text) in [
        (&mut w.noninteractive, p.card, p.line, p.text),
        (&mut w.inactive, p.raised, p.line_hi, p.text),
        (&mut w.hovered, p.raised_hi, p.line_hi, p.text),
        (&mut w.active, p.raised_hi, p.line_hi, p.text),
        (&mut w.open, p.raised_hi, p.line_hi, p.text),
    ] {
        wv.corner_radius = r;
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.bg_stroke = Stroke::new(1.0, stroke);
        wv.fg_stroke = Stroke::new(1.0, text);
        wv.expansion = 0.0;
    }
    v
}

/// Installs fonts, both palettes and the base spacing, then applies `mode`.
pub fn install(ctx: &egui::Context, mode: ThemeMode) {
    install_fonts(ctx);
    apply(ctx, mode);
    ctx.set_visuals_of(Theme::Light, visuals(&Pal::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&Pal::dark()));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 8.0);
        s.spacing.interact_size.y = 32.0;
        s.text_styles.insert(TextStyle::Body, reg(14.0));
        s.text_styles.insert(TextStyle::Button, reg(14.0));
        s.text_styles.insert(TextStyle::Small, reg(12.0));
        s.text_styles.insert(TextStyle::Monospace, mono(13.0));
        s.text_styles.insert(TextStyle::Heading, medium(24.0));
        s.interaction.selectable_labels = false;
    });
}

/// Switches the theme at runtime. `System` follows the OS setting live.
pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    #[cfg(not(windows))]
    if mode == ThemeMode::System && ctx.system_theme().is_none() {
        // X11 has no system theme event: ask the desktop settings, fall back to dark.
        ctx.set_theme(if linux_prefers_dark() { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
        return;
    }
    ctx.set_theme(mode.preference());
}

/// Best effort dark-mode detection on Linux: GNOME/portal `color-scheme`, then the GTK theme name. Dark when unknown.
#[cfg(not(windows))]
fn linux_prefers_dark() -> bool {
    let run = |prog: &str, args: &[&str]| {
        std::process::Command::new(prog)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
    };
    if let Some(v) = run("gsettings", &["get", "org.gnome.desktop.interface", "color-scheme"]) {
        if v.contains("prefer-dark") {
            return true;
        }
        if v.contains("prefer-light") || v.contains("default") {
            return false;
        }
    }
    if let Some(v) = run("gsettings", &["get", "org.gnome.desktop.interface", "gtk-theme"]) {
        return v.contains("dark");
    }
    true
}

// ───────────────────────── motion ─────────────────────────

/// The suite's one easing curve (close to cubic-bezier(0.22, 1, 0.36, 1)).
pub fn ease(t: f32) -> f32 {
    egui::emath::easing::cubic_out(t)
}

/// 0..1 follower of `on`: 120 ms for hover, press and toggle feedback.
pub fn fade(ui: &Ui, id: Id, on: bool) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(id, on, 0.12, ease)
}

/// Same, with an explicit duration (menus 0.18, panels 0.3).
pub fn fade_for(ui: &Ui, id: Id, on: bool, secs: f32) -> f32 {
    ui.ctx().animate_bool_with_time_and_easing(id, on, secs, ease)
}

// ───────────────────────── surfaces and text ─────────────────────────

/// A card: the card surface, a hairline, radius 12, padding 16.
pub fn card(p: &Pal) -> Frame {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(R_CARD)
        .inner_margin(Margin::same(16))
}

/// The frame of a text field drawn by hand (the participants box): input fill, outline, control radius.
pub fn field_frame(p: &Pal) -> Frame {
    Frame::new().fill(p.input).stroke(Stroke::new(1.0, p.line_hi)).corner_radius(R_CONTROL)
}

/// Section title: sentence case, 13px medium, secondary colour. Replaces the old spaced capitals.
pub fn section(ui: &mut Ui, text: &str, p: &Pal) -> Response {
    ui.label(egui::RichText::new(text).font(medium(13.0)).color(p.text2))
}

/// Caption and hint: 12px muted, wraps.
pub fn hint(ui: &mut Ui, text: &str, p: &Pal) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(reg(12.0)).color(p.muted)).wrap())
}

/// Count, value or version: 12px mono, muted. Replaces the pills.
pub fn meta(ui: &mut Ui, text: &str, p: &Pal) -> Response {
    ui.label(egui::RichText::new(text).font(mono(12.0)).color(p.muted))
}

/// One pixel line across the available width, with no space around it.
pub fn hairline(ui: &mut Ui, p: &Pal) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.line));
}

/// Hairline with 16px above and below: separates groups in a settings panel.
pub fn divider(ui: &mut Ui, p: &Pal) {
    ui.add_space(8.0);
    hairline(ui, p);
    ui.add_space(8.0);
}

/// Neutral drop shadow for a floating surface: none on dark, soft on light.
pub fn shadow_for(p: &Pal) -> Shadow {
    if p.dark {
        Shadow::NONE
    } else {
        Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(15) }
    }
}

// ───────────────────────── buttons ─────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    /// The one action of a screen: solid accent, 36 high.
    Primary,
    /// Raised fill and an outline (Browse, Undo, Support).
    Secondary,
    /// No fill, no border; the fill appears on hover.
    Quiet,
    /// Small, inside an input row (Clear, All, None): 28 high, radius 6.
    Small,
    /// Accent text; underline on hover.
    Link,
}

impl Btn {
    pub fn height(self) -> f32 {
        match self {
            Btn::Primary => 36.0,
            Btn::Secondary | Btn::Quiet => 32.0,
            Btn::Small => 28.0,
            Btn::Link => 20.0,
        }
    }
    fn font(self) -> FontId {
        match self {
            Btn::Small => medium(12.0),
            Btn::Link => reg(13.0),
            _ => medium(14.0),
        }
    }
    fn pad_x(self) -> f32 {
        match self {
            Btn::Primary => 16.0,
            Btn::Secondary | Btn::Quiet => 12.0,
            Btn::Small => 8.0,
            Btn::Link => 0.0,
        }
    }
}

/// A button in a given rect. The rect's height decides; use [`Btn::height`] for the standard one.
pub fn button_at(ui: &mut Ui, rect: Rect, id: Id, label: &str, kind: Btn, p: &Pal) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let t = fade(ui, id.with("hover"), resp.hovered());
    let pressed = resp.is_pointer_button_down_on() && kind != Btn::Link;
    let r = if pressed { Rect::from_center_size(rect.center(), rect.size() * 0.98) } else { rect };
    let (fill, stroke, col) = match kind {
        Btn::Primary => (mix(p.accent, p.accent_hover(), t), Stroke::NONE, p.on_accent),
        Btn::Secondary => (mix(p.raised, p.raised_hi, t), Stroke::new(1.0, p.line_hi), p.text),
        Btn::Quiet => (alpha(p.raised, (t * 255.0) as u8), Stroke::NONE, mix(p.text2, p.text, t)),
        Btn::Small => (mix(p.raised, p.raised_hi, t), Stroke::NONE, mix(p.text2, p.text, t)),
        Btn::Link => (Color32::TRANSPARENT, Stroke::NONE, mix(p.accent, p.text, t * 0.25)),
    };
    let radius = if kind == Btn::Small { 6.0 } else { R_CONTROL as f32 };
    let painter = ui.painter();
    painter.rect(r, radius, fill, stroke, StrokeKind::Inside);
    if kind == Btn::Primary {
        painter.hline(r.x_range().shrink(radius), r.top() + 0.5, Stroke::new(1.0, p.top_light()));
    }
    let font = kind.font();
    if kind == Btn::Link {
        let g = painter.layout_no_wrap(label.to_string(), font, col);
        let pos = r.left_center() - vec2(0.0, g.size().y / 2.0);
        if resp.hovered() {
            painter.hline(pos.x..=pos.x + g.size().x, pos.y + g.size().y - 1.0, Stroke::new(1.0, col));
        }
        painter.galley(pos, g, col);
    } else {
        painter.text(r.center(), Align2::CENTER_CENTER, label, font, col);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Button that sizes itself to its text, at the standard height of its kind.
pub fn button(ui: &mut Ui, label: &str, kind: Btn, p: &Pal) -> Response {
    let w = ui.painter().layout_no_wrap(label.to_string(), kind.font(), p.text).size().x;
    let (rect, _) = ui.allocate_exact_size(vec2(w + kind.pad_x() * 2.0, kind.height()), Sense::hover());
    button_at(ui, rect, ui.scope_id().with(("btn", label)), label, kind, p)
}

/// The hero action (Spin, Roll, New task): solid accent, in a rect of the caller's size (44 high).
/// Disabled: raised fill, muted text, no hover.
#[allow(clippy::too_many_arguments)]
pub fn primary_button(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    label: &str,
    icon: Option<fn(&Painter, Pos2, Color32)>,
    enabled: bool,
    p: &Pal,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    let t = fade(ui, id.with("hover"), enabled && resp.hovered());
    let pressed = enabled && resp.is_pointer_button_down_on();
    let r = if pressed { Rect::from_center_size(rect.center(), rect.size() * 0.98) } else { rect };
    let radius = R_CONTROL as f32;
    let (fill, stroke, col) = if enabled {
        (mix(p.accent, p.accent_hover(), t), Stroke::NONE, p.on_accent)
    } else {
        (p.raised, Stroke::new(1.0, p.line), p.muted)
    };
    let painter = ui.painter();
    painter.rect(r, radius, fill, stroke, StrokeKind::Inside);
    if enabled {
        painter.hline(r.x_range().shrink(radius), r.top() + 0.5, Stroke::new(1.0, p.top_light()));
    }
    let galley = painter.layout_no_wrap(label.to_string(), medium(16.0), col);
    let icon_w = if icon.is_some() { 24.0 } else { 0.0 };
    let x0 = r.center().x - (galley.size().x + icon_w) / 2.0;
    if let Some(draw) = icon {
        draw(painter, pos2(x0 + 8.0, r.center().y), col);
    }
    painter.galley(pos2(x0 + icon_w, r.center().y - galley.size().y / 2.0), galley, col);
    if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

// ───────────────────────── switch, slider, segmented ─────────────────────────

/// 40×22 switch: off is a neutral track, on is the accent; the knob is white, 18px, flat.
pub fn switch(ui: &mut Ui, on: &mut bool, p: &Pal) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time_and_easing(resp.id, *on, 0.15, ease);
    let painter = ui.painter();
    painter.rect_filled(rect, 11.0, mix(p.line_hi, p.accent, t));
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    let c = pos2(x, rect.center().y);
    painter.circle_filled(c, 9.0, Color32::WHITE);
    if !p.dark {
        painter.circle_stroke(c, 9.0, Stroke::new(1.0, p.line));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Slider: 4px track, accent fill, 16px white knob with a hairline.
pub fn slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, step: f32, p: &Pal) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 16.0, 4.0));
    if let Some(pos) = resp.interact_pointer_pos() {
        let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let v = ((min + t * (max - min)) / step).round() * step;
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let t = (*value - min) / (max - min);
    let x = track.left() + t * track.width();
    let painter = ui.painter();
    painter.rect_filled(track, 2.0, p.line_hi);
    painter.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), 2.0, p.accent);
    let c = pos2(x, track.center().y);
    painter.circle_filled(c, 8.0, Color32::WHITE);
    painter.circle_stroke(c, 8.0, Stroke::new(1.0, if p.dark { p.line } else { p.line_hi }));
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// One option of a [`segmented`] control.
pub struct Seg<'a> {
    pub label: &'a str,
    pub icon: Option<fn(&Painter, Pos2, Color32)>,
}

fn seg_widths(ui: &Ui, items: &[Seg], p: &Pal) -> Vec<f32> {
    items
        .iter()
        .map(|s| {
            let tw = ui.painter().layout_no_wrap(s.label.to_string(), medium(14.0), p.text).size().x;
            tw + 24.0 + if s.icon.is_some() { 24.0 } else { 0.0 }
        })
        .collect()
}

/// Width of a [`segmented`] control, for centring it next to other things.
pub fn segmented_width(ui: &Ui, items: &[Seg], p: &Pal) -> f32 {
    seg_widths(ui, items, p).iter().sum::<f32>() + 8.0
}

/// Segmented control, 40 high: a card-coloured track (radius 12) with a raised active segment (radius 8).
/// Returns the index the user clicked when it differs from `selected`.
pub fn segmented(ui: &mut Ui, items: &[Seg], selected: usize, p: &Pal) -> Option<usize> {
    let font = medium(14.0);
    let widths = seg_widths(ui, items, p);
    let total = widths.iter().sum::<f32>() + 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 40.0), Sense::hover());
    ui.painter().rect(rect, 12.0, p.card, Stroke::new(1.0, p.line), StrokeKind::Inside);
    let mut clicked = None;
    let mut x = rect.left() + 4.0;
    for (i, (s, w)) in items.iter().zip(widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(w, 32.0));
        let id = ui.scope_id().with(("seg", s.label));
        let resp = ui.interact(r, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let active = i == selected;
        let t = fade(ui, id.with("hover"), resp.hovered() && !active);
        let painter = ui.painter();
        if active {
            painter.rect(r, 8.0, p.raised, Stroke::new(1.0, p.line), StrokeKind::Inside);
            painter.hline(r.x_range().shrink(8.0), r.top() + 0.5, Stroke::new(1.0, p.top_light()));
        } else if t > 0.0 {
            painter.rect_filled(r, 8.0, alpha(p.raised, (t * 160.0) as u8));
        }
        let col = if active { p.text } else { mix(p.text2, p.text, t) };
        let mut tx = r.left() + 12.0;
        if let Some(icon) = s.icon {
            icon(painter, pos2(tx + 8.0, r.center().y), col);
            tx += 24.0;
        }
        painter.text(pos2(tx, r.center().y), Align2::LEFT_CENTER, s.label, font.clone(), col);
        if resp.clicked() && !active {
            clicked = Some(i);
        }
        x += w;
    }
    clicked
}

/// A key cap: raised fill, hairline, a slightly darker bottom edge, mono 12. For hotkeys.
pub fn keycap(ui: &mut Ui, text: &str, p: &Pal) {
    let g = ui.painter().layout_no_wrap(text.to_string(), mono(12.0), p.text2);
    let size = vec2((g.size().x + 12.0).max(24.0), 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, 4.0, p.raised, Stroke::new(1.0, p.line_hi), StrokeKind::Inside);
    painter.hline(rect.x_range().shrink(4.0), rect.bottom() - 1.5, Stroke::new(1.0, p.line));
    painter.galley(rect.center() - g.size() / 2.0 - vec2(0.0, 1.0), g, p.text2);
}

/// A label with a chevron that opens and closes a block. Returns the click response.
pub fn disclosure(ui: &mut Ui, label: &str, open: bool, p: &Pal) -> Response {
    let g = ui.painter().layout_no_wrap(label.to_string(), medium(13.0), p.text2);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 24.0), Sense::click());
    let t = fade(ui, resp.id.with("hover"), resp.hovered());
    let col = mix(p.text2, p.text, t);
    let painter = ui.painter();
    icon_chevron(painter, pos2(rect.left() + 8.0, rect.center().y), col, open);
    painter.galley(pos2(rect.left() + 24.0, rect.center().y - g.size().y / 2.0), g, col);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// The app's flat mark: its own glyph in the accent colour, no tile, no shadow.
pub fn mark(ui: &mut Ui, size: f32, glyph: fn(&Painter, Pos2, Color32), p: &Pal) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    glyph(ui.painter(), rect.center(), p.accent);
}

// ───────────────────────── icons ─────────────────────────
// 16px box, 1.5px stroke, drawn around `c`. They take the colour of their text role.

const STROKE: f32 = 1.5;

pub fn icon_list(painter: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(STROKE, col);
    for dy in [-4.5, 0.0, 4.5] {
        painter.line_segment([c + vec2(-6.0, dy), c + vec2(6.0, dy)], s);
    }
}

pub fn icon_wheel(painter: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(STROKE, col);
    painter.circle_stroke(c, 6.8, s);
    painter.line_segment([c, c + vec2(0.0, -6.8)], s);
    painter.line_segment([c, c + vec2(5.9, 3.4)], s);
    painter.line_segment([c, c + vec2(-5.9, 3.4)], s);
}

pub fn icon_fullscreen(painter: &Painter, c: Pos2, col: Color32, exit: bool) {
    let s = Stroke::new(STROKE, col);
    let (a, b) = (6.5, 3.0);
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = if exit { c + vec2(sx * b, sy * b) } else { c + vec2(sx * a, sy * a) };
        let dir = if exit { 1.0 } else { -1.0 };
        let len = a - b;
        painter.line_segment([corner, corner + vec2(sx * dir * len, 0.0)], s);
        painter.line_segment([corner, corner + vec2(0.0, sy * dir * len)], s);
    }
}

pub fn icon_spin(painter: &Painter, c: Pos2, col: Color32) {
    let r = 6.5;
    let (a0, a1) = (-1.0f32, 4.3f32);
    let pts: Vec<Pos2> = (0..=20)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / 20.0;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    painter.add(Shape::line(pts, Stroke::new(STROKE, col)));
    // Arrow head at the end of the arc, clockwise.
    let end = c + vec2(a1.cos(), a1.sin()) * r;
    let fwd = vec2(-a1.sin(), a1.cos());
    let normal = vec2(a1.cos(), a1.sin());
    painter.add(Shape::convex_polygon(
        vec![end + normal * 3.6, end + fwd * 4.2, end - normal * 3.6],
        col,
        Stroke::NONE,
    ));
}

pub fn icon_check(painter: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(STROKE, col);
    painter.line_segment([c + vec2(-4.5, 0.5), c + vec2(-1.5, 3.5)], s);
    painter.line_segment([c + vec2(-1.5, 3.5), c + vec2(4.5, -3.0)], s);
}

/// Chevron: pointing right when closed, down when open.
pub fn icon_chevron(painter: &Painter, c: Pos2, col: Color32, open: bool) {
    let s = Stroke::new(STROKE, col);
    if open {
        painter.line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], s);
        painter.line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], s);
    } else {
        painter.line_segment([c + vec2(-1.5, -3.5), c + vec2(2.0, 0.0)], s);
        painter.line_segment([c + vec2(2.0, 0.0), c + vec2(-1.5, 3.5)], s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roles_reach_4_5_on_every_surface() {
        for p in [Pal::dark(), Pal::light()] {
            for (sn, s) in [("sidebar", p.sidebar), ("bg", p.bg), ("card", p.card), ("raised", p.raised), ("raised_hi", p.raised_hi)] {
                for (tn, t) in [("text", p.text), ("text2", p.text2), ("muted", p.muted)] {
                    let c = contrast(t, s);
                    assert!(c >= 4.5, "dark={} {tn} on {sn} is {c:.2}", p.dark);
                }
            }
        }
    }

    #[test]
    fn default_accent_is_readable() {
        assert!(accent_problems().is_empty(), "{:?}", accent_problems());
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
    }
}
