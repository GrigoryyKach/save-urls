//! NIMVORO shared module: update check via GitHub Releases and hand-off of the update to the launcher.
//!
//! Copied byte for byte into every Rust tool of the suite. Needs no egui and no HTTP crate:
//! the request is made by `curl.exe`, which ships with Windows. The contract is described in
//! `nimvoro/docs/ADDING_A_TOOL.md`.
//!
//! # Usage
//!
//! ```ignore
//! let updater = Updater::new("random-wheel", "GrigoryyKach/random-wheel", env!("CARGO_PKG_VERSION"));
//! updater.check(move || ctx.request_repaint());       // background check
//! updater.status_text(lang)                           // localized status line for Settings
//! if let UpdateState::Available { version, .. } = updater.state() { /* banner */ }
//! if updater.apply() { /* update handed to the launcher: save and close the window */ }
//! let t = nimvoro::strings(lang);                     // shared captions: t.updates, t.check_now, ...
//! ```

use crate::i18n::Lang;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, PartialEq, Debug)]
pub enum UpdateState {
    /// No check has been started yet.
    Idle,
    Checking,
    UpToDate,
    Available { version: String, url: String },
    Failed,
}

#[derive(Clone)]
pub struct Updater {
    /// Tool id in the NIMVORO catalog.
    pub id: &'static str,
    /// `owner/repo` on GitHub.
    pub repo: &'static str,
    pub current: &'static str,
    state: Arc<Mutex<UpdateState>>,
}

impl Updater {
    pub fn new(id: &'static str, repo: &'static str, current: &'static str) -> Self {
        Self { id, repo, current, state: Arc::new(Mutex::new(UpdateState::Idle)) }
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().map(|s| s.clone()).unwrap_or(UpdateState::Failed)
    }

    pub fn is_checking(&self) -> bool {
        self.state() == UpdateState::Checking
    }

    /// Background check. `on_done` runs on the worker thread (e.g. `ctx.request_repaint()`).
    pub fn check(&self, on_done: impl FnOnce() + Send + 'static) {
        if self.is_checking() {
            return;
        }
        self.set(UpdateState::Checking);
        let me = self.clone();
        std::thread::spawn(move || {
            let st = match latest_release(me.repo) {
                Some((tag, url)) if is_newer(&tag, me.current) => {
                    UpdateState::Available { version: tag.trim_start_matches('v').to_string(), url }
                }
                Some(_) => UpdateState::UpToDate,
                None => UpdateState::Failed,
            };
            me.set(st);
            on_done();
        });
    }

    fn set(&self, st: UpdateState) {
        if let Ok(mut s) = self.state.lock() {
            *s = st;
        }
    }

    /// Starts the update through the launcher if it is installed, otherwise opens the release page.
    /// Returns `true` when the update was handed to the launcher and the tool should exit.
    pub fn apply(&self) -> bool {
        if let Some(exe) = launcher_path() {
            let spawned = Command::new(&exe).args(["--update", self.id]).spawn().is_ok();
            if spawned {
                return true;
            }
        }
        let url = match self.state() {
            UpdateState::Available { url, .. } => url,
            _ => format!("https://github.com/{}/releases/latest", self.repo),
        };
        open_url(&url);
        false
    }

    /// Localized text for the status line in Settings.
    pub fn status_text(&self, lang: Lang) -> String {
        let t = strings(lang);
        match self.state() {
            UpdateState::Idle => t.installed.replace("{v}", self.current),
            UpdateState::Checking => t.checking.to_string(),
            UpdateState::UpToDate => t.latest.replace("{v}", self.current),
            UpdateState::Available { version, .. } => t.available.replace("{v}", &version),
            UpdateState::Failed => t.failed.to_string(),
        }
    }
}

/// Captions shared by all tools. `{v}` is replaced with a version number.
pub struct Strings {
    pub updates: &'static str,
    pub check_on_start: &'static str,
    pub check_on_start_hint: &'static str,
    pub check_now: &'static str,
    pub update_btn: &'static str,
    pub installed: &'static str,
    pub checking: &'static str,
    pub latest: &'static str,
    pub available: &'static str,
    pub failed: &'static str,
    pub you_have: &'static str,
}

const EN: Strings = Strings {
    updates: "Updates",
    check_on_start: "Check on startup",
    check_on_start_hint: "Look for new versions on GitHub and update through NIMVORO",
    check_now: "Check",
    update_btn: "Update",
    installed: "Version {v} installed",
    checking: "Checking for updates…",
    latest: "Version {v} is the latest",
    available: "Version {v} is available",
    failed: "Could not check for updates",
    you_have: "You have {v}",
};

const RU: Strings = Strings {
    updates: "Обновления",
    check_on_start: "Проверять при запуске",
    check_on_start_hint: "Узнавать о новых версиях на GitHub и обновляться через NIMVORO",
    check_now: "Проверить",
    update_btn: "Обновить",
    installed: "Установлена версия {v}",
    checking: "Проверяем обновления…",
    latest: "Версия {v} — последняя",
    available: "Доступна версия {v}",
    failed: "Не удалось проверить обновления",
    you_have: "У вас {v}",
};

const UK: Strings = Strings {
    updates: "Оновлення",
    check_on_start: "Перевіряти під час запуску",
    check_on_start_hint: "Дізнаватися про нові версії на GitHub і оновлюватися через NIMVORO",
    check_now: "Перевірити",
    update_btn: "Оновити",
    installed: "Встановлено версію {v}",
    checking: "Перевіряємо оновлення…",
    latest: "Версія {v} — остання",
    available: "Доступна версія {v}",
    failed: "Не вдалося перевірити оновлення",
    you_have: "У вас {v}",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Ru => &RU,
        Lang::Uk => &UK,
    }
}

/// Path to NIMVORO.exe, which the launcher records in `%APPDATA%\NIMVORO\launcher.json`.
pub fn launcher_path() -> Option<PathBuf> {
    let dir = std::env::var_os("APPDATA")?;
    let text = std::fs::read_to_string(PathBuf::from(dir).join("NIMVORO").join("launcher.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let p = PathBuf::from(v.get("path")?.as_str()?);
    p.exists().then_some(p)
}

fn latest_release(repo: &str) -> Option<(String, String)> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let out = Command::new("curl.exe")
        .args(["-sfL", "--max-time", "10", "-H", "Accept: application/vnd.github+json", "-A", "NIMVORO-updater", &url])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let html = v.get("html_url").and_then(|u| u.as_str()).unwrap_or_default().to_string();
    Some((tag, html))
}

fn parse(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// `true` if `candidate` is newer than `current` (semver without pre-release).
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (a, b) = (parse(candidate), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn open_url(url: &str) {
    let _ = Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).creation_flags(CREATE_NO_WINDOW).spawn();
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("v2", "1.99.0"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("v1.0.0-beta", "1.0.0"));
    }
}
