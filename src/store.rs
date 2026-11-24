//! Link library: data model and storage in `%APPDATA%\Save URLs\library.json`.
//! Shared between the window and the tray thread (`Shared`), so saving is an atomic write through a temporary file.

use crate::theme::ThemeMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub type FolderId = u64;
pub type LinkId = u64;

/// Library file format: 1 = flat folders, 2 = folders may contain folders.
pub const LIB_VERSION: u32 = 2;
/// Folders nest at most this deep; a top-level folder is level 1.
pub const MAX_DEPTH: usize = 5;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    /// Parent folder; `None` for a top-level folder. The order of siblings is their order in `Library::folders`.
    #[serde(default)]
    pub parent: Option<FolderId>,
    /// The subfolders are shown in the sidebar tree.
    #[serde(default)]
    pub expanded: bool,
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
        Self { version: LIB_VERSION, next_id: 1, folders: Vec::new(), links: Vec::new(), settings: Settings::default() }
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

/// A `library.json` next to the exe means portable mode; otherwise `%APPDATA%\Save URLs` (`~/.config/Save URLs` on Linux) is used.
fn default_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let local = dir.join("library.json");
        if local.exists() {
            return Some(local);
        }
    }
    dirs::config_dir().map(|d| d.join("Save URLs").join("library.json"))
}

impl Store {
    pub fn load() -> Self {
        Self::load_from(default_path())
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut load_error = None;
        let mut migrate_from = None;
        let lib = match path.as_ref().map(std::fs::read_to_string) {
            Some(Ok(text)) => match serde_json::from_str::<Library>(&text) {
                Ok(lib) => {
                    if lib.version < LIB_VERSION {
                        migrate_from = Some(lib.version);
                    }
                    lib
                }
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
        if let Some(old) = migrate_from {
            s.migrate(old);
        }
        s.repair();
        s
    }

    /// Upgrades an older file: flat folders become top-level folders in the same order.
    /// The old file is copied to `library.json.bak-<old version>` first (an earlier copy is never overwritten).
    fn migrate(&mut self, old: u32) {
        if let Some(p) = &self.path {
            let bak = p.with_file_name(format!("library.json.bak-{old}"));
            if p.exists() && !bak.exists() {
                let _ = std::fs::copy(p, &bak);
            }
        }
        for f in &mut self.lib.folders {
            f.parent = None;
        }
        self.lib.version = LIB_VERSION;
        self.dirty_at = Some(Instant::now());
    }

    /// Repairs what manual edits of the file may have broken: ids, references to deleted folders, the folder tree.
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
        // A parent that does not exist, a loop, or a chain deeper than the limit: lift the folder to the top level.
        for f in &mut self.lib.folders {
            if f.parent.is_some_and(|p| !folders.contains(&p)) {
                f.parent = None;
            }
        }
        while let Some(id) = self.lib.folders.iter().find(|f| f.parent.is_some() && self.chain_len(f.id).is_none_or(|d| d > MAX_DEPTH)).map(|f| f.id) {
            if let Some(f) = self.lib.folders.iter_mut().find(|f| f.id == id) {
                f.parent = None;
            }
        }
    }

    /// Number of folders from `id` up to the top level (the level of `id`); `None` if the parent chain loops.
    fn chain_len(&self, id: FolderId) -> Option<usize> {
        let mut n = 1;
        let mut cur = self.folder(id)?;
        while let Some(p) = cur.parent {
            n += 1;
            if n > self.lib.folders.len() + 1 {
                return None;
            }
            cur = self.folder(p)?;
        }
        Some(n)
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

    /// Writes the library now. `false` if it could not be written.
    pub fn save_now(&mut self) -> bool {
        self.dirty_at = None;
        let Some(p) = self.path.clone() else { return true };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Once per run, keep a copy of the previous version.
        if !self.backed_up && p.exists() {
            let _ = std::fs::copy(&p, p.with_file_name("library.bak.json"));
            self.backed_up = true;
        }
        let Ok(json) = serde_json::to_string_pretty(&self.lib) else { return false };
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &p).is_ok()
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

    /// New top-level folder.
    pub fn add_folder(&mut self, name: &str) -> FolderId {
        self.add_folder_in(name, None).expect("a top-level folder can always be added")
    }

    /// New folder at the end of `parent`'s children; `None` if that would go deeper than [`MAX_DEPTH`].
    /// The parent is expanded so the new folder is visible.
    pub fn add_folder_in(&mut self, name: &str, parent: Option<FolderId>) -> Option<FolderId> {
        if let Some(p) = parent
            && !self.can_add_child(p)
        {
            return None;
        }
        let id = self.new_id();
        let name = self.unique_name(name.trim(), parent, None);
        self.lib.folders.push(Folder { id, name, parent, expanded: false });
        if let Some(p) = parent {
            self.set_expanded(p, true);
        }
        self.touch();
        Some(id)
    }

    pub fn rename_folder(&mut self, id: FolderId, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(parent) = self.folder(id).map(|f| f.parent) else { return };
        let name = self.unique_name(name, parent, Some(id));
        if let Some(f) = self.lib.folders.iter_mut().find(|f| f.id == id)
            && f.name != name
        {
            f.name = name;
            self.touch();
        }
    }

    /// "New folder", "New folder 2", ... unique among the siblings under `parent`.
    fn unique_name(&self, base: &str, parent: Option<FolderId>, except: Option<FolderId>) -> String {
        let base = if base.is_empty() { crate::strings::s().new_folder } else { base };
        let taken = |n: &str| self.lib.folders.iter().any(|f| f.parent == parent && Some(f.id) != except && f.name.to_lowercase() == n.to_lowercase());
        if !taken(base) {
            return base.to_string();
        }
        (2..).map(|k| format!("{base} {k}")).find(|n| !taken(n)).unwrap()
    }

    pub fn set_expanded(&mut self, id: FolderId, expanded: bool) {
        if let Some(f) = self.lib.folders.iter_mut().find(|f| f.id == id)
            && f.expanded != expanded
        {
            f.expanded = expanded;
            self.touch();
        }
    }

    /// Deletes a folder and everything nested in it. The links of all those folders stay in the library under "No folder".
    pub fn delete_folder(&mut self, id: FolderId) {
        let gone = self.subtree(id);
        self.lib.folders.retain(|f| !gone.contains(&f.id));
        for l in &mut self.lib.links {
            if l.folder.is_some_and(|f| gone.contains(&f)) {
                l.folder = None;
            }
        }
        if self.lib.settings.default_folder.is_some_and(|f| gone.contains(&f)) {
            self.lib.settings.default_folder = None;
        }
        self.touch();
    }

    // ───────────────────────── folder tree ─────────────────────────

    /// Children of `parent` (`None` = top level) in display order.
    pub fn children(&self, parent: Option<FolderId>) -> Vec<FolderId> {
        self.lib.folders.iter().filter(|f| f.parent == parent).map(|f| f.id).collect()
    }

    pub fn has_children(&self, id: FolderId) -> bool {
        self.lib.folders.iter().any(|f| f.parent == Some(id))
    }

    /// Level of a folder: 1 for a top-level folder.
    pub fn depth(&self, id: FolderId) -> usize {
        self.chain_len(id).unwrap_or(1)
    }

    /// `id` and all folders nested in it, parents before children.
    pub fn subtree(&self, id: FolderId) -> Vec<FolderId> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let cur = out[i];
            let next: Vec<FolderId> = self.lib.folders.iter().filter(|f| f.parent == Some(cur) && !out.contains(&f.id)).map(|f| f.id).collect();
            out.extend(next);
            i += 1;
        }
        out
    }

    /// Number of levels in the subtree of `id` (1 for a folder without children).
    pub fn height(&self, id: FolderId) -> usize {
        self.height_at(id, 1)
    }

    fn height_at(&self, id: FolderId, level: usize) -> usize {
        if level > MAX_DEPTH + 1 {
            return 1;
        }
        1 + self.children(Some(id)).into_iter().map(|c| self.height_at(c, level + 1)).max().unwrap_or(0)
    }

    /// A folder may get a subfolder only above the deepest level.
    pub fn can_add_child(&self, parent: FolderId) -> bool {
        self.folder(parent).is_some() && self.depth(parent) < MAX_DEPTH
    }

    /// Whether `id` (with its whole subtree) may be moved under `new_parent` (`None` = top level):
    /// not into itself or one of its descendants, and the deepest folder of the subtree must stay within [`MAX_DEPTH`].
    pub fn can_move(&self, id: FolderId, new_parent: Option<FolderId>) -> bool {
        if self.folder(id).is_none() {
            return false;
        }
        let Some(p) = new_parent else { return self.height(id) <= MAX_DEPTH };
        if self.folder(p).is_none() || self.subtree(id).contains(&p) {
            return false;
        }
        self.depth(p) + self.height(id) <= MAX_DEPTH
    }

    /// Moves `id` under `new_parent` as child number `index` (counted without `id`; past the end = last).
    /// Returns `false`, changing nothing, if the move is not allowed or would not change anything.
    pub fn move_folder_to(&mut self, id: FolderId, new_parent: Option<FolderId>, index: usize) -> bool {
        if !self.can_move(id, new_parent) {
            return false;
        }
        let old_parent = self.folder(id).and_then(|f| f.parent);
        let old_index = self.children(old_parent).iter().position(|c| *c == id).unwrap_or(0);
        let siblings: Vec<FolderId> = self.children(new_parent).into_iter().filter(|c| *c != id).collect();
        let index = index.min(siblings.len());
        if old_parent == new_parent && index == old_index {
            return false;
        }
        let Some(from) = self.lib.folders.iter().position(|f| f.id == id) else { return false };
        let mut f = self.lib.folders.remove(from);
        f.parent = new_parent;
        let at = match siblings.get(index) {
            Some(next) => self.lib.folders.iter().position(|x| x.id == *next).unwrap_or(self.lib.folders.len()),
            None => self.lib.folders.len(),
        };
        self.lib.folders.insert(at, f);
        if let Some(p) = new_parent {
            self.set_expanded(p, true);
        }
        self.touch();
        true
    }

    /// Moves a folder one place up (`-1`) or down (`1`) among its siblings.
    pub fn move_sibling(&mut self, id: FolderId, delta: isize) -> bool {
        let Some(parent) = self.folder(id).map(|f| f.parent) else { return false };
        let sibs = self.children(parent);
        let Some(i) = sibs.iter().position(|c| *c == id) else { return false };
        let to = i as isize + delta;
        if to < 0 || to as usize >= sibs.len() {
            return false;
        }
        self.move_folder_to(id, parent, to as usize)
    }

    /// Folders in the order the sidebar shows them (the tree, depth first) with their levels.
    /// Subfolders of collapsed folders are left out unless `all`.
    pub fn tree(&self, all: bool) -> Vec<(FolderId, usize)> {
        let mut out = Vec::new();
        self.walk(None, 1, all, &mut out);
        out
    }

    fn walk(&self, parent: Option<FolderId>, level: usize, all: bool, out: &mut Vec<(FolderId, usize)>) {
        if level > MAX_DEPTH {
            return;
        }
        for c in self.children(parent) {
            out.push((c, level));
            if all || self.folder(c).is_some_and(|f| f.expanded) {
                self.walk(Some(c), level + 1, all, out);
            }
        }
    }

    /// Names from the top level down to `id`, e.g. `["Work", "Docs"]`.
    pub fn folder_chain(&self, id: FolderId) -> Vec<String> {
        let mut names = Vec::new();
        let mut cur = self.folder(id);
        while let Some(f) = cur {
            names.push(f.name.clone());
            if names.len() > MAX_DEPTH {
                break;
            }
            cur = f.parent.and_then(|p| self.folder(p));
        }
        names.reverse();
        names
    }

    /// `Parent › Child` for a folder, or just its name for a top-level one.
    pub fn folder_path(&self, id: FolderId) -> String {
        self.folder_chain(id).join(" \u{203a} ")
    }

    /// Where a dragged folder lands for a drop at `at`: `(new parent, index among the new siblings)`.
    /// `None` when the drop is not allowed or would change nothing.
    pub fn drop_plan(&self, src: FolderId, at: DropAt) -> Option<(Option<FolderId>, usize)> {
        let (parent, index) = match at {
            DropAt::Inside(t) => {
                self.folder(t)?;
                (Some(t), usize::MAX)
            }
            DropAt::Before(t) => {
                let parent = self.folder(t)?.parent;
                let sibs: Vec<FolderId> = self.children(parent).into_iter().filter(|c| *c != src).collect();
                // Dropping before itself finds no `t` among the siblings: that is "no change".
                (parent, sibs.iter().position(|c| *c == t)?)
            }
            DropAt::End => (None, usize::MAX),
        };
        if !self.can_move(src, parent) {
            return None;
        }
        let old_parent = self.folder(src)?.parent;
        let old_index = self.children(old_parent).iter().position(|c| *c == src)?;
        let n = self.children(parent).into_iter().filter(|c| *c != src).count();
        let index = index.min(n);
        (old_parent != parent || old_index != index).then_some((parent, index))
    }

    /// Drag-and-drop of a folder row, through the same path as the context menu.
    pub fn drop_folder(&mut self, src: FolderId, at: DropAt) -> bool {
        match self.drop_plan(src, at) {
            Some((parent, index)) => self.move_folder_to(src, parent, index),
            None => false,
        }
    }

    /// Links directly in `folder`.
    pub fn count_in(&self, folder: Option<FolderId>) -> usize {
        self.lib.links.iter().filter(|l| l.folder == folder).count()
    }

    /// Links in `folder` and in every folder nested in it.
    pub fn count_deep(&self, folder: FolderId) -> usize {
        let ids = self.subtree(folder);
        self.lib.links.iter().filter(|l| l.folder.is_some_and(|f| ids.contains(&f))).count()
    }

    /// All folders as `(id, "Parent › Child")` in tree order, for pickers.
    pub fn folder_list(&self) -> Vec<(FolderId, String)> {
        self.tree(true).into_iter().map(|(id, _)| (id, self.folder_path(id))).collect()
    }
}

/// Where a dragged folder is dropped, relative to a visible row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropAt {
    /// Into the gap before this folder, at its level.
    Before(FolderId),
    /// Into this folder, as its last child.
    Inside(FolderId),
    /// Past the last row: the end of the top level.
    End,
}

/// What a click at height `y` inside a row (`top..bottom`) means for a folder drag:
/// the upper third is the gap before the row, the lower third the gap before the next row (`next`, or the end of
/// the list), the middle third drops into the row's folder.
pub fn drop_zone(row: FolderId, next: Option<FolderId>, top: f32, bottom: f32, y: f32) -> DropAt {
    let h = (bottom - top).max(1.0);
    let f = (y - top) / h;
    if f < 1.0 / 3.0 {
        DropAt::Before(row)
    } else if f > 2.0 / 3.0 {
        next.map_or(DropAt::End, DropAt::Before)
    } else {
        DropAt::Inside(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> Store {
        Store::load_from(None)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("save_urls_test_{tag}_{}_{}", now_unix(), std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Names of the whole tree, depth first, as `A/B/C` paths.
    fn paths(s: &Store) -> Vec<String> {
        s.folder_list().into_iter().map(|(_, p)| p.replace(" \u{203a} ", "/")).collect()
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
    fn names_are_unique_per_parent() {
        let mut s = temp_store();
        let a = s.add_folder("A");
        let b = s.add_folder("B");
        let x1 = s.add_folder_in("X", Some(a)).unwrap();
        let x2 = s.add_folder_in("x", Some(b)).unwrap();
        let x3 = s.add_folder_in("x", Some(a)).unwrap();
        assert_eq!(s.folder(x1).unwrap().name, "X");
        assert_eq!(s.folder(x2).unwrap().name, "x");
        assert_eq!(s.folder(x3).unwrap().name, "x 2");
    }

    #[test]
    fn migration_from_flat_format() {
        let dir = temp_dir("migrate");
        let p = dir.join("library.json");
        // A 1.6.0 file: folders have only id and name.
        let old = r#"{"version":1,"next_id":9,"folders":[{"id":3,"name":"Work"},{"id":1,"name":"Games"},{"id":2,"name":"Music"}],
            "links":[{"id":5,"url":"https://a.io","title":"A","description":"","folder":1,"added":7,"auto_title":false},
                     {"id":6,"url":"https://b.io","title":"B","description":"","folder":null,"added":8,"auto_title":false}],
            "settings":{"default_folder":2}}"#;
        std::fs::write(&p, old).unwrap();
        let mut s = Store::load_from(Some(p.clone()));
        assert_eq!(s.lib.version, LIB_VERSION);
        assert_eq!(s.lib.folders.iter().map(|f| f.id).collect::<Vec<_>>(), vec![3, 1, 2], "order is kept");
        assert!(s.lib.folders.iter().all(|f| f.parent.is_none() && !f.expanded));
        assert_eq!(s.lib.links.len(), 2);
        assert_eq!(s.link(5).unwrap().folder, Some(1));
        assert_eq!(s.lib.settings.default_folder, Some(2));
        // The old file is kept before anything is written.
        let bak = dir.join("library.json.bak-1");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), old);
        assert!(s.is_dirty());
        assert!(s.save_now());
        let again = Store::load_from(Some(p.clone()));
        assert_eq!(again.lib.version, LIB_VERSION);
        assert_eq!(again.lib.folders.len(), 3);
        // The second start does not migrate or touch the backup.
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), old);
        assert!(!again.is_dirty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn depth_and_limit() {
        let mut s = temp_store();
        let mut ids = vec![s.add_folder("L1")];
        for k in 2..=MAX_DEPTH {
            let id = s.add_folder_in(&format!("L{k}"), Some(*ids.last().unwrap())).unwrap();
            ids.push(id);
        }
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(s.depth(*id), i + 1);
        }
        assert_eq!(s.height(ids[0]), 5);
        assert_eq!(s.height(ids[4]), 1);
        assert!(!s.can_add_child(ids[4]));
        assert!(s.add_folder_in("too deep", Some(ids[4])).is_none());
        assert!(s.can_add_child(ids[3]));
        assert_eq!(s.lib.folders.len(), 5);
        assert_eq!(s.folder_path(ids[2]), "L1 \u{203a} L2 \u{203a} L3");
    }

    #[test]
    fn moving_a_subtree_respects_its_own_depth() {
        let mut s = temp_store();
        let a = s.add_folder("A");
        let a2 = s.add_folder_in("A2", Some(a)).unwrap();
        let b = s.add_folder("B");
        let b2 = s.add_folder_in("B2", Some(b)).unwrap();
        let b3 = s.add_folder_in("B3", Some(b2)).unwrap();
        // B (height 3) under A2 (level 2) would reach level 5: allowed.
        assert!(s.can_move(b, Some(a2)));
        // A leaf under the level-3 folder B3 is level 4: fine; a 2-level subtree under B3 reaches level 5: fine.
        assert!(s.can_move(a2, Some(b3)));
        // Make A a 2-level tree under B3 (level 4) first, then try one more level.
        assert!(s.move_folder_to(a, Some(b3), 0));
        assert_eq!(s.depth(a), 4);
        assert_eq!(s.depth(a2), 5);
        // Now a 3-level subtree cannot go anywhere deeper than level 3.
        let c = s.add_folder("C");
        let c2 = s.add_folder_in("C2", Some(c)).unwrap();
        let c3 = s.add_folder_in("C3", Some(c2)).unwrap();
        assert!(!s.can_move(c, Some(a)), "C(3 levels) under A(level 4) -> 7");
        assert!(!s.can_move(c, Some(a2)));
        assert!(!s.can_move(c, Some(b3)), "B3 is level 3: 3 + 3 = 6");
        assert!(s.can_move(c, Some(b2)), "B2 is level 2: 2 + 3 = 5");
        // Even a leaf cannot become a child of a level-5 folder.
        assert!(!s.can_move(c3, Some(a2)), "A2 is level 5, so no child fits below it");
        let before = paths(&s);
        assert!(!s.move_folder_to(c, Some(a), 0));
        assert_eq!(paths(&s), before, "a refused move changes nothing");
        assert!(s.move_folder_to(c, Some(b2), 0));
        assert_eq!(s.depth(c3), 5);
    }

    #[test]
    fn self_and_descendant_guard() {
        let mut s = temp_store();
        let a = s.add_folder("A");
        let b = s.add_folder_in("B", Some(a)).unwrap();
        let c = s.add_folder_in("C", Some(b)).unwrap();
        let other = s.add_folder("Other");
        assert!(!s.can_move(a, Some(a)));
        assert!(!s.can_move(a, Some(b)));
        assert!(!s.can_move(a, Some(c)));
        assert!(!s.can_move(b, Some(c)));
        assert!(s.can_move(c, Some(other)));
        let before = paths(&s);
        assert!(!s.move_folder_to(a, Some(c), 0));
        assert!(!s.drop_folder(a, DropAt::Inside(c)));
        assert!(!s.drop_folder(a, DropAt::Before(c)));
        assert!(!s.drop_folder(a, DropAt::Inside(a)));
        assert_eq!(paths(&s), before);
        // Unknown folders are refused too.
        assert!(!s.can_move(999, None));
        assert!(!s.can_move(a, Some(999)));
    }

    #[test]
    fn reorder_inside_a_parent() {
        let mut s = temp_store();
        let p = s.add_folder("P");
        let other = s.add_folder("Z");
        let k: Vec<FolderId> = ["a", "b", "c"].iter().map(|n| s.add_folder_in(n, Some(p)).unwrap()).collect();
        let order = |s: &Store| s.children(Some(p)).iter().map(|c| s.folder(*c).unwrap().name.clone()).collect::<Vec<_>>().join("");
        assert_eq!(order(&s), "abc");
        assert!(s.move_sibling(k[0], 1));
        assert_eq!(order(&s), "bac");
        assert!(s.move_sibling(k[2], -1));
        assert_eq!(order(&s), "bca");
        assert!(!s.move_sibling(k[1], -1), "first stays first");
        assert!(!s.move_sibling(k[0], 1), "last stays last");
        // Drag: before / inside / end, within one parent.
        assert!(s.drop_folder(k[0], DropAt::Before(k[1])));
        assert_eq!(order(&s), "abc");
        assert!(!s.drop_folder(k[0], DropAt::Before(k[1])), "no change");
        assert!(!s.drop_folder(k[0], DropAt::Before(k[0])));
        assert!(s.drop_folder(k[0], DropAt::Inside(p)));
        assert_eq!(order(&s), "bca", "dropped onto its own parent = last child");
        // The top level is untouched.
        assert_eq!(s.children(None), vec![p, other]);
        // Ids stay attached to their names.
        assert_eq!(s.folder(k[0]).unwrap().name, "a");
        // Out-of-range indexes clamp.
        assert!(s.move_folder_to(k[2], Some(p), 99));
        assert_eq!(order(&s), "bac");
    }

    #[test]
    fn moving_to_the_top_level() {
        let mut s = temp_store();
        let a = s.add_folder("A");
        let b = s.add_folder("B");
        let a1 = s.add_folder_in("A1", Some(a)).unwrap();
        let a11 = s.add_folder_in("A11", Some(a1)).unwrap();
        // Between the top-level rows, with the subtree coming along.
        assert!(s.drop_folder(a1, DropAt::Before(b)));
        assert_eq!(s.children(None), vec![a, a1, b]);
        assert_eq!(s.folder(a1).unwrap().parent, None);
        assert_eq!(s.folder(a11).unwrap().parent, Some(a1));
        assert_eq!(s.depth(a11), 2);
        // Past the end of the list.
        assert!(s.drop_folder(a11, DropAt::End));
        assert_eq!(s.children(None), vec![a, a1, b, a11]);
        // Already last on the top level: nothing to do.
        assert!(!s.drop_folder(a11, DropAt::End));
        // Context menu path: "Top level".
        assert!(s.move_folder_to(b, Some(a), 0));
        assert!(s.move_folder_to(b, None, 0));
        assert_eq!(s.children(None)[0], b);
        assert_eq!(paths(&s), vec!["B", "A", "A1", "A11"]);
    }

    #[test]
    fn nest_inside_by_drop() {
        let mut s = temp_store();
        let a = s.add_folder("A");
        let b = s.add_folder("B");
        let c = s.add_folder("C");
        assert!(s.drop_folder(c, DropAt::Inside(a)));
        assert!(s.drop_folder(b, DropAt::Inside(a)));
        assert_eq!(s.children(Some(a)), vec![c, b]);
        assert!(s.folder(a).unwrap().expanded);
        assert_eq!(paths(&s), vec!["A", "A/C", "A/B"]);
        // Before a nested folder lands at its level.
        let d = s.add_folder("D");
        assert!(s.drop_folder(d, DropAt::Before(b)));
        assert_eq!(s.children(Some(a)), vec![c, d, b]);
    }

    #[test]
    fn deleting_a_subtree() {
        let mut s = temp_store();
        let a = s.add_folder("A");
        let b = s.add_folder_in("B", Some(a)).unwrap();
        let c = s.add_folder_in("C", Some(b)).unwrap();
        let keep = s.add_folder("Keep");
        let Added::New(l1) = s.add_url("https://1.io", Some(Some(a))) else { panic!() };
        let Added::New(l2) = s.add_url("https://2.io", Some(Some(c))) else { panic!() };
        let Added::New(l3) = s.add_url("https://3.io", Some(Some(keep))) else { panic!() };
        s.lib.settings.default_folder = Some(b);
        assert_eq!(s.count_deep(a), 2);
        assert_eq!(s.count_deep(b), 1);
        assert_eq!(s.subtree(a).len(), 3);
        s.delete_folder(b);
        assert!(s.folder(b).is_none() && s.folder(c).is_none());
        assert!(s.folder(a).is_some());
        assert_eq!(s.link(l2).unwrap().folder, None, "links of nested folders stay in the library");
        assert_eq!(s.link(l1).unwrap().folder, Some(a));
        assert_eq!(s.link(l3).unwrap().folder, Some(keep));
        assert_eq!(s.lib.links.len(), 3);
        assert_eq!(s.lib.settings.default_folder, None);
        s.delete_folder(a);
        assert_eq!(s.lib.folders.len(), 1);
        assert_eq!(s.link(l1).unwrap().folder, None);
    }

    #[test]
    fn expanded_state_is_persisted() {
        let dir = temp_dir("expanded");
        let p = dir.join("library.json");
        let mut s = Store::load_from(Some(p.clone()));
        let a = s.add_folder("A");
        let b = s.add_folder_in("B", Some(a)).unwrap();
        let _c = s.add_folder_in("C", Some(b)).unwrap();
        // Adding a subfolder opens its parent.
        assert!(s.folder(a).unwrap().expanded && s.folder(b).unwrap().expanded);
        s.set_expanded(b, false);
        assert_eq!(s.tree(false).len(), 2, "C is hidden under collapsed B");
        assert_eq!(s.tree(true).len(), 3);
        let rev = s.rev;
        s.set_expanded(b, false);
        assert_eq!(s.rev, rev, "no change, no write");
        assert!(s.save_now());
        let s2 = Store::load_from(Some(p.clone()));
        assert!(s2.folder(a).unwrap().expanded);
        assert!(!s2.folder(b).unwrap().expanded);
        assert_eq!(s2.folder(b).unwrap().parent, Some(a));
        assert_eq!(s2.tree(false).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn broken_tree_is_repaired() {
        let dir = temp_dir("repair");
        let p = dir.join("library.json");
        let v2 = r#"{"version":2,"next_id":10,"folders":[
            {"id":1,"name":"loop1","parent":2},{"id":2,"name":"loop2","parent":1},
            {"id":3,"name":"orphan","parent":77},{"id":4,"name":"ok"},{"id":5,"name":"kid","parent":4}],"links":[]}"#;
        std::fs::write(&p, v2).unwrap();
        let s = Store::load_from(Some(p));
        for f in &s.lib.folders {
            assert!(s.chain_len(f.id).is_some_and(|d| d <= MAX_DEPTH), "{}", f.name);
        }
        assert_eq!(s.folder(3).unwrap().parent, None);
        assert_eq!(s.folder(5).unwrap().parent, Some(4));
        assert_eq!(s.lib.folders.len(), 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn drop_zones_split_a_row_in_thirds() {
        assert_eq!(drop_zone(1, Some(2), 0.0, 36.0, 3.0), DropAt::Before(1));
        assert_eq!(drop_zone(1, Some(2), 0.0, 36.0, 18.0), DropAt::Inside(1));
        assert_eq!(drop_zone(1, Some(2), 0.0, 36.0, 33.0), DropAt::Before(2));
        assert_eq!(drop_zone(1, None, 0.0, 36.0, 33.0), DropAt::End);
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
        let dir = temp_dir("roundtrip");
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
