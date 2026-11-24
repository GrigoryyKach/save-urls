//! Extra components on top of `theme.rs` (`theme.rs` itself is shared across the suite and stays untouched):
//! icons, navigation rows, menu items, input fields, the domain monogram tile.

use eframe::egui::{
    self, Align2, Color32, CursorIcon, Frame, Id, Margin, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    RichText, StrokeKind, TextEdit, Ui, Vec2, pos2, vec2,
    text::{LayoutJob, TextFormat, TextWrapping},
};

use crate::theme::{self, Pal, hex, mix};

pub type IconFn = fn(&Painter, Pos2, Color32);

// ───────────────────────── icons ─────────────────────────

fn poly(painter: &Painter, c: Pos2, pts: &[(f32, f32)], closed: bool, s: Stroke) {
    let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| c + vec2(x, y)).collect();
    if closed {
        painter.add(Shape::closed_line(pts, s));
    } else {
        painter.add(Shape::line(pts, s));
    }
}

/// Capsule outline along the direction `dir`.
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
        p.line_segment([c + d * 4.6, c + d * 7.0], Stroke::new(1.5, col));
    }
}

pub fn icon_plus(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.line_segment([c + vec2(-5.5, 0.0), c + vec2(5.5, 0.0)], s);
    p.line_segment([c + vec2(0.0, -5.5), c + vec2(0.0, 5.5)], s);
}

pub fn icon_search(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.circle_stroke(c + vec2(-1.5, -1.5), 5.0, s);
    p.line_segment([c + vec2(2.2, 2.2), c + vec2(6.0, 6.0)], Stroke::new(1.5, col));
}

pub fn icon_copy(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.rect_stroke(Rect::from_min_max(c + vec2(-2.0, -2.0), c + vec2(6.0, 6.0)), 2.0, s, StrokeKind::Middle);
    poly(p, c, &[(-6.0, 2.5), (-6.0, -4.5), (-4.5, -6.0), (2.5, -6.0)], false, s);
}

pub fn icon_edit(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    poly(p, c, &[(-5.5, 5.5), (-5.0, 2.5), (3.0, -5.5), (5.5, -3.0), (-2.5, 5.0)], true, s);
    p.line_segment([c + vec2(1.0, -3.5), c + vec2(3.5, -1.0)], s);
}

pub fn icon_trash(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], s);
    poly(p, c, &[(-2.0, -4.0), (-2.0, -6.0), (2.0, -6.0), (2.0, -4.0)], false, s);
    poly(p, c, &[(-4.5, -4.0), (-3.6, 6.0), (3.6, 6.0), (4.5, -4.0)], false, s);
    p.line_segment([c + vec2(-1.2, -1.5), c + vec2(-1.0, 3.5)], s);
    p.line_segment([c + vec2(1.2, -1.5), c + vec2(1.0, 3.5)], s);
}

pub fn icon_external(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    poly(p, c, &[(-0.5, -5.5), (-5.5, -5.5), (-5.5, 5.5), (5.5, 5.5), (5.5, 0.5)], false, s);
    p.line_segment([c + vec2(-1.0, 1.0), c + vec2(5.5, -5.5)], s);
    poly(p, c, &[(1.0, -5.5), (5.5, -5.5), (5.5, -1.0)], false, s);
}

pub fn icon_sort(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.line_segment([c + vec2(-6.0, -4.0), c + vec2(6.0, -4.0)], s);
    p.line_segment([c + vec2(-6.0, 0.0), c + vec2(3.0, 0.0)], s);
    p.line_segment([c + vec2(-6.0, 4.0), c + vec2(0.0, 4.0)], s);
}

pub fn icon_chevron(p: &Painter, c: Pos2, col: Color32) {
    poly(p, c, &[(-3.5, -1.5), (0.0, 2.0), (3.5, -1.5)], false, Stroke::new(1.5, col));
}

/// Chevron pointing right: a collapsed tree node.
pub fn icon_chevron_right(p: &Painter, c: Pos2, col: Color32) {
    poly(p, c, &[(-1.5, -3.5), (2.0, 0.0), (-1.5, 3.5)], false, Stroke::new(1.5, col));
}

pub fn icon_check(p: &Painter, c: Pos2, col: Color32) {
    poly(p, c, &[(-4.5, 0.0), (-1.5, 3.0), (4.5, -3.5)], false, Stroke::new(1.5, col));
}

pub fn icon_clipboard(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.rect_stroke(Rect::from_min_max(c + vec2(-5.0, -4.5), c + vec2(5.0, 6.5)), 2.0, s, StrokeKind::Middle);
    p.rect_filled(Rect::from_min_max(c + vec2(-2.5, -6.5), c + vec2(2.5, -3.0)), 1.5, col);
}

pub fn icon_close(p: &Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s);
    p.line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], s);
}

// ───────────────────────── colours ─────────────────────────

/// Destructive actions and errors: the shared negative role (4.5:1 or better on every surface).
pub fn danger(p: &Pal) -> Color32 {
    p.negative
}

/// Text on a filled negative button: dark on the lighter dark-theme red, white on the light-theme red.
pub fn on_danger(p: &Pal) -> Color32 {
    if p.dark { hex(0x141415) } else { Color32::WHITE }
}

/// Muted tints for the monogram tiles: one per folder, never per link.
const FOLDER_TINTS: [u32; 6] = [0x7d93c0, 0x76ab93, 0xc08c6a, 0xb7819a, 0x8f86bd, 0x6aa9ae];

/// The tile of a link: fill and letter colour. Links outside folders are plain neutral;
/// each folder gets one quiet tint, so the tiles of a folder read as a set.
pub fn tile_colors(folder: Option<u64>, p: &Pal) -> (Color32, Color32) {
    match folder {
        None => (p.raised, p.text2),
        Some(id) => {
            let tint = hex(FOLDER_TINTS[(id % FOLDER_TINTS.len() as u64) as usize]);
            (mix(p.raised, tint, if p.dark { 0.16 } else { 0.20 }), mix(p.text2, tint, if p.dark { 0.2 } else { 0.0 }))
        }
    }
}

/// Monogram letter: the first letter of the domain without "www." and subdomains like "m.".
pub fn badge_letter(domain: &str) -> String {
    let parts: Vec<&str> = domain.split('.').collect();
    let main = if parts.len() >= 2 { parts[parts.len() - 2] } else { domain };
    main.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "•".into())
}

/// The domain monogram tile: a 36px rounded square, the letter in mono.
pub fn monogram(painter: &Painter, rect: Rect, letter: &str, folder: Option<u64>, p: &Pal) {
    let (fill, ink) = tile_colors(folder, p);
    painter.rect(rect, 8.0, fill, Stroke::new(1.0, p.line), StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, letter, theme::mono(16.0), ink);
}

// ───────────────────────── components ─────────────────────────

/// Navigation row in the sidebar: icon, label, counter.
pub struct NavRow<'a> {
    pub icon: IconFn,
    pub label: &'a str,
    pub count: Option<usize>,
    pub active: bool,
    /// Highlight while a link is dragged over the folder.
    pub drop_hover: bool,
    /// The row is being dragged: draw it faded.
    pub dim: bool,
}

pub const NAV_H: f32 = 36.0;

pub fn nav_row(ui: &mut Ui, id: Id, row: NavRow, p: &Pal) -> Response {
    nav_row_impl(ui, id, row, None, p).0
}

/// Indent step of one folder level in the sidebar tree.
pub const TREE_STEP: f32 = 14.0;

/// A folder row of the tree: indented by `level` (0 = top), with a chevron when `chevron` is `Some(expanded)`.
/// Without a chevron the icon and name stay where they would be next to one. Returns the row and whether the chevron was clicked.
pub fn nav_row_tree(ui: &mut Ui, id: Id, row: NavRow, level: usize, chevron: Option<bool>, p: &Pal) -> (Response, bool) {
    nav_row_impl(ui, id, row, Some((level as f32 * TREE_STEP, chevron)), p)
}

fn nav_row_impl(ui: &mut Ui, id: Id, row: NavRow, tree: Option<(f32, Option<bool>)>, p: &Pal) -> (Response, bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), NAV_H), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click_and_drag()).on_hover_cursor(CursorIcon::PointingHand);
    let t = theme::fade(ui, id.with("hover"), resp.hovered() && !row.active);
    let mut painter = ui.painter().clone();
    if row.dim {
        painter.multiply_opacity(0.4);
    }
    let painter = &painter;
    if row.drop_hover {
        painter.rect(rect, 8.0, p.raised_hi, Stroke::new(2.0, p.accent), StrokeKind::Inside);
    } else if row.active {
        painter.rect(rect, 8.0, p.raised, Stroke::new(1.0, p.line), StrokeKind::Inside);
        painter.hline(rect.x_range().shrink(8.0), rect.top() + 0.5, Stroke::new(1.0, p.top_light()));
    } else if t > 0.0 {
        painter.rect_filled(rect, 8.0, theme::alpha(p.raised, (t * 200.0) as u8));
    }
    let col = if row.active { p.text } else { mix(p.text2, p.text, t) };
    let (indent, icon_x, text_x) = match tree {
        Some((indent, _)) => (indent, 28.0 + indent, 44.0 + indent),
        None => (0.0, 20.0, 36.0),
    };
    let mut chevron_clicked = false;
    if let Some((_, Some(open))) = tree {
        let hit = Rect::from_min_size(pos2(rect.left() + indent, rect.top()), vec2(22.0, rect.height()));
        let cr = ui.interact(hit, id.with("chevron"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        chevron_clicked = cr.clicked();
        let ct = theme::fade(ui, id.with("chevron-hover"), cr.hovered());
        let c = pos2(hit.center().x, rect.center().y);
        let colr = mix(p.muted, p.text, ct);
        if open { icon_chevron(painter, c, colr) } else { icon_chevron_right(painter, c, colr) }
    }
    (row.icon)(painter, pos2(rect.left() + icon_x, rect.center().y), if row.active { p.text } else { mix(p.muted, p.text2, t) });
    let right = match row.count {
        Some(n) => {
            let g = painter.layout_no_wrap(n.to_string(), theme::mono(12.0), p.muted);
            let right = rect.right() - 12.0 - g.size().x - 8.0;
            painter.galley(pos2(rect.right() - 12.0 - g.size().x, rect.center().y - g.size().y / 2.0), g, p.muted);
            right
        }
        None => rect.right() - 8.0,
    };
    let clip = Rect::from_min_max(pos2(rect.left() + text_x, rect.top()), pos2(right, rect.bottom()));
    let font = if row.active { theme::medium(14.0) } else { theme::reg(14.0) };
    // One line, cut with an ellipsis.
    let mut job = LayoutJob::default();
    job.append(row.label, 0.0, TextFormat { font_id: font, color: col, ..Default::default() });
    job.wrap = TextWrapping { max_width: clip.width().max(1.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let g = painter.layout_job(job);
    painter.galley(pos2(clip.left(), rect.center().y - g.size().y / 2.0), g, col);
    (resp, chevron_clicked)
}

/// Dropdown / context menu item.
pub fn menu_item(ui: &mut Ui, label: &str, icon: Option<IconFn>, checked: bool, is_danger: bool, p: &Pal) -> Response {
    menu_item_indented(ui, label, icon, checked, is_danger, 0.0, p)
}

/// [`menu_item`] shifted right by `indent`, for the levels of a folder tree.
pub fn menu_item_indented(ui: &mut Ui, label: &str, icon: Option<IconFn>, checked: bool, is_danger: bool, indent: f32, p: &Pal) -> Response {
    let w = ui.available_width().max(200.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let t = theme::fade(ui, resp.id.with("hover"), resp.hovered());
    let painter = ui.painter();
    if t > 0.0 {
        painter.rect_filled(rect, 8.0, theme::alpha(p.raised, (t * 255.0) as u8));
    }
    let col = if is_danger { danger(p) } else { p.text };
    let mut x = rect.left() + 8.0 + indent;
    if let Some(icon) = icon {
        icon(painter, pos2(x + 8.0, rect.center().y), if is_danger { danger(p) } else { p.text2 });
        x += 24.0;
    }
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 28.0, rect.bottom()));
    painter.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, theme::reg(14.0), col);
    if checked {
        icon_check(painter, pos2(rect.right() - 16.0, rect.center().y), p.text);
    }
    resp
}

/// A menu item that cannot be used: muted, with a tooltip that says why.
pub fn menu_item_disabled(ui: &mut Ui, label: &str, icon: Option<IconFn>, tip: &str, p: &Pal) {
    let w = ui.available_width().max(200.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    let resp = resp.on_hover_cursor(CursorIcon::NotAllowed);
    let painter = ui.painter();
    let mut x = rect.left() + 8.0;
    if let Some(icon) = icon {
        icon(painter, pos2(x + 8.0, rect.center().y), p.muted);
        x += 24.0;
    }
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 8.0, rect.bottom()));
    painter.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, theme::reg(14.0), p.muted);
    resp.on_hover_text(tip);
}

pub fn menu_separator(ui: &mut Ui, p: &Pal) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(4.0), r.center().y, Stroke::new(1.0, p.line));
}

/// Caption inside a menu ("Move to").
pub fn menu_caption(ui: &mut Ui, text: &str, p: &Pal) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).font(theme::medium(12.0)).color(p.muted));
    ui.add_space(4.0);
}

pub fn menu_frame(p: &Pal) -> Frame {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.line_hi))
        .corner_radius(12)
        .inner_margin(Margin::same(4))
        .shadow(theme::shadow_for(p))
}

/// Square icon button.
pub fn icon_button(ui: &mut Ui, rect: Rect, id: Id, icon: IconFn, tip: &str, is_danger: bool, p: &Pal) -> Response {
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip);
    let t = theme::fade(ui, id.with("hover"), resp.hovered());
    if t > 0.0 {
        ui.painter().rect_filled(rect, 8.0, theme::alpha(p.raised_hi, (t * 255.0) as u8));
    }
    let hot = if is_danger { danger(p) } else { p.text };
    icon(ui.painter(), rect.center(), mix(p.text2, hot, t));
    resp
}

/// Single-line field: input fill, outline, a 2px accent ring on focus, icon on the left.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, icon: Option<IconFn>, id: Id, p: &Pal) -> Response {
    let left = if icon.is_some() { 36 } else { 12 };
    let frame = theme::field_frame(p).inner_margin(Margin { left, right: 12, top: 2, bottom: 2 });
    let mut inner = None;
    let fr = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        inner = Some(ui.add(
            TextEdit::singleline(text)
                .id(id)
                .frame(Frame::NONE)
                .hint_text(hint)
                .desired_width(f32::INFINITY)
                .align(egui::Align2::LEFT_CENTER)
                .font(theme::reg(14.0)),
        ));
    });
    let resp = inner.unwrap();
    let rect = fr.response.rect;
    if let Some(icon) = icon {
        icon(ui.painter(), pos2(rect.left() + 20.0, rect.center().y), if resp.has_focus() { p.text } else { p.muted });
    }
    focus_ring(ui, rect, resp.has_focus(), p);
    resp
}

pub fn multiline_field(ui: &mut Ui, text: &mut String, hint: &str, rows: usize, id: Id, p: &Pal) -> Response {
    let frame = theme::field_frame(p).inner_margin(Margin::symmetric(12, 8));
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

/// Focus: a 2px accent ring over the outline.
pub fn focus_ring(ui: &Ui, rect: Rect, focused: bool, p: &Pal) {
    if focused {
        ui.painter().rect_stroke(rect, theme::R_CONTROL as f32, Stroke::new(2.0, p.accent), StrokeKind::Inside);
    }
}

/// Dropdown button: caption and arrow, styled like an input field.
pub fn dropdown_button(ui: &mut Ui, label: &str, icon: Option<IconFn>, width: f32, id: Id, p: &Pal) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let t = theme::fade(ui, id.with("hover"), resp.hovered());
    let painter = ui.painter();
    painter.rect(rect, theme::R_CONTROL as f32, mix(p.input, p.raised_hi, t), Stroke::new(1.0, p.line_hi), StrokeKind::Inside);
    let mut x = rect.left() + 12.0;
    if let Some(icon) = icon {
        icon(painter, pos2(x + 8.0, rect.center().y), p.text2);
        x += 24.0;
    }
    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.right() - 28.0, rect.bottom()));
    painter.with_clip_rect(clip).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, theme::reg(14.0), p.text);
    icon_chevron(painter, pos2(rect.right() - 16.0, rect.center().y), p.text2);
    resp
}

/// Row "title + hint + switch".
pub fn switch_row(ui: &mut Ui, p: &Pal, title: &str, hint: &str, value: &mut bool) -> bool {
    let mut changed = false;
    let w = ui.available_width();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(w - 56.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(title).font(theme::reg(14.0)).color(p.text));
            if !hint.is_empty() {
                theme::hint(ui, hint, p);
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = theme::switch(ui, value, p).changed();
        });
    });
    changed
}

/// Title and hint on the left, a control on the right (the same column as `switch_row`).
pub fn setting_row(ui: &mut Ui, p: &Pal, title: &str, hint: &str, control_w: f32, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - control_w - 16.0).max(160.0));
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(title).font(theme::reg(14.0)).color(p.text));
            theme::hint(ui, hint, p);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monogram_letters_are_readable_on_their_tiles() {
        for p in [Pal::dark(), Pal::light()] {
            for folder in std::iter::once(None).chain((0..6).map(Some)) {
                let (fill, ink) = tile_colors(folder, &p);
                let c = theme::contrast(ink, fill);
                assert!(c >= 4.5, "dark={} folder={folder:?}: {c:.2}", p.dark);
            }
        }
    }

    #[test]
    fn danger_is_readable_as_text_and_as_a_fill() {
        for p in [Pal::dark(), Pal::light()] {
            for s in [p.sidebar, p.bg, p.card] {
                assert!(theme::contrast(danger(&p), s) >= 4.5, "dark={}", p.dark);
            }
            assert!(theme::contrast(on_danger(&p), danger(&p)) >= 4.5, "dark={}", p.dark);
        }
    }

    #[test]
    fn badge_letter_skips_www() {
        assert_eq!(badge_letter("www.github.com"), "G");
        assert_eq!(badge_letter("m.youtube.com"), "Y");
    }
}
