//! Link library: data model and storage in `%APPDATA%\Save URLs\library.json`.
//! Shared between the window and the tray thread (`Shared`), so saving is an atomic write through a temporary file.

use crate::theme::ThemeMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub type FolderId = u64;
pub type LinkId = u64;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Link {
    pub id: LinkId,
    pub url: String,
    pub title: String,
    pub description: String,
    pub folder: Option<FolderId>,
    /// Time added, Unix seconds.
    pub added: i64,
    /// The title has not been edited by hand, so a downloaded page title may replace it.
    pub auto_title: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Newest,
    Oldest,
    NameAsc,
    NameDesc,
    Domain,
}

impl Sort {
    pub const ALL: [Sort; 5] = [Sort::Newest, Sort::Oldest, Sort::NameAsc, Sort::NameDesc, Sort::Domain];
    /// Caption in the current interface language.
    pub fn label(self) -> &'static str {
        let i = match self {
            Sort::Newest => 0,
            Sort::Oldest => 1,
            Sort::NameAsc => 2,
            Sort::NameDesc => 3,
            Sort::Domain => 4,
        };
        crate::strings::s().sort[i]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickAction {
    Save,
    Open,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub sort: Sort,
    /// Download the page title from the internet after saving.
    pub fetch_titles: bool,
    /// Windows notification after saving from the tray.
    pub notify: bool,
    /// Global hotkey Ctrl+Alt+S.
    pub hotkey: bool,
    /// What a left click on the tray icon does.
    pub click: ClickAction,
    /// Folder for new links (None means "No folder").
    pub default_folder: Option<FolderId>,
    /// The "the program runs in the tray" hint has already been shown.
    pub tray_hint_shown: bool,
    /// Check for updates at startup (NIMVORO).
    pub check_updates: bool,
    /// Interface language code (`en`, `ru`, `uk`); None until first start resolves it.
    pub language: Option<String>,
    /// Theme: system, light or dark.
    pub theme: ThemeMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self { sort: Sort::Newest, fetch_titles: true, notify: true, hotkey: true, click: ClickAction::Save, default_folder: None, tray_hint_shown: false, check_updates: true, language: None, theme: ThemeMode::System }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Library {
    pub version: u32,
    pub next_id: u64,
    pub folders: Vec<Folder>,
    pub links: Vec<Link>,
    pub settings: Settings,
}

impl Default for Library {
    fn default() -> Self {
        Self { version: 1, next_id: 1, folders: Vec::new(), links: Vec::new(), settings: Settings::default() }
    }
}

pub enum Added {
    New(LinkId),
    Exists(LinkId),
}

pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Key for duplicate detection: ignores the scheme, `www.`, a trailing `/` and the host's letter case.
pub fn url_key(url: &str) -> String {
    let p = crate::urlx::pretty(url.trim());
    match p.find('/') {
        Some(i) => format!("{}{}", p[..i].to_lowercase(), &p[i..]),
        None => p.to_lowercase(),
    }
}

pub struct Store {
    pub lib: Library,
    path: Option<PathBuf>,
    dirty_at: Option<Instant>,
    backed_up: bool,
    /// Change counter: the window recomputes derived data when it changes.
    pub rev: u64,
    /// Set when the library file could not be read (it was kept next to the original under another name).
    pub load_error: Option<LoadError>,
}

/// Why the library file was rejected; the window turns it into a localized message.
#[derive(Clone, Debug)]
pub struct LoadError {
    pub detail: String,
    pub backup: String,
}

pub type Shared = Arc<Mutex<Store>>;

/// True when a `library.json` lies next to the exe (portable mode).
pub fn is_portable() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("library.json").exists())).unwrap_or(false)
}

/// A `library.json` next to the exe means portable mode; otherwise `%APPDATA%\Save URLs` is used.
fn default_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let local = dir.join("library.json");
        if local.exists() {
            return Some(local);
        }
    }
    dirs::data_dir().map(|d| d.join("Save URLs").join("library.json"))
}

impl Store {
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut load_error = None;
        let lib = match path.as_ref().map(std::fs::read_to_string) {
            Some(Ok(text)) => match serde_json::from_str::<Library>(&text) {
                Ok(lib) => lib,
                Err(e) => {
                    // Never overwrite a broken file: move it aside.
                    let p = path.as_ref().unwrap();
                    let broken = p.with_file_name(format!("library.broken-{}.json", now_unix()));
                    let _ = std::fs::rename(p, &broken);
                    load_error = Some(LoadError { detail: e.to_string(), backup: broken.file_name().unwrap_or_default().to_string_lossy().into_owned() });
                    Library::default()
                }
            },
            _ => Library::default(),
        };
        let mut s = Self { lib, path, dirty_at: None, backed_up: false, rev: 1, load_error };
        s.repair();
        s
    }

    /// Repairs what manual edits of the file may have broken: ids and references to deleted folders.
    fn repair(&mut self) {
        let max = self.lib.links.iter().map(|l| l.id).chain(self.lib.folders.iter().map(|f| f.id)).max().unwrap_or(0);
        self.lib.next_id = self.lib.next_id.max(max + 1);
        let folders: Vec<FolderId> = self.lib.folders.iter().map(|f| f.id).collect();
        for l in &mut self.lib.links {
            if l.folder.is_some_and(|f| !folders.contains(&f)) {
                l.folder = None;
            }
        }
        if self.lib.settings.default_folder.is_some_and(|f| !folders.contains(&f)) {
            self.lib.settings.default_folder = None;
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    fn new_id(&mut self) -> u64 {
        let id = self.lib.next_id;
        self.lib.next_id += 1;
        id
    }

    /// Marks a change; it is written to disk after half a second (`flush_if_due`) or at once (`save_now`).
    pub fn touch(&mut self) {
        self.rev += 1;
        self.dirty_at.get_or_insert_with(Instant::now);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_at.is_some()
    }

    pub fn flush_if_due(&mut self) {
        if self.dirty_at.is_some_and(|t| t.elapsed().as_millis() >= 400) {
            self.save_now();
        }
    }

    pub fn save_now(&mut self) {
        self.dirty_at = None;
        let Some(p) = self.path.clone() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Once per run, keep a copy of the previous version.
        if !self.backed_up && p.exists() {
            let _ = std::fs::copy(&p, p.with_file_name("library.bak.json"));
            self.backed_up = true;
        }
        let Ok(json) = serde_json::to_string_pretty(&self.lib) else { return };
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    // ───────────────────────── links ─────────────────────────

    pub fn find_url(&self, url: &str) -> Option<LinkId> {
        let key = url_key(url);
        self.lib.links.iter().find(|l| url_key(&l.url) == key).map(|l| l.id)
    }

    /// Adds a link (unless it already exists) to the default folder or to `folder`.
    pub fn add_url(&mut self, url: &str, folder: Option<Option<FolderId>>) -> Added {
        if let Some(id) = self.find_url(url) {
            return Added::Exists(id);
        }
        let id = self.new_id();
        let folder = folder.unwrap_or(self.lib.settings.default_folder);
        self.lib.links.push(Link {
            id,
            url: url.trim().to_string(),
            title: crate::urlx::fallback_title(url),
            description: String::new(),
            folder,
            added: now_unix(),
            auto_title: true,
        });
        self.touch();
        Added::New(id)
    }

    pub fn link(&self, id: LinkId) -> Option<&Link> {
        self.lib.links.iter().find(|l| l.id == id)
    }

    pub fn link_mut(&mut self, id: LinkId) -> Option<&mut Link> {
        self.lib.links.iter_mut().find(|l| l.id == id)
    }

    /// A page title arrived from the network: use it unless the user renamed the link.
    pub fn set_fetched_title(&mut self, id: LinkId, title: &str) -> bool {
        let Some(l) = self.link_mut(id) else { return false };
        if !l.auto_title || title.trim().is_empty() {
            return false;
        }
        l.title = title.trim().to_string();
        l.auto_title = false;
        self.touch();
        true
    }

    pub fn delete_link(&mut self, id: LinkId) -> Option<Link> {
        let i = self.lib.links.iter().position(|l| l.id == id)?;
        let l = self.lib.links.remove(i);
        self.touch();
        Some(l)
    }

    pub fn restore_link(&mut self, link: Link) {
        if self.link(link.id).is_none() {
            let mut link = link;
            if link.folder.is_some_and(|f| self.folder(f).is_none()) {
                link.folder = None;
            }
            self.lib.links.push(link);
            self.touch();
        }
    }

    pub fn move_link(&mut self, id: LinkId, folder: Option<FolderId>) {
        if let Some(l) = self.link_mut(id)
            && l.folder != folder
        {
            l.folder = folder;
            self.touch();
        }
    }

    // ───────────────────────── folders ─────────────────────────

    pub fn folder(&self, id: FolderId) -> Option<&Folder> {
        self.lib.folders.iter().find(|f| f.id == id)
    }

    pub fn folder_name(&self, id: Option<FolderId>) -> String {
        match id.and_then(|f| self.folder(f)) {
            Some(f) => f.name.clone(),
            None => crate::strings::s().no_folder.into(),
        }
    }

    pub fn add_folder(&mut self, name: &str) -> FolderId {
        let id = self.new_id();
        let name = unique_name(name.trim(), &self.lib.folders, None);
        self.lib.folders.push(Folder { id, name });
        self.touch();
        id
    }

    pub fn rename_folder(&mut self, id: FolderId, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let name = unique_name(name, &self.lib.folders, Some(id));
        if let Some(f) = self.lib.folders.iter_mut().find(|f| f.id == id)
            && f.name != name
        {
            f.name = name;
            self.touch();
        }
    }

    /// Deletes a folder; its links stay in the library under "No folder".
    pub fn delete_folder(&mut self, id: FolderId) {
        self.lib.folders.retain(|f| f.id != id);
        for l in &mut self.lib.links {
            if l.folder == Some(id) {
                l.folder = None;
            }
        }
        if self.lib.settings.default_folder == Some(id) {
            self.lib.settings.default_folder = None;
        }
        self.touch();
    }

    pub fn move_folder(&mut self, from: usize, to: usize) {
        if from < self.lib.folders.len() && to < self.lib.folders.len() && from != to {
            let f = self.lib.folders.remove(from);
            self.lib.folders.insert(to, f);
            self.touch();
        }
    }

    pub fn count_in(&self, folder: Option<FolderId>) -> usize {
        self.lib.links.iter().filter(|l| l.folder == folder).count()
    }
}

/// "New folder", "New folder 2", ...
fn unique_name(base: &str, folders: &[Folder], except: Option<FolderId>) -> String {
    let base = if base.is_empty() { crate::strings::s().new_folder } else { base };
    let taken = |n: &str| folders.iter().any(|f| Some(f.id) != except && f.name.to_lowercase() == n.to_lowercase());
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|k| format!("{base} {k}")).find(|n| !taken(n)).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> Store {
        Store::load_from(None)
    }

    #[test]
    fn dedupe_by_key() {
        let mut s = temp_store();
        assert!(matches!(s.add_url("https://www.Example.com/a/", None), Added::New(_)));
        assert!(matches!(s.add_url("http://example.com/a", None), Added::Exists(_)));
        assert!(matches!(s.add_url("https://example.com/A", None), Added::New(_)));
    }

    #[test]
    fn folders() {
        let mut s = temp_store();
        let a = s.add_folder("Игры");
        let b = s.add_folder("игры");
        assert_eq!(s.folder(b).unwrap().name, "игры 2");
        let Added::New(l) = s.add_url("https://x.io", Some(Some(a))) else { panic!() };
        s.lib.settings.default_folder = Some(a);
        s.delete_folder(a);
        assert_eq!(s.link(l).unwrap().folder, None);
        assert_eq!(s.lib.settings.default_folder, None);
    }

    #[test]
    fn fetched_title_respects_manual_edit() {
        let mut s = temp_store();
        let Added::New(id) = s.add_url("https://x.io", None) else { panic!() };
        s.link_mut(id).unwrap().auto_title = false;
        assert!(!s.set_fetched_title(id, "Page"));
        s.link_mut(id).unwrap().auto_title = true;
        assert!(s.set_fetched_title(id, "Page"));
        assert_eq!(s.link(id).unwrap().title, "Page");
    }

    #[test]
    fn roundtrip_and_broken_file() {
        let dir = std::env::temp_dir().join(format!("save_urls_test_{}", now_unix() ^ std::process::id() as i64));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("library.json");
        let mut s = Store::load_from(Some(p.clone()));
        s.add_folder("Работа");
        s.add_url("https://rust-lang.org", None);
        s.save_now();
        let s2 = Store::load_from(Some(p.clone()));
        assert_eq!(s2.lib.links.len(), 1);
        assert_eq!(s2.lib.folders[0].name, "Работа");
        std::fs::write(&p, "{ broken").unwrap();
        let s3 = Store::load_from(Some(p.clone()));
        assert!(s3.load_error.is_some());
        assert!(!p.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
