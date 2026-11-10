//! Всё, что связано с Windows напрямую: значок в трее (свой поток и своё окно сообщений),
//! буфер обмена, горячая клавиша, уведомления, автозапуск, открытие ссылок, загрузка заголовков страниц.

#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::System::Time::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::store::{self, Added, LinkId};

const CF_UNICODETEXT: u32 = 13;
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const TRAY_CLASS: &str = "SaveUrls.TrayWindow";
const WM_TRAY: u32 = WM_APP + 1;
pub const WM_APP_SHOW: u32 = WM_APP + 2;
pub const WM_APP_QUIT: u32 = WM_APP + 3;
pub const WM_APP_HOTKEY: u32 = WM_APP + 4;
const HOTKEY_ID: i32 = 1;
const CLIP_TIMER: usize = 7;
const ID_SAVE: usize = 1;
const ID_OPEN: usize = 2;
const ID_QUIT: usize = 3;
const ID_RECENT: usize = 1000;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let mut n = 0;
    for u in s.encode_utf16() {
        if n + 1 >= dst.len() {
            break;
        }
        dst[n] = u;
        n += 1;
    }
    dst[n] = 0;
}

/// Обрезает строку до `max` символов с многоточием.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max.saturating_sub(1)).collect::<String>() + "…" }
}

// ───────────────────────── связь окна и трея ─────────────────────────

pub enum Event {
    /// Показать окно.
    Show,
    /// Показать окно и открыть ссылку на редактирование.
    Edit(LinkId),
}

pub struct Hub {
    pub store: store::Shared,
    pub ctx: egui::Context,
    pub events: Mutex<Vec<Event>>,
    /// Ссылка, которая сейчас лежит в буфере обмена (для подсказки в окне и трее).
    pub clip_url: Mutex<Option<String>>,
}

impl Hub {
    pub fn send(&self, e: Event) {
        self.events.lock().unwrap().push(e);
        self.ctx.request_repaint();
    }
}

static HUB: OnceLock<Arc<Hub>> = OnceLock::new();
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
pub static HOTKEY_OK: AtomicBool = AtomicBool::new(true);
static LAST_SAVED: AtomicU64 = AtomicU64::new(0);

pub fn post_tray(msg: u32) {
    let h = TRAY_HWND.load(Ordering::SeqCst);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, msg, 0, 0) };
    }
}

/// Один экземпляр на пользователя. Если программа уже запущена — показываем её окно и выходим.
pub fn single_instance() -> bool {
    unsafe {
        let name = wide("Local\\SaveUrls.SingleInstance");
        let h = CreateMutexW(null(), 0, name.as_ptr());
        if !h.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let class = wide(TRAY_CLASS);
            // Трей другого экземпляра мог ещё не успеть создаться.
            for _ in 0..20 {
                let other = FindWindowW(class.as_ptr(), null());
                if !other.is_null() {
                    PostMessageW(other, WM_APP_SHOW, 0, 0);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            return false;
        }
        // Дескриптор мьютекса держим до конца процесса.
        true
    }
}

pub fn spawn_tray(hub: Arc<Hub>) {
    if HUB.set(hub).is_err() {
        return;
    }
    std::thread::Builder::new().name("tray".into()).spawn(|| unsafe { tray_main() }).expect("tray thread");
}

fn hub() -> &'static Arc<Hub> {
    HUB.get().expect("hub")
}

// ───────────────────────── значок ─────────────────────────

unsafe fn make_icon(size: u32) -> HICON {
    unsafe {
        let rgba = crate::icon::icon_rgba(size);
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = size as i32;
        bmi.bmiHeader.biHeight = -(size as i32);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut c_void = null_mut();
        let hdc = GetDC(null_mut());
        let color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        ReleaseDC(null_mut(), hdc);
        if color.is_null() || bits.is_null() {
            return null_mut();
        }
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (size * size * 4) as usize);
        for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
        let mask_bits = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, mask_bits.as_ptr() as *const c_void);
        let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&ii);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

struct Icons {
    small: HICON,
    large: HICON,
}

static ICONS: OnceLock<(isize, isize)> = OnceLock::new();

fn icons() -> Icons {
    let (s, l) = *ICONS.get_or_init(|| unsafe {
        let small = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
        let large = GetSystemMetrics(SM_CXICON).max(32) as u32 * 2;
        (make_icon(small) as isize, make_icon(large) as isize)
    });
    Icons { small: s as HICON, large: l as HICON }
}

unsafe fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    n.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n
}

fn tip_text() -> String {
    let clip = hub().clip_url.lock().unwrap().clone();
    let click = hub().store.lock().unwrap().lib.settings.click;
    match (clip, click) {
        (Some(u), store::ClickAction::Save) => format!("Save URLs\nКлик — сохранить {}", ellipsize(&crate::urlx::pretty(&u), 70)),
        _ => "Save URLs — библиотека ссылок".into(),
    }
}

unsafe fn add_icon(hwnd: HWND) {
    unsafe {
        let mut n = nid(hwnd);
        n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        n.uCallbackMessage = WM_TRAY;
        n.hIcon = icons().small;
        copy_wide(&mut n.szTip, &tip_text());
        Shell_NotifyIconW(NIM_ADD, &n);
        n.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &n);
    }
}

unsafe fn update_tip(hwnd: HWND) {
    unsafe {
        let mut n = nid(hwnd);
        n.uFlags = NIF_TIP | NIF_SHOWTIP;
        copy_wide(&mut n.szTip, &tip_text());
        Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}

/// Уведомление Windows от значка в трее.
unsafe fn balloon(hwnd: HWND, title: &str, text: &str) {
    unsafe {
        let mut n = nid(hwnd);
        n.uFlags = NIF_INFO;
        copy_wide(&mut n.szInfoTitle, title);
        copy_wide(&mut n.szInfo, if text.is_empty() { " " } else { text });
        n.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON | NIIF_NOSOUND;
        n.hBalloonIcon = icons().large;
        Shell_NotifyIconW(NIM_MODIFY, &n);
    }
}

/// Уведомление от значка в трее (можно вызывать из любого потока).
pub fn notify(title: &str, text: &str) {
    let h = TRAY_HWND.load(Ordering::SeqCst);
    if h != 0 {
        LAST_SAVED.store(0, Ordering::SeqCst);
        unsafe { balloon(h as HWND, title, text) };
    }
}

// ───────────────────────── буфер обмена ─────────────────────────

pub unsafe fn clipboard_text(owner: HWND) -> Option<String> {
    unsafe {
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(owner) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
            return None;
        }
        let mut out = None;
        if IsClipboardFormatAvailable(CF_UNICODETEXT) != 0 {
            let h = GetClipboardData(CF_UNICODETEXT);
            if !h.is_null() {
                let p = GlobalLock(h as HGLOBAL) as *const u16;
                if !p.is_null() {
                    let max = GlobalSize(h as HGLOBAL) / 2;
                    let mut len = 0;
                    while len < max && *p.add(len) != 0 {
                        len += 1;
                    }
                    out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
                    GlobalUnlock(h as HGLOBAL);
                }
            }
        }
        CloseClipboard();
        out
    }
}

fn refresh_clip(hwnd: HWND) {
    let url = unsafe { clipboard_text(hwnd) }.and_then(|t| crate::urlx::extract_url(&t));
    let h = hub();
    let changed = {
        let mut c = h.clip_url.lock().unwrap();
        let changed = *c != url;
        *c = url;
        changed
    };
    if changed {
        unsafe { update_tip(hwnd) };
        h.ctx.request_repaint();
    }
}

// ───────────────────────── сохранение из трея ─────────────────────────

unsafe fn save_from_clipboard(hwnd: HWND, open_if_empty: bool) {
    unsafe {
        let h = hub();
        let url = clipboard_text(hwnd).and_then(|t| crate::urlx::extract_url(&t));
        let Some(url) = url else {
            LAST_SAVED.store(0, Ordering::SeqCst);
            if open_if_empty {
                h.send(Event::Show);
            } else if h.store.lock().unwrap().lib.settings.notify {
                balloon(hwnd, "В буфере нет ссылки", "Скопируйте адрес страницы и попробуйте ещё раз");
            }
            return;
        };
        let (res, notify, fetch, title, folder) = {
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
                if notify {
                    let text = format!("{}\n{} · нажмите, чтобы добавить описание", ellipsize(&crate::urlx::pretty(&url), 80), folder);
                    balloon(hwnd, "Ссылка сохранена", &text);
                }
                if fetch {
                    fetch_title_async(h.store.clone(), h.ctx.clone(), id, url, Some(hwnd as isize));
                }
            }
            Added::Exists(id) => {
                LAST_SAVED.store(id, Ordering::SeqCst);
                if notify {
                    balloon(hwnd, "Уже в библиотеке", &format!("{}\n{}", ellipsize(&title, 80), folder));
                }
            }
        }
    }
}

/// Загружает заголовок страницы в фоне и подставляет его вместо временного названия.
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

// ───────────────────────── окно трея ─────────────────────────

unsafe fn apply_hotkey(hwnd: HWND) {
    unsafe {
        UnregisterHotKey(hwnd, HOTKEY_ID);
        let want = hub().store.lock().unwrap().lib.settings.hotkey;
        let ok = !want || RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 'S' as u32) != 0;
        HOTKEY_OK.store(ok, Ordering::SeqCst);
        hub().ctx.request_repaint();
    }
}

unsafe fn quit(hwnd: HWND) -> ! {
    unsafe {
        {
            let mut st = hub().store.lock().unwrap();
            st.save_now();
        }
        let n = nid(hwnd);
        Shell_NotifyIconW(NIM_DELETE, &n);
        std::process::exit(0);
    }
}

unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let clip = clipboard_text(hwnd).and_then(|t| crate::urlx::extract_url(&t));
        let (recent, exists): (Vec<(String, String)>, bool) = {
            let st = hub().store.lock().unwrap();
            let mut v: Vec<_> = st.lib.links.iter().collect();
            v.sort_by_key(|l| std::cmp::Reverse((l.added, l.id)));
            let exists = clip.as_ref().is_some_and(|u| st.find_url(u).is_some());
            (v.into_iter().take(8).map(|l| (l.title.clone(), l.url.clone())).collect(), exists)
        };

        let menu = CreatePopupMenu();
        let save_label = match &clip {
            Some(u) if exists => format!("Уже сохранено\t{}", ellipsize(&crate::urlx::pretty(u), 38)),
            Some(u) => format!("Сохранить ссылку\t{}", ellipsize(&crate::urlx::pretty(u), 38)),
            None => "Сохранить ссылку — в буфере нет ссылки".into(),
        };
        let flags = if clip.is_some() && !exists { MF_STRING } else { MF_STRING | MF_GRAYED };
        AppendMenuW(menu, flags, ID_SAVE, wide(&save_label).as_ptr());
        AppendMenuW(menu, MF_STRING, ID_OPEN, wide("Открыть библиотеку").as_ptr());
        let sub = CreatePopupMenu();
        for (i, (title, _)) in recent.iter().enumerate() {
            let t = ellipsize(title, 50).replace('&', "&&");
            AppendMenuW(sub, MF_STRING, ID_RECENT + i, wide(&t).as_ptr());
        }
        if recent.is_empty() {
            AppendMenuW(sub, MF_STRING | MF_GRAYED, 0, wide("Пока пусто").as_ptr());
        }
        AppendMenuW(menu, MF_POPUP, sub as usize, wide("Недавние").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, ID_QUIT, wide("Выход").as_ptr());
        SetMenuDefaultItem(menu, if clip.is_some() && !exists { ID_SAVE as u32 } else { ID_OPEN as u32 }, 0);

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Без этого меню не закрывается по щелчку мимо.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null()) as usize;
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);

        match cmd {
            ID_SAVE => save_from_clipboard(hwnd, false),
            ID_OPEN => hub().send(Event::Show),
            ID_QUIT => quit(hwnd),
            c if c >= ID_RECENT && c < ID_RECENT + recent.len() => open_url(&recent[c - ID_RECENT].1),
            _ => {}
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TRAY => {
                let event = (lp as u32) & 0xffff;
                match event {
                    NIN_SELECT | NIN_KEYSELECT => {
                        let click = hub().store.lock().unwrap().lib.settings.click;
                        match click {
                            store::ClickAction::Save => save_from_clipboard(hwnd, true),
                            store::ClickAction::Open => hub().send(Event::Show),
                        }
                    }
                    WM_CONTEXTMENU => show_menu(hwnd),
                    NIN_BALLOONUSERCLICK => {
                        let id = LAST_SAVED.load(Ordering::SeqCst);
                        hub().send(if id != 0 { Event::Edit(id) } else { Event::Show });
                    }
                    _ => {}
                }
                0
            }
            WM_CLIPBOARDUPDATE => {
                // Читаем не сразу: программа, положившая данные, может ещё работать с буфером.
                SetTimer(hwnd, CLIP_TIMER, 150, None);
                0
            }
            WM_TIMER if wp == CLIP_TIMER => {
                KillTimer(hwnd, CLIP_TIMER);
                refresh_clip(hwnd);
                0
            }
            WM_HOTKEY if wp as i32 == HOTKEY_ID => {
                // Дать отпустить клавиши и приложению — дописать буфер.
                std::thread::sleep(std::time::Duration::from_millis(60));
                save_from_clipboard(hwnd, false);
                0
            }
            WM_APP_SHOW => {
                hub().send(Event::Show);
                0
            }
            WM_APP_QUIT => quit(hwnd),
            WM_APP_HOTKEY => {
                apply_hotkey(hwnd);
                update_tip(hwnd);
                0
            }
            _ if msg == TASKBAR_CREATED.load(Ordering::SeqCst) && msg != 0 => {
                // Проводник перезапустился — значок нужно добавить заново.
                add_icon(hwnd);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

unsafe fn tray_main() {
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = wide(TRAY_CLASS);
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassW(&wc);
        let title = wide("Save URLs tray");
        let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), WS_OVERLAPPED, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return;
        }
        TRAY_HWND.store(hwnd as isize, Ordering::SeqCst);
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::SeqCst);
        refresh_clip(hwnd);
        add_icon(hwnd);
        AddClipboardFormatListener(hwnd);
        apply_hotkey(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

// ───────────────────────── окно приложения ─────────────────────────

/// Вывести окно на передний план (после показа из трея).
pub fn bring_to_front(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let h = hwnd as HWND;
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        SetForegroundWindow(h);
    }
}

// ───────────────────────── оболочка ─────────────────────────

/// Открывает ссылку в браузере по умолчанию. Только веб-адреса — никаких локальных программ.
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
    unsafe {
        ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(&safe).as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

pub fn open_folder(path: &std::path::Path) {
    let s = path.display().to_string();
    unsafe {
        ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(&s).as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

// ───────────────────────── автозапуск ─────────────────────────

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Save URLs";

fn autostart_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" --background", exe.display()))
}

pub fn autostart_enabled() -> bool {
    unsafe {
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        let r = RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide(RUN_VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        );
        if r != 0 {
            return false;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let val = String::from_utf16_lossy(&buf[..n]).to_lowercase();
        autostart_command().is_some_and(|c| val == c.to_lowercase())
    }
}

pub fn set_autostart(on: bool) -> bool {
    unsafe {
        if on {
            let Some(cmd) = autostart_command() else { return false };
            let data = wide(&cmd);
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                wide(RUN_VALUE).as_ptr(),
                REG_SZ,
                data.as_ptr() as *const c_void,
                (data.len() * 2) as u32,
            ) == 0
        } else {
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr());
            r == 0 || r == ERROR_FILE_NOT_FOUND
        }
    }
}

// ───────────────────────── время ─────────────────────────

pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

pub fn local_time(unix: i64) -> LocalTime {
    unsafe {
        let ticks = ((unix + 11_644_473_600).max(0) as u64) * 10_000_000;
        let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut loc: SYSTEMTIME = std::mem::zeroed();
        FileTimeToSystemTime(&ft, &mut utc);
        if SystemTimeToTzSpecificLocalTime(null(), &utc, &mut loc) == 0 {
            loc = utc;
        }
        LocalTime { year: loc.wYear as i32, month: loc.wMonth as u32, day: loc.wDay as u32, hour: loc.wHour as u32, minute: loc.wMinute as u32 }
    }
}

// ───────────────────────── HTTP ─────────────────────────

struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
}

/// GET через WinHTTP (системные прокси и сертификаты). Возвращает тело (не больше `max` байт) и Content-Type.
pub fn http_get(url: &str, max: usize) -> Option<(Vec<u8>, String)> {
    unsafe {
        let agent = wide("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36");
        let mut session = Handle(WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0));
        if session.0.is_null() {
            session = Handle(WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, null(), null(), 0));
        }
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, 5000, 6000, 6000, 8000);
        let mut flag: u32 = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
        WinHttpSetOption(session.0, WINHTTP_OPTION_DECOMPRESSION, &mut flag as *mut u32 as *mut c_void, 4);

        let wurl = wide(url);
        let mut c: URL_COMPONENTS = std::mem::zeroed();
        c.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
        c.dwSchemeLength = u32::MAX;
        c.dwHostNameLength = u32::MAX;
        c.dwUrlPathLength = u32::MAX;
        c.dwExtraInfoLength = u32::MAX;
        if WinHttpCrackUrl(wurl.as_ptr(), 0, 0, &mut c) == 0 || c.lpszHostName.is_null() {
            return None;
        }
        let host: Vec<u16> = std::slice::from_raw_parts(c.lpszHostName, c.dwHostNameLength as usize).iter().copied().chain([0]).collect();
        let mut path: Vec<u16> = Vec::new();
        if !c.lpszUrlPath.is_null() {
            path.extend_from_slice(std::slice::from_raw_parts(c.lpszUrlPath, c.dwUrlPathLength as usize));
        }
        if !c.lpszExtraInfo.is_null() {
            let extra = std::slice::from_raw_parts(c.lpszExtraInfo, c.dwExtraInfoLength as usize);
            // Якорь (#…) серверу не отправляется.
            let end = extra.iter().position(|&u| u == '#' as u16).unwrap_or(extra.len());
            path.extend_from_slice(&extra[..end]);
        }
        if path.is_empty() {
            path.push('/' as u16);
        }
        path.push(0);

        let connect = Handle(WinHttpConnect(session.0, host.as_ptr(), c.nPort, 0));
        if connect.0.is_null() {
            return None;
        }
        let secure = if c.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS { WINHTTP_FLAG_SECURE } else { 0 };
        let req = Handle(WinHttpOpenRequest(connect.0, wide("GET").as_ptr(), path.as_ptr(), null(), null(), null(), secure));
        if req.0.is_null() {
            return None;
        }
        let headers = wide("Accept: text/html,application/xhtml+xml;q=0.9,*/*;q=0.5\r\nAccept-Language: ru,en;q=0.8\r\n");
        if WinHttpSendRequest(req.0, headers.as_ptr(), u32::MAX, null(), 0, 0, 0) == 0 {
            return None;
        }
        if WinHttpReceiveResponse(req.0, null_mut()) == 0 {
            return None;
        }
        let mut status: u32 = 0;
        let mut len = 4u32;
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            &mut status as *mut u32 as *mut c_void,
            &mut len,
            null_mut(),
        );
        if !(200..300).contains(&status) {
            return None;
        }
        let mut ct = [0u16; 256];
        let mut ct_len = (ct.len() * 2) as u32;
        let ctype = if WinHttpQueryHeaders(req.0, WINHTTP_QUERY_CONTENT_TYPE, null(), ct.as_mut_ptr() as *mut c_void, &mut ct_len, null_mut()) != 0 {
            String::from_utf16_lossy(&ct[..(ct_len / 2) as usize])
        } else {
            String::new()
        };
        if !ctype.is_empty() && !ctype.to_ascii_lowercase().contains("html") {
            return Some((Vec::new(), ctype));
        }

        let mut body = Vec::new();
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let mut read = 0u32;
            if WinHttpReadData(req.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read) == 0 || read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
            // Заголовок обычно в начале — дальше читать незачем.
            if body.len() >= max || contains_ci(&body, b"</title") {
                break;
            }
        }
        Some((body, ctype))
    }
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    /// Ходит в интернет, поэтому запускается вручную: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn fetches_real_titles() {
        let t = super::fetch_title("https://www.rust-lang.org/learn").expect("title");
        assert!(t.contains("Rust"), "{t}");
        let t = super::fetch_title("https://github.com/emilk/egui").expect("title");
        assert!(t.contains("egui"), "{t}");
        // Редирект http → https.
        assert!(super::fetch_title("http://example.com").is_some_and(|t| t.contains("Example")));
    }
}
