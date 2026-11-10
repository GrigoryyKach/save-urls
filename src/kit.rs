//! Дополнительные компоненты поверх `theme.rs` (сам `theme.rs` общий с Random Wheel и не меняется):
//! значки, строки навигации, пункты меню, поле ввода с фокусной обводкой, цвета значков сайтов.

use eframe::egui::{
    self, Align2, Color32, CursorIcon, Frame, Id, Margin, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, TextEdit, TextureHandle, Ui, Vec2, pos2, vec2,
};

use crate::theme::{self, Pal, hex, mix};

pub type IconFn = fn(&Painter, Pos2, Color32);

// ───────────────────────── значки ─────────────────────────

fn poly(painter: &Painter, c: Pos2, pts: &[(f32, f32)], closed: bool, s: Stroke) {
    let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| c + vec2(x, y)).collect();
    if closed {
        painter.add(Shape::closed_line(pts, s));
    } else {
        painter.add(Shape::line(pts, s));
    }
}

/// Контур капсулы вдоль направления `dir`.
fn capsule(painter: &Painter, center: Pos2, dir: Vec2, half: f32, r: f32, s: Stroke) {
    let n = vec2(-dir.y, dir.x);
    let mut pts = Vec::new();
    for (end, d, nn) in [(center + dir * half, dir, n), (center - dir * half, -dir, -n)] {
        for i in 0..=10 {
            let t = std::f32::consts::FRAC_PI_2 - std::f32::consts::PI * i as f32 / 10.0;
            pts.push(end + (d * t.cos() + nn * t.sin()) * r);
        }
    }
    painter.add(Shape::closed_line(pts, s));
}

pub fn icon_link(p: &Painter, c: Pos2, col: Color32) {
    let d = vec2(1.0, -1.0).normalized();
    let s = Stroke::new(1.5, col);
    capsule(p, c - d * 2.6, d, 2.2, 2.6, s);
    capsule(p, c + d * 2.6, d, 2.2, 2.6, s);
}

pub fn icon_inbox(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    poly(p, c, &[(-7.0, 0.5), (-4.5, -5.0), (4.5, -5.0), (7.0, 0.5), (7.0, 5.0), (-7.0, 5.0)], true, s);
    poly(p, c, &[(-7.0, 0.5), (-3.0, 0.5), (-2.0, 2.5), (2.0, 2.5), (3.0, 0.5), (7.0, 0.5)], false, s);
}

pub fn icon_folder(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    poly(p, c, &[(-7.0, -5.0), (-2.5, -5.0), (-1.0, -3.2), (7.0, -3.2), (7.0, 5.0), (-7.0, 5.0)], true, s);
}

pub fn icon_gear(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 4.2, Stroke::new(1.5, col));
    p.circle_filled(c, 1.4, col);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * 4.6, c + d * 7.0], Stroke::new(2.2, col));
    }
}

pub fn icon_plus(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.7, col);
    p.line_segment([c + vec2(-5.5, 0.0), c + vec2(5.5, 0.0)], s);
    p.line_segment([c + vec2(0.0, -5.5), c + vec2(0.0, 5.5)], s);
}

pub fn icon_search(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.circle_stroke(c + vec2(-1.5, -1.5), 5.0, s);
    p.line_segment([c + vec2(2.2, 2.2), c + vec2(6.0, 6.0)], Stroke::new(1.9, col));
}

pub fn icon_copy(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.rect_stroke(Rect::from_min_max(c + vec2(-2.0, -2.0), c + vec2(6.0, 6.0)), 2.0, s, StrokeKind::Middle);
    poly(p, c, &[(-6.0, 2.5), (-6.0, -4.5), (-4.5, -6.0), (2.5, -6.0)], false, s);
}

pub fn icon_edit(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    poly(p, c, &[(-5.5, 5.5), (-5.0, 2.5), (3.0, -5.5), (5.5, -3.0), (-2.5, 5.0)], true, s);
    p.line_segment([c + vec2(1.0, -3.5), c + vec2(3.5, -1.0)], s);
}

pub fn icon_trash(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], s);
    poly(p, c, &[(-2.0, -4.0), (-2.0, -6.0), (2.0, -6.0), (2.0, -4.0)], false, s);
    poly(p, c, &[(-4.5, -4.0), (-3.6, 6.0), (3.6, 6.0), (4.5, -4.0)], false, s);
    p.line_segment([c + vec2(-1.2, -1.5), c + vec2(-1.0, 3.5)], s);
    p.line_segment([c + vec2(1.2, -1.5), c + vec2(1.0, 3.5)], s);
}

pub fn icon_external(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    poly(p, c, &[(-0.5, -5.5), (-5.5, -5.5), (-5.5, 5.5), (5.5, 5.5), (5.5, 0.5)], false, s);
    p.line_segment([c + vec2(-1.0, 1.0), c + vec2(5.5, -5.5)], s);
    poly(p, c, &[(1.0, -5.5), (5.5, -5.5), (5.5, -1.0)], false, s);
}

pub fn icon_sort(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], s);
    p.line_segment([c + vec2(-6.0, 0.0), c + vec2(3.0, 0.0)], s);
    p.line_segment([c + vec2(-6.0, 4.0), c + vec2(0.0, 4.0)], s);
}

pub fn icon_chevron(p: &Painter, c: Pos2, col: Color32) {
    poly(p, c, &[(-3.5, -1.5), (0.0, 2.0), (3.5, -1.5)], false, Stroke::new(1.5, col));
}

pub fn icon_check(p: &Painter, c: Pos2, col: Color32) {
    poly(p, c, &[(-4.5, 0.0), (-1.5, 3.0), (4.5, -3.5)], false, Stroke::new(1.8, col));
}

pub fn icon_clipboard(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.rect_stroke(Rect::from_min_max(c + vec2(-5.0, -4.5), c + vec2(5.0, 6.5)), 2.0, s, StrokeKind::Middle);
    p.rect_filled(Rect::from_min_max(c + vec2(-2.5, -6.5), c + vec2(2.5, -3.0)), 1.5, col);
}

pub fn icon_close(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s);
    p.line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], s);
}

// ───────────────────────── цвета ─────────────────────────

pub const DANGER: Color32 = hex(0xe5484d);

const BADGES: [u32; 8] = [0x4f6bff, 0x8a5bff, 0x12a594, 0xf76b15, 0xe93d82, 0x30a46c, 0x0090ff, 0xe5484d];

/// Цвет значка сайта — стабильный для домена.
pub fn badge_color(domain: &str) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in domain.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    hex(BADGES[(h % BADGES.len() as u32) as usize])
}

/// Буква для значка: первая буква домена без «www.» и поддоменов вроде «m.».
pub fn badge_letter(domain: &str) -> String {
    let parts: Vec<&str> = domain.split('.').collect();
    let main = if parts.len() >= 2 { parts[parts.len() - 2] } else { domain };
    main.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "•".into())
}

// ───────────────────────── компоненты ─────────────────────────

/// Логотип: картинка иконки с тенью (как `theme::logo`).
pub fn logo(ui: &mut Ui, size: f32, tex: &TextureHandle) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter();
    painter.add(theme::shadow(4, 14, Color32::from_rgba_unmultiplied(111, 107, 255, 90)).as_shape(rect, size * 0.24));
    painter.image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

/// Строка навигации в боковой панели: значок, название, счётчик.
pub struct NavRow<'a> {
    pub icon: IconFn,
    pub label: &'a str,
    pub count: Option<usize>,
    pub active: bool,
    /// Подсветка при перетаскивании ссылки на папку.
    pub drop_hover: bool,
}

pub fn nav_row(ui: &mut Ui, id: Id, row: NavRow, p: &Pal) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if row.drop_hover {
        painter.rect(rect, 9.0, mix(p.sidebar, p.accent, 0.16), Stroke::new(1.5, p.accent), StrokeKind::Inside);
    } else if row.active {
        if !p.dark {
            painter.add(theme::shadow(1, 3, Color32::from_black_alpha(22)).as_shape(rect, 9.0));
        }
        painter.rect_filled(rect, 9.0, p.card);
    } else if resp.hovered() {
        painter.rect_filled(rect, 9.0, p.hover);
    }
    let col = if row.active { p.text } else if resp.hovered() { p.text } else { mix(p.muted, p.text, 0.35) };
    (row.icon)(painter, pos2(rect.left() + 20.0, rect.center().y), if row.active { p.accent } else { col });
    let right = match row.count {
        Some(n) => {
            let g = painter.layout_no_wrap(n.to_string(), theme::semi(12.0), if row.active { p.accent } else { p.muted });
            let pr = Rect::from_center_size(pos2(rect.right() - 12.0 - (g.size().x + 14.0) / 2.0, rect.center().y), vec2(g.size().x + 14.0, 20.0));
            if row.active {
                painter.rect_filled(pr, 10.0, p.accent_soft);
            }
            painter.galley(pr.center() - g.size() / 2.0, g, p.muted);
            pr.left() - 6.0
        }
        None => rect.right() - 10.0,
    };
    let clip = Rect::from_min_max(pos2(rect.left() + 38.0, rect.top()), pos2(right, rect.bottom()));
    let font = if row.active { theme::semi(14.0) } else { theme::reg(14.0) };
    painter.with_clip_rect(clip).text(pos2(clip.left(), rect.center().y), Align2::LEFT_CENTER, row.label, font, col);
    resp
}

/// Пункт выпадающего / контекстного меню.
pub fn menu_item(ui: &mut Ui, label: &str, icon: Option<IconFn>, checked: bool, danger: bool, p: &Pal) -> Response {
    let w = ui.available_width().max(200.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 7.0, if danger { DANGER.gamma_multiply(0.12) } else { p.hover });
    }
    let col = if danger { DANGER } else { p.text };
    let mut x = rect.left() + 10.0;
    if let Some(icon) = icon {
        icon(painter, pos2(x + 8.0, rect.center().y), if danger { DANGER } else { p.muted });
        x += 26.0;
    }
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 28.0, rect.bottom()));
    painter.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, theme::reg(13.5), col);
    if checked {
        icon_check(painter, pos2(rect.right() - 16.0, rect.center().y), p.accent);
    }
    resp
}

pub fn menu_separator(ui: &mut Ui, p: &Pal) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(6.0), r.center().y, Stroke::new(1.0, p.line));
}

pub fn menu_frame(p: &Pal) -> Frame {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(12)
        .inner_margin(Margin::same(5))
        .shadow(theme::shadow(8, 24, Color32::from_black_alpha(if p.dark { 110 } else { 40 })))
}

/// Квадратная кнопка-значок.
pub fn icon_button(ui: &mut Ui, rect: Rect, id: Id, icon: IconFn, tip: &str, danger: bool, p: &Pal) -> Response {
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip);
    let hov = resp.hovered();
    if hov {
        ui.painter().rect_filled(rect, 8.0, if danger { DANGER.gamma_multiply(0.14) } else { p.hover });
    }
    let col = if hov && danger { DANGER } else if hov { p.text } else { p.muted };
    icon(ui.painter(), rect.center(), col);
    resp
}

/// Однострочное поле в стиле Smart Switcher: фон, рамка, акцентная обводка в фокусе, значок слева.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, icon: Option<IconFn>, id: Id, p: &Pal) -> Response {
    let left = if icon.is_some() { 36 } else { 12 };
    let frame = Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.line)).corner_radius(10).inner_margin(Margin { left, right: 12, top: 9, bottom: 9 });
    let mut inner = None;
    let fr = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        inner = Some(ui.add(
            TextEdit::singleline(text).id(id).frame(Frame::NONE).hint_text(hint).desired_width(f32::INFINITY).font(theme::reg(14.0)),
        ));
    });
    let resp = inner.unwrap();
    let rect = fr.response.rect;
    if let Some(icon) = icon {
        icon(ui.painter(), pos2(rect.left() + 19.0, rect.center().y), if resp.has_focus() { p.accent } else { p.muted });
    }
    focus_ring(ui, rect, resp.has_focus(), p);
    resp
}

pub fn multiline_field(ui: &mut Ui, text: &mut String, hint: &str, rows: usize, id: Id, p: &Pal) -> Response {
    let frame = Frame::new().fill(p.bg).stroke(Stroke::new(1.0, p.line)).corner_radius(10).inner_margin(Margin::symmetric(12, 9));
    let mut inner = None;
    let fr = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        inner = Some(ui.add(
            TextEdit::multiline(text)
                .id(id)
                .frame(Frame::NONE)
                .hint_text(hint)
                .desired_width(f32::INFINITY)
                .desired_rows(rows)
                .font(theme::reg(14.0)),
        ));
    });
    let resp = inner.unwrap();
    focus_ring(ui, fr.response.rect, resp.has_focus(), p);
    resp
}

pub fn focus_ring(ui: &Ui, rect: Rect, focused: bool, p: &Pal) {
    if focused {
        ui.painter().rect_stroke(rect.expand(1.5), 11.5, Stroke::new(3.0, p.accent_soft), StrokeKind::Outside);
        ui.painter().rect_stroke(rect, 10.0, Stroke::new(1.0, p.accent), StrokeKind::Inside);
    }
}

/// Кнопка-выпадающий список: подпись и стрелка, как поле ввода.
pub fn dropdown_button(ui: &mut Ui, label: &str, icon: Option<IconFn>, width: f32, id: Id, p: &Pal) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 38.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let hov = resp.hovered();
    let painter = ui.painter();
    painter.rect(rect, 10.0, p.bg, Stroke::new(1.0, if hov { mix(p.line, p.muted, 0.5) } else { p.line }), StrokeKind::Inside);
    let mut x = rect.left() + 12.0;
    if let Some(icon) = icon {
        icon(painter, pos2(x + 7.0, rect.center().y), p.muted);
        x += 22.0;
    }
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 28.0, rect.bottom()));
    painter.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, theme::reg(13.5), p.text);
    icon_chevron(painter, pos2(rect.right() - 15.0, rect.center().y), p.muted);
    resp
}

/// Сегментированный переключатель (как «Список / Колесо» в Random Wheel).
pub fn segmented(ui: &mut Ui, items: &[&str], selected: usize, p: &Pal) -> Option<usize> {
    let font = theme::semi(13.0);
    let widths: Vec<f32> = items.iter().map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), p.text).size().x + 28.0).collect();
    let h = 36.0;
    let (rect, _) = ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 8.0, h), Sense::hover());
    ui.painter().rect(rect, 11.0, p.sidebar, Stroke::new(1.0, p.line), StrokeKind::Inside);
    let mut x = rect.left() + 4.0;
    let mut out = None;
    for (i, (label, w)) in items.iter().zip(widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(w, h - 8.0));
        let resp = ui.interact(r, ui.scope_id().with(("seg", i, *label)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let active = i == selected;
        let painter = ui.painter();
        if active {
            if !p.dark {
                painter.add(theme::shadow(1, 3, Color32::from_black_alpha(25)).as_shape(r, 8.0));
            }
            painter.rect_filled(r, 8.0, p.card);
        } else if resp.hovered() {
            painter.rect_filled(r, 8.0, p.hover);
        }
        let col = if active { p.accent } else if resp.hovered() { p.text } else { p.muted };
        painter.text(r.center(), Align2::CENTER_CENTER, *label, font.clone(), col);
        if resp.clicked() && !active {
            out = Some(i);
        }
        x += w;
    }
    out
}

/// Строка «название + подсказка + переключатель» (как в Random Wheel).
pub fn switch_row(ui: &mut Ui, p: &Pal, title: &str, hint: &str, value: &mut bool) -> bool {
    let mut changed = false;
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(w - 58.0);
            ui.label(egui::RichText::new(title).font(theme::reg(14.0)).color(p.text));
            if !hint.is_empty() {
                ui.add_space(-6.0);
                theme::hint(ui, hint, p);
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = theme::switch(ui, value, p).changed();
        });
    });
    changed
}

/// Карточка-секция для страницы настроек.
pub fn card_frame(p: &Pal) -> Frame {
    Frame::new().fill(p.card).stroke(Stroke::new(1.0, p.line)).corner_radius(14).inner_margin(Margin::symmetric(20, 16))
}
