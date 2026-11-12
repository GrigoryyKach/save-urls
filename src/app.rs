use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, DragAndDrop, Frame, Id, Key, Layout, Margin, Modifiers, Order, Popup,
    Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, ViewportCommand, pos2,
    text::{LayoutJob, TextFormat, TextWrapping},
    vec2,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::i18n;
use crate::kit::{self, IconFn};
use crate::nimvoro::{self, UpdateState, Updater};
use crate::search::Query;
use crate::store::{Added, ClickAction, FolderId, Link, LinkId, Shared, Sort, Store};
use crate::strings;
use crate::theme::{self, Btn, Pal, Textures};
use crate::urlx;
use crate::win::{self, Event, Hub};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    All,
    Unsorted,
    Folder(FolderId),
    Settings,
}

impl Page {
    fn folder_filter(self) -> Option<Option<FolderId>> {
        match self {
            Page::Unsorted => Some(None),
            Page::Folder(f) => Some(Some(f)),
            _ => None,
        }
    }
}

/// A link is being dragged with the mouse onto a folder in the sidebar.
#[derive(Clone, Copy)]
struct LinkDrag(LinkId);

struct Editor {
    id: Option<LinkId>,
    url: String,
    title: String,
    desc: String,
    folder: Option<FolderId>,
    orig_title: String,
    error: Option<String>,
    focus_pending: bool,
    fetch: Option<Arc<Mutex<Option<Option<String>>>>>,
}

struct Toast {
    text: String,
    undo: Option<Link>,
    edit: Option<LinkId>,
    at: f64,
}

struct Rename {
    id: FolderId,
    text: String,
    focus_pending: bool,
}

/// Actions that cannot run while the library is locked for the frame (file dialogs).
enum After {
    Export,
}

struct ListCache {
    rev: u64,
    query: String,
    page: Page,
    sort: Sort,
    ids: Vec<LinkId>,
}

pub struct App {
    store: Shared,
    hub: Arc<Hub>,
    hwnd: isize,
    tex: Textures,
    logo_tex: TextureHandle,
    page: Page,
    query: String,
    editor: Option<Editor>,
    confirm_delete_folder: Option<FolderId>,
    rename: Option<Rename>,
    toast: Option<Toast>,
    after: Option<After>,
    cache: Option<ListCache>,
    background_start: bool,
    frames: u64,
    autostart: bool,
    autostart_error: bool,
    updater: Updater,
}

/// "5 links" / "5 ссылок" for the current interface language.
fn pl(n: usize, forms: &[&str; 3]) -> String {
    i18n::plural(strings::lang(), n, forms)
}

fn fmt_date(ts: i64) -> String {
    let tr = strings::s();
    let t = win::local_time(ts);
    let now = win::local_time(crate::store::now_unix());
    let same = |a: &win::LocalTime, b: &win::LocalTime| a.year == b.year && a.month == b.month && a.day == b.day;
    if same(&t, &now) {
        return format!("{}, {:02}:{:02}", tr.today, t.hour, t.minute);
    }
    if same(&win::local_time(ts + 86_400), &now) {
        return format!("{}, {:02}:{:02}", tr.yesterday, t.hour, t.minute);
    }
    let m = tr.months[(t.month.clamp(1, 12) - 1) as usize];
    let day_month = if tr.day_first { format!("{} {m}", t.day) } else { format!("{m} {}", t.day) };
    if t.year == now.year { day_month } else if tr.day_first { format!("{day_month} {}", t.year) } else { format!("{day_month}, {}", t.year) }
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Bookmarks file in the browser (Netscape) format: it can be imported into Chrome, Firefox and Edge.
fn bookmarks_html(st: &Store) -> String {
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n\
         <TITLE>Bookmarks</TITLE>\n<H1>Save URLs</H1>\n<DL><p>\n",
    );
    let link = |out: &mut String, l: &Link, indent: &str| {
        out.push_str(&format!("{indent}<DT><A HREF=\"{}\" ADD_DATE=\"{}\">{}</A>\n", esc_html(&l.url), l.added, esc_html(&l.title)));
        if !l.description.trim().is_empty() {
            out.push_str(&format!("{indent}<DD>{}\n", esc_html(l.description.trim())));
        }
    };
    for f in &st.lib.folders {
        out.push_str(&format!("    <DT><H3>{}</H3>\n    <DL><p>\n", esc_html(&f.name)));
        for l in st.lib.links.iter().filter(|l| l.folder == Some(f.id)) {
            link(&mut out, l, "        ");
        }
        out.push_str("    </DL><p>\n");
    }
    for l in st.lib.links.iter().filter(|l| l.folder.is_none()) {
        link(&mut out, l, "    ");
    }
    out.push_str("</DL><p>\n");
    out
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Shared, background: bool) -> Self {
        let mode = store.lock().map(|s| s.lib.settings.theme).unwrap_or_default();
        theme::install(&cc.egui_ctx, mode);
        let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
            _ => 0,
        };
        let hub = Arc::new(Hub { store: store.clone(), ctx: cc.egui_ctx.clone(), events: Mutex::new(Vec::new()), clip_url: Mutex::new(None) });
        win::spawn_tray(hub.clone());
        let logo = match eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
            Ok(i) => egui::ColorImage::from_rgba_unmultiplied([i.width as usize, i.height as usize], &i.rgba),
            Err(_) => egui::ColorImage::filled([1, 1], Color32::TRANSPARENT),
        };
        let logo_tex = cc.egui_ctx.load_texture("logo", logo, TextureOptions::LINEAR);
        let updater = Updater::new("save-urls", "GrigoryyKach/save-urls", env!("CARGO_PKG_VERSION"));
        if store.lock().map(|s| s.lib.settings.check_updates).unwrap_or(true) {
            let ctx = cc.egui_ctx.clone();
            updater.check(move || ctx.request_repaint());
        }
        #[allow(unused_mut)]
        let mut app = Self {
            store,
            hub,
            hwnd,
            tex: Textures::new(&cc.egui_ctx),
            logo_tex,
            page: Page::All,
            query: String::new(),
            editor: None,
            confirm_delete_folder: None,
            rename: None,
            toast: None,
            after: None,
            cache: None,
            background_start: background,
            frames: 0,
            autostart: win::autostart_enabled(),
            autostart_error: false,
            updater,
        };
        // For manual checks of debug builds only: SAVE_URLS_DEBUG="q:text;page:settings;edit:10;folder:1;new;lang:ru".
        #[cfg(debug_assertions)]
        if let Ok(dbg) = std::env::var("SAVE_URLS_DEBUG") {
            let st = app.store.clone();
            let st = st.lock().unwrap();
            for part in dbg.split(';') {
                let (k, v) = part.split_once(':').unwrap_or((part, ""));
                match k {
                    "q" => app.query = v.to_string(),
                    "page" if v == "settings" => app.page = Page::Settings,
                    "page" if v == "unsorted" => app.page = Page::Unsorted,
                    "folder" => app.page = Page::Folder(v.parse().unwrap_or(0)),
                    "edit" => app.open_editor(&st, v.parse().ok(), None),
                    "new" => app.open_editor(&st, None, Some(v.to_string()).filter(|s| !s.is_empty())),
                    "delete-folder" => app.confirm_delete_folder = v.parse().ok(),
                    "theme" if v == "light" => cc.egui_ctx.set_theme(egui::ThemePreference::Light),
                    "lang" => {
                        if let Some(l) = i18n::Lang::from_code(v) {
                            strings::set_lang(l);
                        }
                    }
                    "toast" => app.toast = Some(Toast { text: strings::s().deleted.replace("{t}", "Excalidraw"), undo: Some(Link::default()), edit: None, at: 0.0 }),
                    _ => {}
                }
            }
        }
        app
    }

    fn now(ui: &Ui) -> f64 {
        ui.input(|i| i.time)
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        win::bring_to_front(self.hwnd);
    }

    fn open_editor(&mut self, st: &Store, id: Option<LinkId>, url: Option<String>) {
        let ed = match id.and_then(|id| st.link(id)) {
            Some(l) => Editor {
                id: Some(l.id),
                url: l.url.clone(),
                title: l.title.clone(),
                desc: l.description.clone(),
                folder: l.folder,
                orig_title: l.title.clone(),
                error: None,
                focus_pending: true,
                fetch: None,
            },
            None => Editor {
                id: None,
                url: url.unwrap_or_default(),
                title: String::new(),
                desc: String::new(),
                folder: match self.page {
                    Page::Folder(f) => Some(f),
                    Page::Unsorted => None,
                    _ => st.lib.settings.default_folder,
                },
                orig_title: String::new(),
                error: None,
                focus_pending: true,
                fetch: None,
            },
        };
        self.editor = Some(ed);
    }

    /// Quick save (from the clipboard banner or Ctrl+V): no dialog, with a notice at the bottom.
    fn quick_save(&mut self, st: &mut Store, url: &str, now: f64) {
        let folder = self.page.folder_filter();
        match st.add_url(url, folder) {
            Added::New(id) => {
                st.save_now();
                if st.lib.settings.fetch_titles {
                    win::fetch_title_async(self.store.clone(), self.hub.ctx.clone(), id, url.to_string(), None);
                }
                let place = st.folder_name(st.link(id).and_then(|l| l.folder));
                self.toast = Some(Toast { text: strings::s().saved_in.replace("{place}", &place), undo: None, edit: Some(id), at: now });
            }
            Added::Exists(id) => {
                self.toast = Some(Toast { text: strings::s().already_in.into(), undo: None, edit: Some(id), at: now });
            }
        }
    }

    fn delete_link(&mut self, st: &mut Store, id: LinkId, now: f64) {
        if let Some(l) = st.delete_link(id) {
            self.toast = Some(Toast { text: strings::s().deleted.replace("{t}", &win::ellipsize(&l.title, 40)), undo: Some(l), edit: None, at: now });
        }
    }

    fn visible_ids(&mut self, st: &Store) -> Vec<LinkId> {
        let sort = st.lib.settings.sort;
        if let Some(c) = &self.cache
            && c.rev == st.rev
            && c.query == self.query
            && c.page == self.page
            && c.sort == sort
        {
            return c.ids.clone();
        }
        let filter = self.page.folder_filter();
        let q = Query::new(&self.query);
        let mut items: Vec<(&Link, i32)> = st
            .lib
            .links
            .iter()
            .filter(|l| filter.is_none_or(|f| l.folder == f))
            .filter_map(|l| {
                if q.is_empty() { Some((l, 0)) } else { q.score(&l.title, &l.description, &urlx::pretty(&l.url)).map(|s| (l, s)) }
            })
            .collect();
        let key = |l: &Link| l.title.to_lowercase();
        items.sort_by(|(a, sa), (b, sb)| {
            sb.cmp(sa).then_with(|| match sort {
                Sort::Newest => (b.added, b.id).cmp(&(a.added, a.id)),
                Sort::Oldest => (a.added, a.id).cmp(&(b.added, b.id)),
                Sort::NameAsc => key(a).cmp(&key(b)),
                Sort::NameDesc => key(b).cmp(&key(a)),
                Sort::Domain => urlx::domain(&a.url).cmp(&urlx::domain(&b.url)).then_with(|| key(a).cmp(&key(b))),
            })
        });
        let ids: Vec<LinkId> = items.into_iter().map(|(l, _)| l.id).collect();
        self.cache = Some(ListCache { rev: st.rev, query: self.query.clone(), page: self.page, sort, ids: ids.clone() });
        ids
    }

    // ───────────────────────── sidebar ─────────────────────────

    fn sidebar(&mut self, ui: &mut Ui, st: &mut Store, p: &Pal) {
        let t = strings::s();
        ui.horizontal(|ui| {
            kit::logo(ui, 36.0, &self.logo_tex);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(RichText::new("Save URLs").font(theme::semi(15.5)).color(p.text));
                ui.add_space(-8.0);
                ui.label(RichText::new(t.tagline).font(theme::reg(12.0)).color(p.muted));
            });
        });
        ui.add_space(18.0);

        let dragging = DragAndDrop::has_payload_of_type::<LinkDrag>(ui.ctx());
        ui.spacing_mut().item_spacing.y = 2.0;
        theme::caps(ui, t.library, p);
        ui.add_space(4.0);
        let total = st.lib.links.len();
        if kit::nav_row(ui, Id::unique("nav-all"), kit::NavRow { icon: kit::icon_link, label: t.all_links, count: Some(total), active: self.page == Page::All, drop_hover: false }, p).clicked() {
            self.page = Page::All;
        }
        let unsorted = st.count_in(None);
        let resp = ui.push_id("nav-unsorted-row", |ui| {
            let hover = dragging && ui.rect_contains_pointer(Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 36.0)));
            kit::nav_row(
                ui,
                Id::unique("nav-unsorted"),
                kit::NavRow { icon: kit::icon_inbox, label: t.no_folder, count: Some(unsorted), active: self.page == Page::Unsorted, drop_hover: hover },
                p,
            )
        });
        let resp = resp.inner;
        if resp.clicked() {
            self.page = Page::Unsorted;
        }
        if let Some(d) = resp.dnd_release_payload::<LinkDrag>() {
            st.move_link(d.0, None);
        }

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            theme::caps(ui, t.folders, p);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::hover());
                if kit::icon_button(ui, r, Id::unique("add-folder"), kit::icon_plus, t.new_folder, false, p).clicked() {
                    let id = st.add_folder(t.new_folder);
                    self.rename = Some(Rename { id, text: st.folder_name(Some(id)), focus_pending: true });
                    self.page = Page::Folder(id);
                }
            });
        });
        ui.add_space(4.0);

        // Folders scroll, "Settings" stays at the bottom.
        let bottom_h = 46.0;
        let avail = ui.available_height() - bottom_h;
        ScrollArea::vertical().id_salt("folders").max_height(avail.max(60.0)).auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if st.lib.folders.is_empty() {
                ui.add_space(2.0);
                theme::hint(ui, t.folders_empty_hint, p);
            }
            let folders: Vec<(FolderId, String)> = st.lib.folders.iter().map(|f| (f.id, f.name.clone())).collect();
            for (idx, (fid, name)) in folders.into_iter().enumerate() {
                if let Some(rn) = self.rename.as_mut().filter(|r| r.id == fid) {
                    let id = Id::unique(("rename", fid));
                    let resp = kit::text_field(ui, &mut rn.text, t.folder_name_ph, Some(kit::icon_folder), id, p);
                    if rn.focus_pending {
                        resp.request_focus();
                        // Select everything so a new name can be typed at once.
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                            let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(rn.text.chars().count()));
                            state.cursor.set_char_range(Some(range));
                            state.store(ui.ctx(), id);
                        }
                        rn.focus_pending = false;
                    }
                    let cancel = ui.input(|i| i.key_pressed(Key::Escape));
                    if cancel {
                        self.rename = None;
                    } else if resp.lost_focus() {
                        let text = rn.text.clone();
                        st.rename_folder(fid, &text);
                        self.rename = None;
                    }
                    continue;
                }
                let count = st.count_in(Some(fid));
                let top = ui.cursor().min;
                let hover = dragging && ui.rect_contains_pointer(Rect::from_min_size(top, vec2(ui.available_width(), 36.0)));
                let resp = kit::nav_row(
                    ui,
                    Id::unique(("nav-folder", fid)),
                    kit::NavRow { icon: kit::icon_folder, label: &name, count: Some(count), active: self.page == Page::Folder(fid), drop_hover: hover },
                    p,
                );
                if resp.clicked() {
                    self.page = Page::Folder(fid);
                }
                if resp.double_clicked() {
                    self.rename = Some(Rename { id: fid, text: name.clone(), focus_pending: true });
                }
                if let Some(d) = resp.dnd_release_payload::<LinkDrag>() {
                    st.move_link(d.0, Some(fid));
                }
                let is_default = st.lib.settings.default_folder == Some(fid);
                let nfolders = st.lib.folders.len();
                resp.context_menu(|ui| {
                    ui.set_min_width(220.0);
                    if kit::menu_item(ui, t.rename, Some(kit::icon_edit), false, false, p).clicked() {
                        self.rename = Some(Rename { id: fid, text: name.clone(), focus_pending: true });
                        ui.close();
                    }
                    if kit::menu_item(ui, t.save_here, Some(kit::icon_inbox), is_default, false, p).clicked() {
                        st.lib.settings.default_folder = if is_default { None } else { Some(fid) };
                        st.touch();
                        ui.close();
                    }
                    if idx > 0 && kit::menu_item(ui, t.move_up, None, false, false, p).clicked() {
                        st.move_folder(idx, idx - 1);
                        ui.close();
                    }
                    if idx + 1 < nfolders && kit::menu_item(ui, t.move_down, None, false, false, p).clicked() {
                        st.move_folder(idx, idx + 1);
                        ui.close();
                    }
                    kit::menu_separator(ui, p);
                    if kit::menu_item(ui, t.delete_folder, Some(kit::icon_trash), false, true, p).clicked() {
                        self.confirm_delete_folder = Some(fid);
                        ui.close();
                    }
                });
            }
        });

        // Bottom of the panel.
        let rest = ui.available_rect_before_wrap();
        let r = Rect::from_min_max(pos2(rest.left(), rest.bottom() - 38.0), pos2(rest.right(), rest.bottom()));
        if let UpdateState::Available { version, .. } = self.updater.state() {
            let r2 = Rect::from_min_max(pos2(rest.left(), r.top() - 40.0), pos2(rest.right(), r.top() - 6.0));
            let mut up = ui.new_child(egui::UiBuilder::new().max_rect(r2));
            if theme::button(&mut up, &t.update_available.replace("{v}", &version), Btn::Soft, p).clicked() {
                self.page = Page::Settings;
            }
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
        if kit::nav_row(&mut child, Id::unique("nav-settings"), kit::NavRow { icon: kit::icon_gear, label: t.settings, count: None, active: self.page == Page::Settings, drop_hover: false }, p).clicked() {
            self.page = Page::Settings;
        }
    }

    // ───────────────────────── library ─────────────────────────

    fn library(&mut self, ui: &mut Ui, st: &mut Store, p: &Pal) {
        let t = strings::s();
        let now = Self::now(ui);
        let ids = self.visible_ids(st);
        let (title, total) = match self.page {
            Page::All => (t.all_links.to_string(), st.lib.links.len()),
            Page::Unsorted => (t.no_folder.to_string(), st.count_in(None)),
            Page::Folder(f) => (st.folder_name(Some(f)), st.count_in(Some(f))),
            Page::Settings => unreachable!(),
        };

        // Heading and the "Add" button.
        let head = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover()).0;
        ui.painter().text(head.left_top() + vec2(0.0, 0.0), Align2::LEFT_TOP, &title, theme::bold(26.0), p.text);
        let sub = if self.query.trim().is_empty() {
            if total == 0 { t.empty_now.into() } else { pl(total, &t.links) }
        } else {
            t.found.replace("{n}", &ids.len().to_string()).replace("{total}", &total.to_string())
        };
        ui.painter().text(head.left_top() + vec2(1.0, 34.0), Align2::LEFT_TOP, sub, theme::reg(13.5), p.muted);
        let brect = Rect::from_min_size(pos2(head.right() - 220.0, head.top() + 4.0), vec2(220.0, 42.0));
        if theme::gradient_button(ui, brect, Id::unique("add-link"), t.add_link, Some(kit::icon_plus), true, &self.tex, p).clicked() {
            let clip = self.hub.clip_url.lock().unwrap().clone().filter(|u| st.find_url(u).is_none());
            self.open_editor(st, None, clip);
        }
        ui.add_space(12.0);

        // A link in the clipboard that is not in the library yet.
        let clip = self.hub.clip_url.lock().unwrap().clone().filter(|u| st.find_url(u).is_none());
        if let Some(url) = clip {
            let r = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover()).0;
            let painter = ui.painter();
            painter.rect(r, 12.0, p.accent_soft, Stroke::NONE, StrokeKind::Inside);
            kit::icon_clipboard(painter, pos2(r.left() + 22.0, r.center().y), p.accent);
            let bw = 120.0;
            let clip_r = Rect::from_min_max(pos2(r.left() + 42.0, r.top()), pos2(r.right() - bw - 130.0, r.bottom()));
            let mut job = LayoutJob::default();
            job.append(t.in_clipboard, 0.0, TextFormat { font_id: theme::reg(13.5), color: p.muted, ..Default::default() });
            job.append(&urlx::pretty(&url), 0.0, TextFormat { font_id: theme::semi(13.5), color: p.text, ..Default::default() });
            job.wrap = TextWrapping { max_width: clip_r.width(), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
            let g = painter.layout_job(job);
            painter.galley(pos2(clip_r.left(), r.center().y - g.size().y / 2.0), g, p.text);
            let save_r = Rect::from_min_size(pos2(r.right() - bw - 8.0, r.top() + 7.0), vec2(bw, 32.0));
            if theme::button_at(ui, save_r, Id::unique("clip-save"), t.save, Btn::Soft, p).clicked() {
                self.quick_save(st, &url, now);
            }
            let edit_r = Rect::from_min_size(pos2(save_r.left() - 126.0, r.top() + 7.0), vec2(120.0, 32.0));
            if theme::button_at(ui, edit_r, Id::unique("clip-edit"), t.with_description, Btn::Link, p).clicked() {
                self.open_editor(st, None, Some(url.clone()));
            }
            ui.add_space(10.0);
        }

        // Search and sorting.
        ui.horizontal(|ui| {
            let sort_w = 210.0;
            ui.allocate_ui(vec2(ui.available_width() - sort_w - 8.0, 38.0), |ui| {
                let resp = kit::text_field(ui, &mut self.query, t.search_ph, Some(kit::icon_search), Id::unique("search"), p);
                if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    self.query.clear();
                }
                if !self.query.is_empty() {
                    let r = Rect::from_center_size(pos2(resp.rect.right() - 10.0, resp.rect.center().y), vec2(26.0, 26.0));
                    if kit::icon_button(ui, r, Id::unique("search-clear"), kit::icon_close, t.clear_esc, false, p).clicked() {
                        self.query.clear();
                    }
                }
            });
            let sort = st.lib.settings.sort;
            let resp = kit::dropdown_button(ui, sort.label(), Some(kit::icon_sort), sort_w, Id::unique("sort"), p);
            Popup::menu(&resp).frame(kit::menu_frame(p)).width(sort_w).show(|ui| {
                for s in Sort::ALL {
                    if kit::menu_item(ui, s.label(), None, s == sort, false, p).clicked() {
                        st.lib.settings.sort = s;
                        st.touch();
                        ui.close();
                    }
                }
            });
        });
        ui.add_space(12.0);

        if ids.is_empty() {
            self.empty_state(ui, st, p, total);
            return;
        }

        let q = Query::new(&self.query);
        let show_folder = self.page == Page::All;
        let folders: Vec<(FolderId, String)> = st.lib.folders.iter().map(|f| (f.id, f.name.clone())).collect();
        let mut action: Option<(LinkId, &'static str, Option<Option<FolderId>>)> = None;
        ScrollArea::vertical().id_salt(("links", self.page_key())).auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.set_width(ui.available_width() - 6.0);
            for id in &ids {
                let Some(link) = st.link(*id) else { continue };
                if let Some(a) = link_card(ui, link, &q, show_folder, st.folder_name(link.folder), &folders, p) {
                    action = Some((*id, a.0, a.1));
                }
            }
            ui.add_space(8.0);
        });
        if let Some((id, act, folder)) = action {
            match act {
                "open" => {
                    if let Some(l) = st.link(id) {
                        win::open_url(&l.url);
                    }
                }
                "copy" => {
                    if let Some(l) = st.link(id) {
                        ui.ctx().copy_text(l.url.clone());
                        self.toast = Some(Toast { text: t.copied.into(), undo: None, edit: None, at: now });
                    }
                }
                "edit" => self.open_editor(st, Some(id), None),
                "delete" => self.delete_link(st, id, now),
                "move" => {
                    if let Some(f) = folder {
                        st.move_link(id, f);
                    }
                }
                _ => {}
            }
        }
    }

    fn page_key(&self) -> String {
        format!("{:?}", self.page)
    }

    fn empty_state(&mut self, ui: &mut Ui, st: &mut Store, p: &Pal, total: usize) {
        let r = ui.allocate_exact_size(vec2(ui.available_width(), 170.0), Sense::hover()).0;
        let stroke = Stroke::new(1.0, p.line);
        let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        for k in 0..4 {
            ui.painter().extend(egui::Shape::dashed_line(&[c[k], c[k + 1]], stroke, 6.0, 4.0));
        }
        let t = strings::s();
        let (head, body) = if !self.query.trim().is_empty() {
            (t.nothing_found.to_string(), t.nothing_found_hint.to_string())
        } else if total == 0 && st.lib.links.is_empty() {
            (t.empty_lib.to_string(), t.empty_lib_hint.to_string())
        } else {
            (t.empty_folder.to_string(), t.empty_folder_hint.to_string())
        };
        ui.painter().text(r.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, head, theme::semi(16.0), p.text);
        let g = ui.painter().layout(body, theme::reg(13.5), p.muted, (r.width() - 60.0).min(520.0));
        ui.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.center().y + 6.0), g, p.muted);
    }

    // ───────────────────────── settings ─────────────────────────

    fn settings(&mut self, ui: &mut Ui, st: &mut Store, p: &Pal) {
        let t = strings::s();
        let nt = nimvoro::strings(strings::lang());
        ScrollArea::vertical().id_salt("settings").auto_shrink([false; 2]).show(ui, |ui| {
            let w = ui.available_width().min(680.0);
            ui.set_width(w);
            ui.label(RichText::new(t.settings).font(theme::bold(26.0)).color(p.text));
            ui.add_space(-4.0);
            ui.label(RichText::new(t.settings_sub).font(theme::reg(13.5)).color(p.muted));
            ui.add_space(14.0);

            let mut changed = false;
            let mut hotkey_changed = false;

            theme::caps(ui, t.sec_appearance, p);
            ui.add_space(2.0);
            kit::card_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut lang = strings::lang();
                if i18n::language_row(ui, &mut lang, p) {
                    strings::set_lang(lang);
                    st.lib.settings.language = Some(lang.code().to_string());
                    changed = true;
                    // The tray thread refreshes its tooltip.
                    hotkey_changed = true;
                }
                theme::divider(ui, p);
                let mut mode = st.lib.settings.theme;
                if i18n::theme_row(ui, &mut mode, strings::lang(), p) {
                    st.lib.settings.theme = mode;
                    theme::apply(ui.ctx(), mode);
                    changed = true;
                }
            });
            ui.add_space(16.0);

            theme::caps(ui, t.sec_tray, p);
            ui.add_space(2.0);
            kit::card_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 380.0).max(160.0));
                        ui.label(RichText::new(t.click_title).font(theme::reg(14.0)).color(p.text));
                        ui.add_space(-6.0);
                        theme::hint(ui, t.click_hint, p);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let sel = if st.lib.settings.click == ClickAction::Save { 0 } else { 1 };
                        // In right_to_left the segments are drawn as usual: the layout only affects the position.
                        if let Some(i) = kit::segmented(ui, &[t.click_save, t.click_open], sel, p) {
                            st.lib.settings.click = if i == 0 { ClickAction::Save } else { ClickAction::Open };
                            changed = true;
                            hotkey_changed = true;
                        }
                    });
                });
                theme::divider(ui, p);
                let mut hk = st.lib.settings.hotkey;
                if kit::switch_row(ui, p, t.hotkey_title, t.hotkey_hint, &mut hk) {
                    st.lib.settings.hotkey = hk;
                    changed = true;
                    hotkey_changed = true;
                }
                if hk && !win::HOTKEY_OK.load(std::sync::atomic::Ordering::SeqCst) {
                    ui.label(RichText::new(t.hotkey_busy).font(theme::reg(12.5)).color(kit::DANGER));
                }
                theme::divider(ui, p);
                let mut n = st.lib.settings.notify;
                if kit::switch_row(ui, p, t.notify_title, t.notify_hint, &mut n) {
                    st.lib.settings.notify = n;
                    changed = true;
                }
            });
            ui.add_space(16.0);

            theme::caps(ui, t.sec_new, p);
            ui.add_space(2.0);
            kit::card_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut f = st.lib.settings.fetch_titles;
                if kit::switch_row(
                    ui,
                    p,
                    t.fetch_title,
                    t.fetch_hint,
                    &mut f,
                ) {
                    st.lib.settings.fetch_titles = f;
                    changed = true;
                }
                theme::divider(ui, p);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 250.0).max(160.0));
                        ui.label(RichText::new(t.default_folder).font(theme::reg(14.0)).color(p.text));
                        ui.add_space(-6.0);
                        theme::hint(ui, t.default_folder_hint, p);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let cur = st.lib.settings.default_folder;
                        let label = st.folder_name(cur);
                        let resp = kit::dropdown_button(ui, &label, Some(kit::icon_folder), 240.0, Id::unique("default-folder"), p);
                        let folders: Vec<(FolderId, String)> = st.lib.folders.iter().map(|f| (f.id, f.name.clone())).collect();
                        Popup::menu(&resp).frame(kit::menu_frame(p)).width(240.0).show(|ui| {
                            if kit::menu_item(ui, t.no_folder, Some(kit::icon_inbox), cur.is_none(), false, p).clicked() {
                                st.lib.settings.default_folder = None;
                                changed = true;
                                ui.close();
                            }
                            for (id, name) in &folders {
                                if kit::menu_item(ui, name, Some(kit::icon_folder), cur == Some(*id), false, p).clicked() {
                                    st.lib.settings.default_folder = Some(*id);
                                    changed = true;
                                    ui.close();
                                }
                            }
                        });
                    });
                });
            });
            ui.add_space(16.0);

            theme::caps(ui, t.sec_system, p);
            ui.add_space(2.0);
            kit::card_frame(p).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let mut a = self.autostart;
                if kit::switch_row(ui, p, t.autostart_title, t.autostart_hint, &mut a) {
                    if win::set_autostart(a) {
                        self.autostart = a;
                        self.autostart_error = false;
                    } else {
                        self.autostart_error = true;
                    }
                }
                if self.autostart_error {
                    ui.label(RichText::new(t.autostart_error).font(theme::reg(12.5)).color(kit::DANGER));
                }
                theme::divider(ui, p);
                ui.label(RichText::new(t.lib_file).font(theme::reg(14.0)).color(p.text));
                ui.add_space(-6.0);
                let path = st.path().cloned();
                let path_text = path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t.unknown.into());
                theme::hint(ui, &format!("{path_text} · {}, {}", pl(st.lib.links.len(), &t.links), pl(st.lib.folders.len(), &t.folders_pl)), p);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::button(ui, t.open_folder, Btn::Soft, p).clicked()
                        && let Some(dir) = path.as_ref().and_then(|p| p.parent())
                    {
                        let _ = std::fs::create_dir_all(dir);
                        win::open_folder(dir);
                    }
                    if theme::button(ui, t.export, Btn::Ghost, p)
                        .on_hover_text(t.export_tip)
                        .clicked()
                    {
                        self.after = Some(After::Export);
                    }
                });
                theme::divider(ui, p);
                let mut cu = st.lib.settings.check_updates;
                if kit::switch_row(ui, p, nt.check_on_start, nt.check_on_start_hint, &mut cu) {
                    st.lib.settings.check_updates = cu;
                    changed = true;
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    theme::hint(ui, &self.updater.status_text(strings::lang()), p);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let UpdateState::Available { .. } = self.updater.state() {
                            if theme::button(ui, nt.update_btn, Btn::Soft, p).clicked() && self.updater.apply() {
                                st.save_now();
                                win::post_tray(win::WM_APP_QUIT);
                            }
                        } else if !self.updater.is_checking() && theme::button(ui, nt.check_now, Btn::Link, p).clicked() {
                            let c = ui.ctx().clone();
                            self.updater.check(move || c.request_repaint());
                        }
                    });
                });
                theme::divider(ui, p);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 160.0).max(160.0));
                        ui.label(RichText::new(t.quit_title).font(theme::reg(14.0)).color(p.text));
                        ui.add_space(-6.0);
                        theme::hint(ui, t.quit_hint, p);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::button(ui, t.quit, Btn::Ghost, p).clicked() {
                            st.save_now();
                            win::post_tray(win::WM_APP_QUIT);
                        }
                    });
                });
            });
            ui.add_space(16.0);
            theme::hint(ui, t.keys_hint, p);
            ui.add_space(12.0);

            if changed {
                st.touch();
            }
            if hotkey_changed {
                win::post_tray(win::WM_APP_HOTKEY);
            }
        });
    }

    // ───────────────────────── overlays ─────────────────────────

    fn editor_modal(&mut self, ctx: &egui::Context, st: &mut Store, p: &Pal, now: f64) {
        let t = strings::s();
        let Some(ed) = self.editor.as_mut() else { return };
        // Result of "Get from site".
        if let Some(slot) = &ed.fetch {
            let res = slot.lock().unwrap().take();
            match res {
                Some(Some(t)) => {
                    ed.title = t;
                    ed.fetch = None;
                }
                Some(None) => {
                    ed.error = Some(t.err_fetch.into());
                    ed.fetch = None;
                }
                None => ctx.request_repaint_after(Duration::from_millis(100)),
            }
        }

        let mut save = false;
        let mut delete = false;
        let mut close = false;
        let folders: Vec<(FolderId, String)> = st.lib.folders.iter().map(|f| (f.id, f.name.clone())).collect();
        let folder_label = st.folder_name(ed.folder);
        let fetch_on = st.lib.settings.fetch_titles;

        let modal = egui::Modal::new(Id::unique("editor"))
            .backdrop_color(Color32::from_black_alpha(130))
            .frame(
                Frame::new()
                    .fill(p.card)
                    .stroke(Stroke::new(1.0, p.line))
                    .corner_radius(20)
                    .inner_margin(Margin::symmetric(28, 24))
                    .shadow(theme::shadow(20, 60, Color32::from_black_alpha(90))),
            )
            .show(ctx, |ui| {
                ui.set_width(480.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if ed.id.is_some() { t.link } else { t.new_link }).font(theme::bold(21.0)).color(p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                        if kit::icon_button(ui, r, Id::unique("ed-close"), kit::icon_close, t.close_esc, false, p).clicked() {
                            close = true;
                        }
                        if let Some(id) = ed.id {
                            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                            if kit::icon_button(ui, r, Id::unique("ed-open"), kit::icon_external, t.open_browser, false, p).clicked() {
                                win::open_url(&ed.url);
                            }
                            let _ = id;
                        }
                    });
                });
                ui.add_space(8.0);

                theme::caps(ui, t.address, p);
                let url_resp = kit::text_field(ui, &mut ed.url, "https://…", Some(kit::icon_link), Id::unique("ed-url"), p);
                if ed.focus_pending {
                    if ed.url.trim().is_empty() {
                        url_resp.request_focus();
                    } else {
                        ui.memory_mut(|m| m.request_focus(Id::unique("ed-title")));
                    }
                    ed.focus_pending = false;
                }
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    theme::caps(ui, t.title, p);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ed.fetch.is_some() {
                            ui.label(RichText::new(t.loading).font(theme::reg(12.5)).color(p.muted));
                        } else if theme::button(ui, t.take_from_site, Btn::Link, p).clicked() {
                            match urlx::extract_url(&ed.url) {
                                Some(u) => {
                                    let slot = Arc::new(Mutex::new(None));
                                    let s2 = slot.clone();
                                    let c2 = ctx.clone();
                                    std::thread::spawn(move || {
                                        *s2.lock().unwrap() = Some(win::fetch_title(&u));
                                        c2.request_repaint();
                                    });
                                    ed.fetch = Some(slot);
                                    ed.error = None;
                                }
                                None => ed.error = Some(t.err_enter_url.into()),
                            }
                        }
                    });
                });
                let hint = if ed.id.is_none() && fetch_on { t.title_auto } else { t.title_ph };
                let title_resp = kit::text_field(ui, &mut ed.title, hint, None, Id::unique("ed-title"), p);
                ui.add_space(6.0);

                theme::caps(ui, t.description, p);
                kit::multiline_field(ui, &mut ed.desc, t.description_ph, 3, Id::unique("ed-desc"), p);
                ui.add_space(6.0);

                theme::caps(ui, t.folder, p);
                let resp = kit::dropdown_button(ui, &folder_label, Some(if ed.folder.is_some() { kit::icon_folder } else { kit::icon_inbox }), ui.available_width(), Id::unique("ed-folder"), p);
                Popup::menu(&resp).frame(kit::menu_frame(p)).width(resp.rect.width()).show(|ui| {
                    if kit::menu_item(ui, t.no_folder, Some(kit::icon_inbox), ed.folder.is_none(), false, p).clicked() {
                        ed.folder = None;
                        ui.close();
                    }
                    for (id, name) in &folders {
                        if kit::menu_item(ui, name, Some(kit::icon_folder), ed.folder == Some(*id), false, p).clicked() {
                            ed.folder = Some(*id);
                            ui.close();
                        }
                    }
                });

                if let Some(err) = &ed.error {
                    ui.add_space(2.0);
                    ui.label(RichText::new(err).font(theme::reg(13.0)).color(kit::DANGER));
                }
                ui.add_space(16.0);

                let enter = (url_resp.lost_focus() || title_resp.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
                let ctrl_enter = ui.input(|i| i.modifiers.command && i.key_pressed(Key::Enter));
                if enter || ctrl_enter {
                    save = true;
                }

                let h = 42.0;
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
                let save_r = Rect::from_min_max(pos2(row.right() - 150.0, row.top()), row.max);
                let cancel_r = Rect::from_min_size(pos2(save_r.left() - 118.0, row.top()), vec2(110.0, h));
                if theme::gradient_button(ui, save_r, Id::unique("ed-save"), if ed.id.is_some() { t.save } else { t.add }, None, true, &self.tex, p).clicked() {
                    save = true;
                }
                if theme::button_at(ui, cancel_r, Id::unique("ed-cancel"), t.cancel, Btn::Ghost, p).clicked() {
                    close = true;
                }
                if ed.id.is_some() {
                    let del_r = Rect::from_min_size(row.min, vec2(100.0, h));
                    let resp = ui.interact(del_r, Id::unique("ed-del"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
                    let col = if resp.hovered() { kit::DANGER } else { kit::DANGER.gamma_multiply(0.85) };
                    kit::icon_trash(ui.painter(), pos2(del_r.left() + 10.0, del_r.center().y), col);
                    ui.painter().text(pos2(del_r.left() + 24.0, del_r.center().y), Align2::LEFT_CENTER, t.delete, theme::semi(13.5), col);
                    if resp.clicked() {
                        delete = true;
                    }
                }
            });
        if modal.should_close() {
            close = true;
        }

        if delete {
            if let Some(id) = ed.id {
                self.editor = None;
                self.delete_link(st, id, now);
            }
            return;
        }
        if save {
            let Some(url) = urlx::extract_url(&ed.url) else {
                ed.error = Some(t.err_not_url.into());
                return;
            };
            if let Some(other) = st.find_url(&url).filter(|o| Some(*o) != ed.id) {
                let title = st.link(other).map(|l| l.title.clone()).unwrap_or_default();
                ed.error = Some(t.err_duplicate.replace("{t}", &win::ellipsize(&title, 50)));
                return;
            }
            let title = ed.title.trim().to_string();
            let desc = ed.desc.trim().to_string();
            match ed.id {
                Some(id) => {
                    let url_changed = st.link(id).is_some_and(|l| l.url != url);
                    let title_changed = title != ed.orig_title;
                    if let Some(l) = st.link_mut(id) {
                        l.url = url.clone();
                        if title.is_empty() {
                            l.title = urlx::fallback_title(&url);
                            l.auto_title = true;
                        } else if title_changed {
                            l.title = title;
                            l.auto_title = false;
                        }
                        l.description = desc;
                        l.folder = ed.folder;
                    }
                    st.touch();
                    if url_changed && fetch_on && st.link(id).is_some_and(|l| l.auto_title) {
                        win::fetch_title_async(self.store.clone(), ctx.clone(), id, url, None);
                    }
                }
                None => {
                    if let Added::New(id) = st.add_url(&url, Some(ed.folder)) {
                        if let Some(l) = st.link_mut(id) {
                            if !title.is_empty() {
                                l.title = title;
                                l.auto_title = false;
                            }
                            l.description = desc;
                        }
                        st.save_now();
                        if fetch_on && st.link(id).is_some_and(|l| l.auto_title) {
                            win::fetch_title_async(self.store.clone(), ctx.clone(), id, url, None);
                        }
                    }
                }
            }
            self.editor = None;
            return;
        }
        if close {
            self.editor = None;
        }
    }

    fn confirm_modal(&mut self, ctx: &egui::Context, st: &mut Store, p: &Pal) {
        let t = strings::s();
        let Some(fid) = self.confirm_delete_folder else { return };
        let Some(name) = st.folder(fid).map(|f| f.name.clone()) else {
            self.confirm_delete_folder = None;
            return;
        };
        let n = st.count_in(Some(fid));
        let mut close = false;
        let mut confirm = false;
        let modal = egui::Modal::new(Id::unique("confirm"))
            .backdrop_color(Color32::from_black_alpha(130))
            .frame(Frame::new().fill(p.card).stroke(Stroke::new(1.0, p.line)).corner_radius(20).inner_margin(Margin::symmetric(28, 24)))
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(RichText::new(t.delete_folder_q.replace("{name}", &name)).font(theme::bold(19.0)).color(p.text));
                let body = if n == 0 { t.folder_is_empty.to_string() } else { t.links_move_out.replace("{links}", &pl(n, &t.links)) };
                theme::hint(ui, &body, p);
                ui.add_space(16.0);
                let h = 40.0;
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
                let del_r = Rect::from_min_max(pos2(row.right() - 150.0, row.top()), row.max);
                let cancel_r = Rect::from_min_size(pos2(del_r.left() - 118.0, row.top()), vec2(110.0, h));
                let resp = ui.interact(del_r, Id::unique("cf-del"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
                let fill = if resp.hovered() { theme::mix(kit::DANGER, Color32::BLACK, 0.1) } else { kit::DANGER };
                ui.painter().rect_filled(del_r, 12.0, fill);
                ui.painter().text(del_r.center(), Align2::CENTER_CENTER, t.delete, theme::semi(14.5), Color32::WHITE);
                if resp.clicked() {
                    confirm = true;
                }
                if theme::button_at(ui, cancel_r, Id::unique("cf-cancel"), t.cancel, Btn::Ghost, p).clicked() {
                    close = true;
                }
            });
        if modal.should_close() {
            close = true;
        }
        if confirm {
            st.delete_folder(fid);
            if self.page == Page::Folder(fid) {
                self.page = Page::All;
            }
            close = true;
        }
        if close {
            self.confirm_delete_folder = None;
        }
    }

    fn toast(&mut self, ctx: &egui::Context, st: &mut Store, p: &Pal, now: f64) {
        let tr = strings::s();
        let Some(t) = &self.toast else { return };
        let age = now - t.at;
        let life = 5.0;
        if age > life {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        let a = (age / 0.2).min(1.0).min((life - age) / 0.3) as f32;
        let screen = ctx.content_rect();
        let mut undo = false;
        let mut edit = None;
        egui::Area::new(Id::unique("toast"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -24.0 + (1.0 - a) * 12.0))
            .show(ctx, |ui| {
                ui.multiply_opacity(a);
                Frame::new()
                    .fill(if p.dark { theme::hex(0x2a2f3a) } else { theme::hex(0x1f232b) })
                    .corner_radius(14)
                    .inner_margin(Margin { left: 18, right: 10, top: 8, bottom: 8 })
                    .shadow(theme::shadow(8, 24, Color32::from_black_alpha(70)))
                    .show(ui, |ui| {
                        ui.set_max_width(screen.width() - 80.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&t.text).font(theme::reg(13.5)).color(Color32::WHITE));
                            ui.add_space(6.0);
                            let label = if t.undo.is_some() { Some(tr.undo) } else if t.edit.is_some() { Some(tr.edit) } else { None };
                            if let Some(label) = label {
                                let g = ui.painter().layout_no_wrap(label.into(), theme::semi(13.5), Color32::WHITE);
                                let (r, resp) = ui.allocate_exact_size(g.size() + vec2(20.0, 12.0), Sense::click());
                                let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
                                ui.painter().rect_filled(r, 8.0, Color32::from_white_alpha(if resp.hovered() { 40 } else { 22 }));
                                ui.painter().galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
                                if resp.clicked() {
                                    if t.undo.is_some() {
                                        undo = true;
                                    } else {
                                        edit = t.edit;
                                    }
                                }
                            } else {
                                ui.add_space(8.0);
                            }
                        });
                    });
            });
        if undo {
            if let Some(l) = self.toast.take().and_then(|t| t.undo) {
                st.restore_link(l);
            }
        }
        if let Some(id) = edit {
            self.toast = None;
            self.open_editor(st, Some(id), None);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context, st: &mut Store, now: f64) {
        if self.editor.is_some() || self.confirm_delete_folder.is_some() {
            return;
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
            if self.page == Page::Settings {
                self.page = Page::All;
            }
            ctx.memory_mut(|m| m.request_focus(Id::unique("search")));
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::N)) {
            let clip = self.hub.clip_url.lock().unwrap().clone().filter(|u| st.find_url(u).is_none());
            self.open_editor(st, None, clip);
        }
        if !typing {
            let pasted = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Paste(s) = e { Some(s.clone()) } else { None }));
            if let Some(text) = pasted {
                match urlx::extract_url(&text) {
                    Some(url) => self.quick_save(st, &url, now),
                    None => self.toast = Some(Toast { text: strings::s().no_link_in_clipboard.into(), undo: None, edit: None, at: now }),
                }
            }
        }
    }

    fn draw(&mut self, ui: &mut Ui, st: &mut Store, p: &Pal) {
        let ctx = ui.ctx().clone();
        let now = Self::now(ui);
        if let Page::Folder(f) = self.page
            && st.folder(f).is_none()
        {
            self.page = Page::All;
        }
        self.handle_keys(&ctx, st, now);

        egui::Panel::left("sidebar")
            .exact_size(264.0)
            .resizable(false)
            .frame(Frame::new().fill(p.sidebar).inner_margin(Margin { left: 16, right: 16, top: 20, bottom: 14 }))
            .show(ui, |ui| self.sidebar(ui, st, p));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin { left: 32, right: 26, top: 22, bottom: 0 }))
            .show(ui, |ui| {
                if let Some(err) = st.load_error.clone().map(|e| strings::s().library_broken.replace("{e}", &e.detail).replace("{name}", &e.backup)) {
                    let r = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover()).0;
                    ui.painter().rect_filled(r, 10.0, kit::DANGER.gamma_multiply(0.15));
                    ui.painter().text(pos2(r.left() + 14.0, r.center().y), Align2::LEFT_CENTER, win::ellipsize(&err, 120), theme::reg(13.0), kit::DANGER);
                    if ui.interact(r, Id::unique("load-err"), Sense::click()).clicked() {
                        st.load_error = None;
                    }
                    ui.add_space(10.0);
                }
                if self.page == Page::Settings {
                    self.settings(ui, st, p);
                } else {
                    self.library(ui, st, p);
                }
            });

        // Ghost of the dragged link next to the cursor.
        if let Some(d) = DragAndDrop::payload::<LinkDrag>(&ctx)
            && let Some(pos) = ctx.pointer_interact_pos()
            && let Some(l) = st.link(d.0)
        {
            ctx.set_cursor_icon(CursorIcon::Grabbing);
            let painter = ctx.layer_painter(egui::LayerId::new(Order::Tooltip, Id::unique("drag-ghost")));
            let g = painter.layout_no_wrap(win::ellipsize(&l.title, 40), theme::semi(13.0), Color32::WHITE);
            let r = Rect::from_min_size(pos + vec2(14.0, 10.0), g.size() + vec2(24.0, 14.0));
            painter.add(theme::shadow(6, 18, Color32::from_black_alpha(60)).as_shape(r, 10.0));
            theme::gradient_rect(&painter, r, 10.0, &self.tex.accent);
            painter.galley(r.min + vec2(12.0, 7.0), g, Color32::WHITE);
        }

        self.editor_modal(&ctx, st, p, now);
        self.confirm_modal(&ctx, st, p);
        self.toast(&ctx, st, p, now);
    }
}

/// Link card. Returns the action: open / copy / edit / delete / move.
fn link_card(
    ui: &mut Ui,
    l: &Link,
    q: &Query,
    show_folder: bool,
    folder_name: String,
    folders: &[(FolderId, String)],
    p: &Pal,
) -> Option<(&'static str, Option<Option<FolderId>>)> {
    let t = strings::s();
    let w = ui.available_width();
    let pad = 14.0;
    let badge = 38.0;
    let x0 = pad + badge + 14.0;
    let right_w = 150.0;
    let text_w = (w - x0 - right_w - pad).max(120.0);

    let desc = l.description.trim();
    let desc_galley = (!desc.is_empty()).then(|| {
        let mut job = highlighted(desc, q, theme::reg(13.0), theme::mix(p.muted, p.text, 0.35), p);
        job.wrap = TextWrapping { max_width: w - x0 - pad - 8.0, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
        ui.painter().layout_job(job)
    });
    let h = 62.0 + desc_galley.as_ref().map_or(0.0, |g| g.size().y + 4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let id = Id::unique(("card", l.id));
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    if resp.drag_started() {
        resp.dnd_set_drag_payload(LinkDrag(l.id));
    }
    // Not `resp.hovered()`: over the action buttons the buttons take the hover themselves,
    // the card stops counting as hovered and the buttons vanish before they can be clicked.
    let hovered = ui.rect_contains_pointer(rect) || resp.context_menu_opened();
    let mut out = None;

    let painter = ui.painter();
    let fill = if hovered { theme::mix(p.card, p.accent, if p.dark { 0.06 } else { 0.03 }) } else { p.card };
    let stroke = if hovered { theme::mix(p.line, p.accent, 0.45) } else { p.line };
    if hovered && !p.dark {
        painter.add(theme::shadow(4, 14, Color32::from_black_alpha(14)).as_shape(rect, 14.0));
    }
    painter.rect(rect, 14.0, fill, Stroke::new(1.0, stroke), StrokeKind::Inside);

    // Site badge.
    let domain = urlx::domain(&l.url);
    let br = Rect::from_min_size(rect.min + vec2(pad, 12.0), vec2(badge, badge));
    let bc = kit::badge_color(&domain);
    painter.rect_filled(br, 11.0, bc);
    painter.text(br.center(), Align2::CENTER_CENTER, kit::badge_letter(&domain), theme::bold(17.0), Color32::WHITE);

    // Title and address.
    let mut tj = highlighted(&l.title, q, theme::semi(14.5), p.text, p);
    tj.wrap = TextWrapping { max_width: text_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let tg = painter.layout_job(tj);
    painter.galley(rect.min + vec2(x0, 11.0), tg, p.text);
    let mut uj = highlighted(&urlx::pretty(&l.url), q, theme::reg(12.5), p.muted, p);
    uj.wrap = TextWrapping { max_width: text_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let ug = painter.layout_job(uj);
    painter.galley(rect.min + vec2(x0, 34.0), ug, p.muted);
    if let Some(g) = desc_galley {
        painter.galley(rect.min + vec2(x0, 58.0), g, p.muted);
    }

    // On the right: date and folder, buttons on hover.
    if hovered {
        let size = 30.0;
        let mut x = rect.right() - pad - size;
        let y = rect.top() + 11.0;
        let buttons: [(IconFn, &str, &'static str, bool); 4] = [
            (kit::icon_trash, t.delete, "delete", true),
            (kit::icon_edit, t.edit_tip, "edit", false),
            (kit::icon_copy, t.copy_link, "copy", false),
            (kit::icon_external, t.open_browser, "open", false),
        ];
        for (icon, tip, act, danger) in buttons {
            let r = Rect::from_min_size(pos2(x, y), vec2(size, size));
            if kit::icon_button(ui, r, id.with(act), icon, tip, danger, p).clicked() {
                out = Some((act, None));
            }
            x -= size + 4.0;
        }
    } else {
        painter.text(pos2(rect.right() - pad, rect.top() + 14.0), Align2::RIGHT_TOP, fmt_date(l.added), theme::reg(12.0), p.muted);
        if show_folder && l.folder.is_some() {
            let g = painter.layout_no_wrap(win::ellipsize(&folder_name, 18), theme::reg(12.0), p.muted);
            let fr = Rect::from_min_size(pos2(rect.right() - pad - g.size().x - 30.0, rect.top() + 34.0), vec2(g.size().x + 30.0, 20.0));
            painter.rect_filled(fr, 10.0, p.hover);
            kit::icon_folder(&painter.with_clip_rect(fr), pos2(fr.left() + 13.0, fr.center().y), p.muted);
            painter.galley(pos2(fr.left() + 24.0, fr.center().y - g.size().y / 2.0), g, p.muted);
        }
    }

    if out.is_none() && resp.clicked() {
        out = Some(("open", None));
    }
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        if kit::menu_item(ui, t.open_browser, Some(kit::icon_external), false, false, p).clicked() {
            out = Some(("open", None));
            ui.close();
        }
        if kit::menu_item(ui, t.copy_link, Some(kit::icon_copy), false, false, p).clicked() {
            out = Some(("copy", None));
            ui.close();
        }
        if kit::menu_item(ui, t.edit_menu, Some(kit::icon_edit), false, false, p).clicked() {
            out = Some(("edit", None));
            ui.close();
        }
        kit::menu_separator(ui, p);
        ui.label(RichText::new(t.move_to).font(theme::semi(11.0)).color(p.muted));
        if kit::menu_item(ui, t.no_folder, Some(kit::icon_inbox), l.folder.is_none(), false, p).clicked() {
            out = Some(("move", Some(None)));
            ui.close();
        }
        for (fid, name) in folders {
            if kit::menu_item(ui, name, Some(kit::icon_folder), l.folder == Some(*fid), false, p).clicked() {
                out = Some(("move", Some(Some(*fid))));
                ui.close();
            }
        }
        kit::menu_separator(ui, p);
        if kit::menu_item(ui, t.delete, Some(kit::icon_trash), false, true, p).clicked() {
            out = Some(("delete", None));
            ui.close();
        }
    });
    out
}

/// Text with the search matches highlighted.
fn highlighted(text: &str, q: &Query, font: egui::FontId, color: Color32, p: &Pal) -> LayoutJob {
    let mut job = LayoutJob::default();
    let normal = TextFormat { font_id: font.clone(), color, ..Default::default() };
    let hl = TextFormat { font_id: font, color: p.accent, background: p.accent_soft, ..Default::default() };
    let mut pos = 0;
    for (s, e) in q.highlights(text) {
        if s > pos {
            job.append(&text[pos..s], 0.0, normal.clone());
        }
        job.append(&text[s..e], 0.0, hl.clone());
        pos = e;
    }
    if pos < text.len() {
        job.append(&text[pos..], 0.0, normal);
    }
    job
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.background_start && self.frames <= 2 {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        // Closing the window sends it to the tray.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            let mut st = self.store.lock().unwrap();
            if !st.lib.settings.tray_hint_shown {
                st.lib.settings.tray_hint_shown = true;
                st.touch();
                drop(st);
                win::notify(strings::s().tray_title, strings::s().tray_body);
            }
        }

        let events: Vec<Event> = std::mem::take(&mut *self.hub.events.lock().unwrap());
        for e in events {
            self.background_start = false;
            self.show_window(ctx);
            if let Event::Edit(id) = e {
                let st = self.store.clone();
                let st = st.lock().unwrap();
                self.toast = None;
                self.open_editor(&st, Some(id), None);
            }
        }

        let mut st = self.store.lock().unwrap();
        st.flush_if_due();
        if st.is_dirty() {
            ctx.request_repaint_after(Duration::from_millis(450));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let p = Pal::of(ui.visuals().dark_mode);
        let store = self.store.clone();
        {
            let mut st = store.lock().unwrap();
            self.draw(ui, &mut st, &p);
        }
        if let Some(After::Export) = self.after.take() {
            let html = bookmarks_html(&store.lock().unwrap());
            if let Some(path) = rfd::FileDialog::new()
                .set_title(strings::s().export_dialog)
                .set_file_name("save-urls-bookmarks.html")
                .add_filter(strings::s().export_filter, &["html"])
                .save_file()
            {
                let ok = std::fs::write(&path, html).is_ok();
                let now = ui.input(|i| i.time);
                self.toast = Some(Toast {
                    text: if ok { strings::s().bookmarks_saved.into() } else { strings::s().write_failed.into() },
                    undo: None,
                    edit: None,
                    at: now,
                });
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Ok(mut st) = self.store.lock() {
            st.save_now();
        }
    }
}
