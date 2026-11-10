//! Палитра и компоненты в стиле Smart Switcher (src/settings/styles.css):
//! те же цвета, радиусы, переключатели, кнопки и заголовки секций.

use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Id,
    Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextStyle, TextureHandle, TextureOptions, Theme,
    Ui, Vec2, epaint::RectShape, epaint::Shadow, pos2, text::LayoutJob, text::TextFormat, vec2,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub accent_soft: Color32,
    pub hover: Color32,
    pub track: Color32,
}

pub const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Pal {
    pub fn light() -> Self {
        Self {
            dark: false,
            bg: hex(0xf4f5f8),
            sidebar: hex(0xeceef3),
            card: hex(0xffffff),
            text: hex(0x1b1f27),
            muted: hex(0x6b7280),
            line: hex(0xe4e6ec),
            accent: hex(0x4f6bff),
            accent2: hex(0x8a5bff),
            accent_soft: Color32::from_rgba_unmultiplied(79, 107, 255, 31),
            hover: Color32::from_rgba_unmultiplied(127, 127, 127, 26),
            track: Color32::from_rgba_unmultiplied(127, 127, 127, 89),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: hex(0x14161b),
            sidebar: hex(0x191c22),
            card: hex(0x1f232b),
            text: hex(0xe8eaef),
            muted: hex(0x9198a5),
            line: hex(0x2b303a),
            accent: hex(0x6f87ff),
            accent2: hex(0x8a5bff),
            accent_soft: Color32::from_rgba_unmultiplied(111, 135, 255, 41),
            hover: Color32::from_rgba_unmultiplied(127, 127, 127, 26),
            track: Color32::from_rgba_unmultiplied(127, 127, 127, 89),
        }
    }

    pub fn of(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }
}

/// Линейное смешивание двух цветов (premultiplied), t ∈ [0, 1].
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (a, b) = (a.to_array(), b.to_array());
    let f = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(f(0), f(1), f(2), f(3))
}

// ───────────────────────── шрифты ─────────────────────────

pub fn reg(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semi(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

/// Segoe UI (обычный, полужирный, жирный), как в Smart Switcher,
/// плюс Segoe UI Symbol/Emoji, чтобы значки не превращались в квадраты.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::path::Path::new(r"C:\Windows\Fonts");
    let mut load = |key: &str, file: &str| -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    let has_reg = load("segoe", "segoeui.ttf");
    let has_sb = load("segoe-sb", "seguisb.ttf");
    let has_b = load("segoe-b", "segoeuib.ttf");
    let has_sym = load("segsym", "seguisym.ttf");
    let has_emj = load("segemj", "seguiemj.ttf");

    let base = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with_front = |front: Option<&str>| {
        let mut v = base.clone();
        if let Some(f) = front {
            v.insert(0, f.to_string());
        }
        if has_sym {
            v.push("segsym".into());
        }
        if has_emj {
            v.push("segemj".into());
        }
        v
    };
    let regular = has_reg.then_some("segoe");
    let prop = with_front(regular);
    let sb = with_front(if has_sb { Some("segoe-sb") } else { regular });
    let b = with_front(if has_b { Some("segoe-b") } else { regular });
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("semibold".into()), sb);
    fonts.families.insert(FontFamily::Name("bold".into()), b);
    ctx.set_fonts(fonts);
}

fn visuals(p: &Pal) -> egui::Visuals {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(9);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(16);
    v.extreme_bg_color = p.bg;
    v.text_edit_bg_color = Some(p.bg);
    v.faint_bg_color = p.sidebar;
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = mix(p.bg, p.accent, 0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);
    let w = &mut v.widgets;
    for (wv, stroke) in [
        (&mut w.noninteractive, p.line),
        (&mut w.inactive, p.line),
        (&mut w.hovered, mix(p.line, p.muted, 0.5)),
        (&mut w.active, p.accent),
        (&mut w.open, p.accent),
    ] {
        wv.corner_radius = r;
        wv.bg_fill = p.card;
        wv.weak_bg_fill = p.card;
        wv.bg_stroke = Stroke::new(1.0, stroke);
        wv.fg_stroke = Stroke::new(1.0, p.text);
        wv.expansion = 0.0;
    }
    v
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::ThemePreference::System);
    ctx.set_visuals_of(Theme::Light, visuals(&Pal::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&Pal::dark()));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.text_styles.insert(TextStyle::Body, reg(14.0));
        s.text_styles.insert(TextStyle::Button, reg(14.0));
        s.text_styles.insert(TextStyle::Small, reg(12.0));
        s.text_styles.insert(TextStyle::Heading, bold(24.0));
        s.interaction.selectable_labels = false;
    });
}

// ───────────────────────── градиенты ─────────────────────────

/// Текстуры-градиенты: фирменный (#4f6bff → #8a5bff, как .hero) и серый (как .hero.off).
pub struct Textures {
    pub accent: TextureHandle,
    pub grey: TextureHandle,
}

impl Textures {
    pub fn new(ctx: &egui::Context) -> Self {
        let grad = |name: &str, a: Color32, b: Color32| {
            let w = 128;
            let px = (0..w).map(|x| mix(a, b, x as f32 / (w - 1) as f32)).collect();
            ctx.load_texture(name, ColorImage::new([w, 1], px), TextureOptions::LINEAR)
        };
        Self {
            accent: grad("grad-accent", hex(0x4f6bff), hex(0x8a5bff)),
            grey: grad("grad-grey", hex(0x7b8190), hex(0x5d6270)),
        }
    }
}

pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, tex: &TextureHandle) {
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.add(RectShape::filled(rect, radius, Color32::WHITE).with_texture(tex.id(), uv));
}

pub fn shadow(offset_y: i8, blur: u8, color: Color32) -> Shadow {
    Shadow { offset: [0, offset_y], blur, spread: 0, color }
}

/// Квадратный логотип с градиентом, как .logo.
pub fn logo(ui: &mut Ui, size: f32, glyph: &str, tex: &Textures) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let radius = size * 0.28;
    let painter = ui.painter();
    painter.add(shadow(4, 14, Color32::from_rgba_unmultiplied(111, 107, 255, 90)).as_shape(rect, radius));
    gradient_rect(painter, rect, radius, &tex.accent);
    painter.text(rect.center(), Align2::CENTER_CENTER, glyph, bold(size * 0.48), Color32::WHITE);
}

// ───────────────────────── текст ─────────────────────────

/// Заголовок секции капсом с разрядкой (как h2 в настройках).
pub fn caps(ui: &mut Ui, text: &str, p: &Pal) -> Response {
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        TextFormat { font_id: semi(11.5), color: p.muted, extra_letter_spacing: 0.8, ..Default::default() },
    );
    ui.label(job)
}

pub fn hint(ui: &mut Ui, text: &str, p: &Pal) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(reg(12.5)).color(p.muted)).wrap())
}

pub fn divider(ui: &mut Ui, p: &Pal) {
    ui.add_space(10.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.line));
    ui.add_space(10.0);
}

// ───────────────────────── кнопки ─────────────────────────

/// Большая градиентная кнопка (как .hero / главная кнопка «Крутить»).
#[allow(clippy::too_many_arguments)]
pub fn gradient_button(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    label: &str,
    icon: Option<fn(&Painter, Pos2, Color32)>,
    enabled: bool,
    tex: &Textures,
    p: &Pal,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    let pressed = enabled && resp.is_pointer_button_down_on();
    let rect = if pressed { rect.shrink(1.0) } else { rect };
    let radius = 12.0;
    let painter = ui.painter();
    if enabled {
        let a = if p.dark { 70 } else { 60 };
        painter.add(shadow(8, 24, Color32::from_rgba_unmultiplied(95, 100, 255, a)).as_shape(rect, radius));
        gradient_rect(painter, rect, radius, &tex.accent);
        if resp.hovered() {
            painter.rect_filled(rect, radius, Color32::from_white_alpha(20));
        }
    } else {
        gradient_rect(painter, rect, radius, &tex.grey);
    }
    let text_col = if enabled { Color32::WHITE } else { Color32::from_white_alpha(200) };
    let galley = painter.layout_no_wrap(label.to_string(), semi(15.0), text_col);
    let icon_w = if icon.is_some() { 24.0 } else { 0.0 };
    let total = galley.size().x + icon_w;
    let x0 = rect.center().x - total / 2.0;
    if let Some(draw) = icon {
        draw(painter, pos2(x0 + 8.0, rect.center().y), text_col);
    }
    painter.galley(pos2(x0 + icon_w, rect.center().y - galley.size().y / 2.0), galley, text_col);
    if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    /// Прозрачная с рамкой (.btn.ghost).
    Ghost,
    /// Мягкая акцентная заливка (как «Browse…»).
    Soft,
    /// Маленькая серая (как «Clear» в поле ввода).
    Small,
    /// Текстовая ссылка (.link).
    Link,
}

pub fn button_at(ui: &mut Ui, rect: Rect, id: Id, label: &str, kind: Btn, p: &Pal) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hov = resp.hovered();
    let (fill, stroke, col) = match kind {
        Btn::Ghost => (
            if hov { p.hover } else { Color32::TRANSPARENT },
            Stroke::new(1.0, p.line),
            if hov { p.text } else { p.muted },
        ),
        Btn::Soft => (
            if hov { mix(p.accent_soft, p.accent, 0.12) } else { p.accent_soft },
            Stroke::NONE,
            p.accent,
        ),
        Btn::Small => (
            if hov { Color32::from_rgba_unmultiplied(127, 127, 127, 60) } else { Color32::from_rgba_unmultiplied(127, 127, 127, 36) },
            Stroke::NONE,
            if hov { p.text } else { p.muted },
        ),
        Btn::Link => (Color32::TRANSPARENT, Stroke::NONE, if hov { p.accent2 } else { p.accent }),
    };
    let font = match kind {
        Btn::Small => semi(12.0),
        Btn::Link => reg(13.0),
        _ => semi(13.5),
    };
    let radius = if kind == Btn::Small { 7.0 } else { 9.0 };
    let painter = ui.painter();
    painter.rect(rect, radius, fill, stroke, StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, col);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Кнопка, которая сама занимает место по размеру текста.
pub fn button(ui: &mut Ui, label: &str, kind: Btn, p: &Pal) -> Response {
    let (font, pad) = match kind {
        Btn::Small => (semi(12.0), vec2(9.0, 4.0)),
        Btn::Link => (reg(13.0), vec2(0.0, 2.0)),
        _ => (semi(13.5), vec2(14.0, 8.0)),
    };
    let w = ui.painter().layout_no_wrap(label.to_string(), font, p.text).size();
    let (rect, _) = ui.allocate_exact_size(w + pad * 2.0, Sense::hover());
    button_at(ui, rect, ui.scope_id().with(("btn", label)), label, kind, p)
}

// ───────────────────────── переключатель и слайдер ─────────────────────────

/// Переключатель 42×24 как .switch.
pub fn switch(ui: &mut Ui, on: &mut bool, p: &Pal) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(42.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.18);
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, mix(p.track, p.accent, t));
    let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12.0, t);
    let c = pos2(x, rect.center().y);
    painter.circle_filled(c + vec2(0.0, 1.0), 9.5, Color32::from_black_alpha(45));
    painter.circle_filled(c, 9.0, Color32::WHITE);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Слайдер с акцентной заливкой (как .slider с accent-color).
pub fn slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, step: f32, p: &Pal) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 18.0, 4.0));
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
    painter.rect_filled(track, 2.0, p.track);
    painter.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), 2.0, p.accent);
    let c = pos2(x, track.center().y);
    let active = resp.hovered() || resp.dragged();
    if active {
        painter.circle_filled(c, 13.0, p.accent_soft);
    }
    painter.circle_filled(c + vec2(0.0, 1.0), 9.0, Color32::from_black_alpha(40));
    painter.circle_filled(c, 8.5, Color32::WHITE);
    painter.circle_stroke(c, 8.5, Stroke::new(2.0, p.accent));
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Значение в «таблетке» как .pill.
pub fn pill(ui: &mut Ui, text: &str, p: &Pal) {
    let g = ui.painter().layout_no_wrap(text.to_string(), bold(12.5), p.accent);
    let size = vec2((g.size().x + 20.0).max(40.0), 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 12.0, p.accent_soft);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, p.accent);
}

// ───────────────────────── значки ─────────────────────────

pub fn icon_list(painter: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    for dy in [-4.5, 0.0, 4.5] {
        painter.line_segment([c + vec2(-6.0, dy), c + vec2(6.0, dy)], s);
    }
}

pub fn icon_wheel(painter: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    painter.circle_stroke(c, 6.8, s);
    painter.line_segment([c, c + vec2(0.0, -6.8)], s);
    painter.line_segment([c, c + vec2(5.9, 3.4)], s);
    painter.line_segment([c, c + vec2(-5.9, 3.4)], s);
}

pub fn icon_fullscreen(painter: &Painter, c: Pos2, col: Color32, exit: bool) {
    let s = Stroke::new(1.5, col);
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
    painter.add(Shape::line(pts, Stroke::new(1.7, col)));
    // Наконечник стрелки в конце дуги, по ходу часовой стрелки.
    let end = c + vec2(a1.cos(), a1.sin()) * r;
    let fwd = vec2(-a1.sin(), a1.cos());
    let normal = vec2(a1.cos(), a1.sin());
    painter.add(Shape::convex_polygon(
        vec![end + normal * 3.6, end + fwd * 4.2, end - normal * 3.6],
        col,
        Stroke::NONE,
    ));
}
