//! Linux counterpart of `win.rs` with the same public interface: the tray icon (StatusNotifier / AppIndicator through
//! `tray-icon` on its own GTK thread), clipboard polling, the X11 hotkey, notifications, autostart, opening links,
//! downloading page titles (through `curl`) and the single-instance socket.
//!
//! Wayland has no global hotkeys and no tray click events for ordinary apps, so there the same actions are available
//! from the tray menu and from the command line: bind a system shortcut to `SaveUrls --save-clipboard`.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};

use crate::store::{self, Added, LinkId};

pub const WM_APP_QUIT: u32 = 3;
pub const WM_APP_HOTKEY: u32 = 4;

/// Cuts a string to `max` characters with an ellipsis.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max.saturating_sub(1)).collect::<String>() + "…" }
}

// ───────────────────────── window and tray hand-off ─────────────────────────

pub enum Event {
    /// Show the window.
    Show,
    /// Show the window and open a link for editing (not raised on Linux: notifications have no click callback).
    #[allow(dead_code)]
    Edit(LinkId),
}

pub struct Hub {
    pub store: store::Shared,
    pub ctx: egui::Context,
    pub events: Mutex<Vec<Event>>,
    /// The link currently in the clipboard (for the hint in the window and the tray).
    pub clip_url: Mutex<Option<String>>,
}

impl Hub {
    pub fn send(&self, e: Event) {
        self.events.lock().unwrap().push(e);
        self.ctx.request_repaint();
    }
}

static HUB: OnceLock<Arc<Hub>> = OnceLock::new();
static LISTENER: OnceLock<UnixListener> = OnceLock::new();
static HOTKEY_TX: OnceLock<Mutex<std::sync::mpsc::Sender<()>>> = OnceLock::new();
static TRAY_OK: AtomicBool = AtomicBool::new(false);
pub static HOTKEY_OK: AtomicBool = AtomicBool::new(true);
static LAST_SAVED: AtomicU64 = AtomicU64::new(0);

fn hub() -> &'static Arc<Hub> {
    HUB.get().expect("hub")
}

/// Is there a notification-area icon the user can use to get the window back?
/// Without one (GNOME without the AppIndicator extension) closing the window quits the program.
pub fn has_tray() -> bool {
    TRAY_OK.load(Ordering::SeqCst)
}

/// Messages that `win.rs` posts to its tray window.
pub fn post_tray(msg: u32) {
    match msg {
        WM_APP_QUIT => quit(),
        WM_APP_HOTKEY => {
            if let Some(tx) = HOTKEY_TX.get() {
                let _ = tx.lock().unwrap().send(());
            }
        }
        _ => {}
    }
}

fn quit() -> ! {
    if let Some(h) = HUB.get() {
        h.store.lock().unwrap().save_now();
    }
    std::process::exit(0);
}

// ───────────────────────── single instance and command line ─────────────────────────

/// Empty for the installed copy. A portable copy gets its own tag, so it never collides with the installed program.
fn instance_tag() -> String {
    if !store::is_portable() {
        return String::new();
    }
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.display().to_string())).unwrap_or_default();
    let hash = dir.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("-{hash:016x}")
}

fn socket_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        // SAFETY: getuid has no preconditions.
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/save-urls-{}", unsafe { libc::getuid() })));
    let _ = std::fs::create_dir_all(&base);
    base.join(format!("save-urls{}.sock", instance_tag()))
}

fn send_to_running(cmd: &str) -> bool {
    match UnixStream::connect(socket_path()) {
        Ok(mut s) => s.write_all(format!("{cmd}\n").as_bytes()).is_ok(),
        Err(_) => false,
    }
}

/// One instance per user. If the program is already running, show its window and exit.
pub fn single_instance() -> bool {
    if send_to_running("show") {
        return false;
    }
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    if let Ok(l) = UnixListener::bind(&path) {
        let _ = LISTENER.set(l);
    }
    true
}

/// Command line actions: `--save-clipboard` saves the link from the clipboard (the running program does it, or this
/// process when nothing runs). Returns `true` when the process should exit right away.
pub fn handle_cli() -> bool {
    if !std::env::args().any(|a| a == "--save-clipboard") {
        return false;
    }
    if send_to_running("save") {
        return true;
    }
    let Some(url) = clipboard_text().and_then(|t| crate::urlx::extract_url(&t)) else {
        eprintln!("No link in the clipboard");
        return true;
    };
    let mut st = store::Store::load();
    match st.add_url(&url, None) {
        Added::New(_) => eprintln!("Saved: {url}"),
        Added::Exists(_) => eprintln!("Already saved: {url}"),
    }
    st.save_now();
    true
}

fn ipc_loop(listener: &'static UnixListener) {
    for conn in listener.incoming().flatten() {
        let mut line = String::new();
        if BufReader::new(conn).read_line(&mut line).is_ok() {
            match line.trim() {
                "show" => hub().send(Event::Show),
                "save" => save_from_clipboard(false),
                _ => {}
            }
        }
    }
}

pub fn spawn_tray(hub: Arc<Hub>) {
    if HUB.set(hub).is_err() {
        return;
    }
    if let Some(l) = LISTENER.get() {
        std::thread::Builder::new().name("ipc".into()).spawn(move || ipc_loop(l)).expect("ipc thread");
    }
    std::thread::Builder::new().name("clipboard".into()).spawn(clipboard_loop).expect("clipboard thread");
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = HOTKEY_TX.set(Mutex::new(tx));
    std::thread::Builder::new().name("hotkey".into()).spawn(move || hotkey_loop(rx)).expect("hotkey thread");
    std::thread::Builder::new().name("tray".into()).spawn(tray_main).expect("tray thread");
}

// ───────────────────────── clipboard ─────────────────────────

fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// There are no clipboard change events for ordinary apps on X11 and Wayland, so the clipboard is polled.
fn clipboard_loop() {
    let mut clip = arboard::Clipboard::new().ok();
    loop {
        if clip.is_none() {
            clip = arboard::Clipboard::new().ok();
        }
        let url = clip.as_mut().and_then(|c| c.get_text().ok()).and_then(|t| crate::urlx::extract_url(&t));
        let h = hub();
        let changed = {
            let mut c = h.clip_url.lock().unwrap();
            let changed = *c != url;
            *c = url;
            changed
        };
        if changed {
            h.ctx.request_repaint();
        }
        std::thread::sleep(Duration::from_millis(700));
    }
}

// ───────────────────────── saving ─────────────────────────

fn save_from_clipboard(open_if_empty: bool) {
    let h = hub();
    let Some(url) = clipboard_text().and_then(|t| crate::urlx::extract_url(&t)) else {
        LAST_SAVED.store(0, Ordering::SeqCst);
        if open_if_empty {
            h.send(Event::Show);
        } else if h.store.lock().unwrap().lib.settings.notify {
            let t = crate::strings::s();
            notify(t.balloon_no_link, t.balloon_no_link_hint);
        }
        return;
    };
    let (res, notify_on, fetch, title, folder) = {
        let mut st = h.store.lock().unwrap();
        let res = st.add_url(&url, None);
        st.save_now();
        let id = match res {
            Added::New(id) | Added::Exists(id) => id,
        };
        let (title, folder) = st.link(id).map(|l| (l.title.clone(), st.folder_name(l.folder))).unwrap_or_default();
        (res, st.lib.settings.notify, st.lib.settings.fetch_titles, title, folder)
    };
    h.ctx.request_repaint();
    match res {
        Added::New(id) => {
            LAST_SAVED.store(id, Ordering::SeqCst);
            if notify_on {
                let t = crate::strings::s();
                let text = format!("{}\n{} · {}", ellipsize(&crate::urlx::pretty(&url), 80), folder, t.balloon_saved_hint);
                notify(t.balloon_saved, &text);
            }
            if fetch {
                fetch_title_async(h.store.clone(), h.ctx.clone(), id, url, None);
            }
        }
        Added::Exists(id) => {
            LAST_SAVED.store(id, Ordering::SeqCst);
            if notify_on {
                notify(crate::strings::s().balloon_exists, &format!("{}\n{}", ellipsize(&title, 80), folder));
            }
        }
    }
}

/// Downloads the page title in the background and uses it instead of the temporary title.
pub fn fetch_title_async(store: store::Shared, ctx: egui::Context, id: LinkId, url: String, _tray: Option<isize>) {
    std::thread::spawn(move || {
        let Some(title) = fetch_title(&url) else { return };
        let mut st = store.lock().unwrap();
        if st.set_fetched_title(id, &title) {
            st.save_now();
            drop(st);
            ctx.request_repaint();
        }
    });
}

pub fn fetch_title(url: &str) -> Option<String> {
    let (body, ctype) = http_get(url, 768 * 1024)?;
    if !ctype.is_empty() && !ctype.to_ascii_lowercase().contains("html") {
        return None;
    }
    let html = crate::urlx::decode_html(&body, &ctype);
    crate::urlx::parse_title(&html)
}

/// Desktop notification (best effort; may be called from any thread).
pub fn notify(title: &str, text: &str) {
    let (title, text) = (title.to_string(), text.to_string());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().appname("Save URLs").summary(&title).body(&text).show();
    });
}

// ───────────────────────── hotkey ─────────────────────────

fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland")) || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

/// Ctrl+Alt+S through `global-hotkey` on X11. Wayland has no global hotkeys: the flag is cleared, the settings page
/// reports the shortcut as unavailable and the README explains `--save-clipboard`.
fn hotkey_loop(rx: std::sync::mpsc::Receiver<()>) {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyS);
    let manager = if is_wayland() && std::env::var_os("DISPLAY").is_none() { None } else { GlobalHotKeyManager::new().ok() };
    let id = hotkey.id();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.id == id && e.state == HotKeyState::Pressed {
            // Give the user time to release the keys and the source app time to finish the clipboard.
            std::thread::sleep(Duration::from_millis(60));
            save_from_clipboard(false);
        }
    }));
    let mut registered = false;
    let apply = |registered: &mut bool| {
        let want = hub().store.lock().unwrap().lib.settings.hotkey;
        let ok = match &manager {
            // A Wayland session (even with XWayland) never delivers keys to other windows to an X11 grab.
            Some(_) if is_wayland() => !want,
            Some(m) => {
                if *registered {
                    let _ = m.unregister(hotkey);
                    *registered = false;
                }
                if want {
                    *registered = m.register(hotkey).is_ok();
                    *registered
                } else {
                    true
                }
            }
            None => !want,
        };
        HOTKEY_OK.store(ok, Ordering::SeqCst);
        hub().ctx.request_repaint();
    };
    apply(&mut registered);
    while rx.recv().is_ok() {
        apply(&mut registered);
    }
}

// ───────────────────────── tray ─────────────────────────

struct MenuIds {
    save: MenuId,
    open: MenuId,
    quit: MenuId,
    recent: Vec<(MenuId, String)>,
}

static MENU_IDS: Mutex<Option<MenuIds>> = Mutex::new(None);

/// What the menu shows; the menu is rebuilt when this changes.
#[derive(PartialEq, Clone)]
struct MenuState {
    clip: Option<String>,
    exists: bool,
    recent: Vec<(String, String)>,
    lang: crate::i18n::Lang,
}

fn menu_state() -> MenuState {
    let h = hub();
    let clip = h.clip_url.lock().unwrap().clone();
    let st = h.store.lock().unwrap();
    let mut v: Vec<_> = st.lib.links.iter().collect();
    v.sort_by_key(|l| std::cmp::Reverse((l.added, l.id)));
    let exists = clip.as_ref().is_some_and(|u| st.find_url(u).is_some());
    MenuState { clip, exists, recent: v.into_iter().take(8).map(|l| (l.title.clone(), l.url.clone())).collect(), lang: crate::strings::lang() }
}

fn build_menu(s: &MenuState) -> Option<Menu> {
    let t = crate::strings::s();
    let save_label = match &s.clip {
        Some(u) if s.exists => format!("{}: {}", t.menu_exists, ellipsize(&crate::urlx::pretty(u), 38)),
        Some(u) => format!("{}: {}", t.menu_save, ellipsize(&crate::urlx::pretty(u), 38)),
        None => t.menu_save_empty.into(),
    };
    let save = MenuItem::new(save_label, s.clip.is_some() && !s.exists, None);
    let open = MenuItem::new(t.menu_open, true, None);
    let quit = MenuItem::new(t.menu_quit, true, None);
    let sub = Submenu::new(t.menu_recent, true);
    let mut recent = Vec::new();
    for (title, url) in &s.recent {
        let item = MenuItem::new(ellipsize(title, 50), true, None);
        sub.append(&item).ok()?;
        recent.push((item.id().clone(), url.clone()));
    }
    if recent.is_empty() {
        sub.append(&MenuItem::new(t.menu_none, false, None)).ok()?;
    }
    let menu = Menu::new();
    menu.append_items(&[&save, &open, &sub, &PredefinedMenuItem::separator(), &quit]).ok()?;
    *MENU_IDS.lock().unwrap() = Some(MenuIds { save: save.id().clone(), open: open.id().clone(), quit: quit.id().clone(), recent });
    Some(menu)
}

/// Is a StatusNotifier host running? Without one the icon would be invisible.
fn tray_host_present() -> bool {
    let out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            "org.kde.StatusNotifierWatcher",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).contains("true"),
        // gdbus is missing or the bus is unreachable: assume the best.
        _ => true,
    }
}

fn tray_main() {
    if gtk::init().is_err() {
        return;
    }
    let Ok(data) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) else { return };
    let Ok(icon) = Icon::from_rgba(data.rgba, data.width, data.height) else { return };
    let mut state = menu_state();
    let Some(menu) = build_menu(&state) else { return };
    let Ok(tray) = TrayIconBuilder::new().with_icon(icon).with_tooltip(crate::strings::s().tip_idle).with_menu(Box::new(menu)).build() else { return };
    TRAY_OK.store(tray_host_present(), Ordering::SeqCst);

    MenuEvent::set_event_handler(Some(|e: MenuEvent| {
        enum Act {
            Save,
            Open,
            Quit,
            Url(String),
        }
        let act = {
            let ids = MENU_IDS.lock().unwrap();
            let Some(ids) = ids.as_ref() else { return };
            if e.id == ids.save {
                Act::Save
            } else if e.id == ids.open {
                Act::Open
            } else if e.id == ids.quit {
                Act::Quit
            } else if let Some((_, u)) = ids.recent.iter().find(|(id, _)| *id == e.id) {
                Act::Url(u.clone())
            } else {
                return;
            }
        };
        match act {
            Act::Save => {
                std::thread::spawn(|| save_from_clipboard(false));
            }
            Act::Open => hub().send(Event::Show),
            Act::Quit => quit(),
            Act::Url(u) => open_url(&u),
        }
    }));

    gtk::glib::timeout_add_local(Duration::from_millis(1000), move || {
        let now = menu_state();
        if now != state {
            if let Some(menu) = build_menu(&now) {
                tray.set_menu(Some(Box::new(menu)));
                let t = crate::strings::s();
                let tip = match &now.clip {
                    Some(u) => t.tip_click.replace("{u}", &ellipsize(&crate::urlx::pretty(u), 70)),
                    None => t.tip_idle.to_string(),
                };
                let _ = tray.set_tooltip(Some(tip));
            }
            state = now;
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

// ───────────────────────── app window ─────────────────────────

/// Raising the window is done by the `Focus` viewport command; there is nothing extra to do on Linux.
pub fn bring_to_front(_hwnd: isize) {}

// ───────────────────────── shell ─────────────────────────

fn xdg_open(arg: &str) {
    let _ = Command::new("xdg-open").arg(arg).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Opens a link in the default browser. Web addresses only: no local programs.
pub fn open_url(url: &str) {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    let safe = if ["http://", "https://", "ftp://", "mailto:"].iter().any(|s| lower.starts_with(s)) {
        url.to_string()
    } else if lower.contains(':') || lower.starts_with('\\') || lower.starts_with('/') {
        return;
    } else {
        format!("https://{url}")
    };
    xdg_open(&safe);
}

pub fn open_folder(path: &std::path::Path) {
    xdg_open(&path.display().to_string());
}

// ───────────────────────── autostart ─────────────────────────

fn autostart_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart").join("save-urls.desktop"))
}

/// A `.desktop` `Exec` value: the path in double quotes with the reserved characters escaped.
fn exec_quote(path: &str) -> String {
    let mut s = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    s
}

fn autostart_entry() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!(
        "[Desktop Entry]\nType=Application\nName=Save URLs\nComment=A link library that lives in the tray\nExec={} --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec_quote(&exe.display().to_string())
    ))
}

pub fn autostart_enabled() -> bool {
    let (Some(file), Some(entry)) = (autostart_file(), autostart_entry()) else { return false };
    std::fs::read_to_string(file).is_ok_and(|t| t.lines().any(|l| entry.lines().any(|e| e == l && e.starts_with("Exec="))))
}

pub fn set_autostart(on: bool) -> bool {
    let Some(file) = autostart_file() else { return false };
    if on {
        let Some(entry) = autostart_entry() else { return false };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(file, entry).is_ok()
    } else {
        match std::fs::remove_file(file) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    }
}

// ───────────────────────── time ─────────────────────────

pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

pub fn local_time(unix: i64) -> LocalTime {
    // SAFETY: localtime_r fills the zeroed `tm` we pass and reads the `time_t` by pointer.
    let tm = unsafe {
        let t = unix as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    };
    LocalTime { year: tm.tm_year + 1900, month: (tm.tm_mon + 1) as u32, day: tm.tm_mday as u32, hour: tm.tm_hour as u32, minute: tm.tm_min as u32 }
}

// ───────────────────────── HTTP ─────────────────────────

/// The status line and headers of the final response in `buf` (curl `-D -` output, possibly after redirects and
/// `100 Continue` blocks). `None` while the head is still incomplete.
#[derive(Debug, PartialEq)]
struct Head {
    status: u16,
    ctype: String,
    body_start: usize,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn parse_head(buf: &[u8]) -> Option<Head> {
    let mut pos = 0;
    loop {
        if buf.len() < pos + 5 {
            return None;
        }
        if &buf[pos..pos + 5] != b"HTTP/" {
            return None;
        }
        let end = find(buf, b"\r\n\r\n", pos)?;
        let block = String::from_utf8_lossy(&buf[pos..end]).to_string();
        let mut lines = block.lines();
        let first = lines.next().unwrap_or_default();
        let status: u16 = first.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let tunnel = status == 200 && first.to_ascii_lowercase().contains("connection established");
        if status / 100 == 1 || status / 100 == 3 || tunnel {
            pos = end + 4;
            continue;
        }
        let ctype = lines
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default();
        return Some(Head { status, ctype, body_start: end + 4 });
    }
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

/// GET through `curl` (system proxies and certificates). Returns the body (at most `max` bytes) and the Content-Type.
pub fn http_get(url: &str, max: usize) -> Option<(Vec<u8>, String)> {
    let accept_language = match crate::strings::lang() {
        crate::i18n::Lang::En => "en",
        crate::i18n::Lang::Ru => "ru,en;q=0.8",
        crate::i18n::Lang::Uk => "uk,en;q=0.8",
    };
    let mut child = Command::new("curl")
        .args(["-sL", "--compressed", "--connect-timeout", "5", "-m", "12", "--max-redirs", "10", "--proto", "=http,https", "--proto-redir", "=http,https", "-D", "-"])
        .args(["-A", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36"])
        .args(["-H", "Accept: text/html,application/xhtml+xml;q=0.9,*/*;q=0.5"])
        .args(["-H", &format!("Accept-Language: {accept_language}")])
        .args(["--", url])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let mut buf = Vec::new();
    let mut chunk = vec![0u8; 32 * 1024];
    let mut head: Option<Head> = None;
    let result = loop {
        let n = out.read(&mut chunk).unwrap_or(0);
        buf.extend_from_slice(&chunk[..n]);
        if head.is_none() {
            head = parse_head(&buf);
        }
        if let Some(h) = &head {
            if !(200..300).contains(&h.status) {
                break None;
            }
            if !h.ctype.is_empty() && !h.ctype.to_ascii_lowercase().contains("html") {
                break Some((Vec::new(), h.ctype.clone()));
            }
            let body = &buf[h.body_start.min(buf.len())..];
            // The title is usually near the top, so there is no need to read further.
            if n == 0 || body.len() >= max || contains_ci(body, b"</title") {
                break Some((body.to_vec(), h.ctype.clone()));
            }
        } else if n == 0 {
            break None;
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_follows_redirects() {
        let raw = b"HTTP/1.1 301 Moved\r\nLocation: https://x\r\n\r\nHTTP/2 200 \r\ncontent-type: text/html; charset=utf-8\r\n\r\n<html>";
        let h = parse_head(raw).unwrap();
        assert_eq!(h.status, 200);
        assert_eq!(h.ctype, "text/html; charset=utf-8");
        assert_eq!(&raw[h.body_start..], b"<html>");
    }

    #[test]
    fn head_waits_for_the_end_of_the_headers() {
        assert_eq!(parse_head(b"HTTP/1.1 200 OK\r\nContent-Type: text/ht"), None);
        assert_eq!(parse_head(b"HTTP/1.1 301 Moved\r\nLocation: a\r\n\r\n"), None);
        assert_eq!(parse_head(b""), None);
    }

    #[test]
    fn head_reports_errors() {
        let h = parse_head(b"HTTP/1.1 404 Not Found\r\n\r\n").unwrap();
        assert_eq!(h.status, 404);
        assert_eq!(h.ctype, "");
    }

    #[test]
    fn exec_is_quoted() {
        assert_eq!(exec_quote("/opt/my app/SaveUrls"), "\"/opt/my app/SaveUrls\"");
        assert_eq!(exec_quote("/a\"b$c"), "\"/a\\\"b\\$c\"");
    }

    #[test]
    fn local_time_is_sane() {
        let t = local_time(1_700_000_000);
        assert_eq!(t.year, 2023);
        assert!(t.month == 11 && (14..=15).contains(&t.day));
    }

    /// Goes to the internet, so it is run by hand: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn fetches_real_titles() {
        let t = fetch_title("https://www.rust-lang.org/learn").expect("title");
        assert!(t.contains("Rust"), "{t}");
        assert!(fetch_title("http://example.com").is_some_and(|t| t.contains("Example")));
    }
}
