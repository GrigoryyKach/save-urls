//! NIMVORO shared module: update check via GitHub Releases, hand-off of the update to the launcher,
//! and self-update (`selfupdate.rs`) when the launcher is not installed.
//!
//! Copied byte for byte into every Rust tool of the suite. Needs no egui and no HTTP crate:
//! the request is made by `curl` (`curl.exe` on Windows), which ships with Windows and with Linux. The contract is described in
//! `nimvoro/docs/ADDING_A_TOOL.md`; the self-update in `tasks/selfupdate-design.md`.
//!
//! # Usage
//!
//! ```ignore
//! let updater = Updater::new("random-wheel", "GrigoryyKach/random-wheel", env!("CARGO_PKG_VERSION"));
//! updater.check(move || ctx.request_repaint());       // background check
//! updater.status_text(lang)                           // localized status line for Settings
//! if let UpdateState::Available { version, .. } = updater.state() { /* banner */ }
//! if updater.apply() { /* update handed to the launcher: save and close the window */ }
//! // Without the launcher `apply()` starts the self-update (returns false); show `updater.phase()` (see `update_ui.rs`).
//! let t = nimvoro::strings(lang);                     // shared captions: t.updates, t.check_now, ...
//! ```

use crate::i18n::Lang;
use crate::selfupdate::{self, Phase, Release, Route, UpdateError};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
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
    release: Arc<Mutex<Option<Release>>>,
    phase: selfupdate::Shared,
    restart: selfupdate::Restart,
}

impl Updater {
    pub fn new(id: &'static str, repo: &'static str, current: &'static str) -> Self {
        Self { id, repo, current, state: Arc::new(Mutex::new(UpdateState::Idle)),
            release: Arc::new(Mutex::new(None)),
            phase: Arc::new(Mutex::new(Phase::Idle)),
            restart: selfupdate::Restart::default(),
        }
    }

    /// Arguments for the restarted process after a self-update (default: the current ones).
    #[allow(dead_code)]
    pub fn relaunch_args(mut self, args: &[&str]) -> Self {
        self.restart.args = Some(args.iter().map(|a| a.to_string()).collect());
        self
    }

    /// Quits a tray-only resident by its normal path once the new process has started (see `Restart::exit`).
    #[allow(dead_code)]
    pub fn exit_with(mut self, f: fn()) -> Self {
        self.restart.exit = Some(f);
        self
    }

    /// Runs before the launcher is started or the new process is spawned: stop a resident process that holds
    /// the app's files or its single-instance lock.
    #[allow(dead_code)]
    pub fn before_restart(mut self, f: fn()) -> Self {
        self.restart.before = Some(f);
        self
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
            let st = match selfupdate::fetch_release(me.id, me.repo) {
                Some(r) if is_newer(&r.tag, me.current) => {
                    let st = UpdateState::Available { version: r.tag.trim_start_matches('v').to_string(), url: r.html_url.clone() };
                    if let Ok(mut g) = me.release.lock() {
                        *g = Some(r);
                    }
                    st
                }
                Some(_) => UpdateState::UpToDate,
                None => UpdateState::Failed,
            };
            let found = matches!(st, UpdateState::Available { .. });
            me.set(st);
            // Test hook: `<ID>_AUTO_UPDATE=1` starts the update as soon as a newer version is found.
            if found && std::env::var(selfupdate::env_name(me.id, "AUTO_UPDATE")).is_ok_and(|v| v == "1") {
                me.apply();
            }
            on_done();
        });
    }

    fn set(&self, st: UpdateState) {
        if let Ok(mut s) = self.state.lock() {
            *s = st;
        }
    }

    /// Starts the update: through the launcher if it is installed, otherwise by self-update when the app folder
    /// is writable, otherwise opens the release page.
    /// Returns `true` when the update was handed to the launcher and the tool should exit.
    /// A self-update runs in the background (`phase()`) and restarts the process by itself.
    pub fn apply(&self) -> bool {
        if self.is_busy() {
            return false;
        }
        let dir = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from));
        let writable = dir.as_deref().is_some_and(selfupdate::dir_writable);
        match selfupdate::choose_route(launcher_path(), writable) {
            Route::Launcher(exe) => {
                if let Some(f) = self.restart.before {
                    f();
                }
                if Command::new(&exe).args(["--update", self.id]).spawn().is_ok() {
                    return true;
                }
            }
            Route::SelfUpdate => {
                let release = self.release.lock().ok().and_then(|g| g.clone());
                if let Some(r) = release {
                    selfupdate::set_phase(&self.phase, Phase::Downloading { done: 0, total: 0 });
                    selfupdate::run(self.id, r, self.phase.clone(), self.restart.clone());
                    return false;
                }
            }
            Route::OpenPage => {}
        }
        let url = match self.state() {
            UpdateState::Available { url, .. } => url,
            _ => format!("https://github.com/{}/releases/latest", self.repo),
        };
        open_url(&url);
        false
    }

    /// Opens the GitHub release page (the manual fallback).
    pub fn open_release_page(&self) {
        let url = match self.state() {
            UpdateState::Available { url, .. } if !url.is_empty() => url,
            _ => format!("https://github.com/{}/releases/latest", self.repo),
        };
        open_url(&url);
    }

    /// Progress of a running self-update.
    pub fn phase(&self) -> Phase {
        self.phase.lock().map(|p| p.clone()).unwrap_or(Phase::Idle)
    }

    /// `true` while a self-update downloads, installs or restarts.
    pub fn is_busy(&self) -> bool {
        !matches!(self.phase(), Phase::Idle | Phase::Failed(_))
    }

    /// `apply()` after the app has saved its data: `saved == false` aborts with an error and the app keeps running.
    pub fn apply_if(&self, saved: bool) -> bool {
        if !saved {
            selfupdate::set_phase(&self.phase, Phase::Failed(UpdateError::SaveFailed));
            return false;
        }
        self.apply()
    }

    /// `true` once the new version has been started: save and close by the normal path (the new process waits for us).
    pub fn should_exit(&self) -> bool {
        self.phase() == Phase::Restarting
    }

    /// Forgets a failed self-update (back to "available").
    pub fn dismiss_error(&self) {
        selfupdate::set_phase(&self.phase, Phase::Idle);
    }

    /// One line describing the self-update phase, with `12.3 / 45.6 MB` while downloading.
    pub fn phase_text(&self, lang: Lang) -> Option<String> {
        let t = strings(lang);
        Some(match self.phase() {
            Phase::Idle => return None,
            Phase::Downloading { done, total } => {
                let mb = |b: u64| b as f64 / 1_048_576.0;
                if total > 0 {
                    format!("{} {:.1} / {:.1} MB", t.downloading, mb(done), mb(total))
                } else {
                    t.downloading.to_string()
                }
            }
            Phase::Verifying => t.verifying.to_string(),
            Phase::Installing => t.installing.to_string(),
            Phase::Restarting => t.restarting.to_string(),
            Phase::Failed(e) => error_text(t, e).to_string(),
        })
    }

    /// Localized text for the status line in Settings.
    pub fn status_text(&self, lang: Lang) -> String {
        let t = strings(lang);
        if let Some(text) = self.phase_text(lang) {
            return text;
        }
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
    pub downloading: &'static str,
    pub verifying: &'static str,
    pub installing: &'static str,
    pub restarting: &'static str,
    pub try_again: &'static str,
    pub download_manually: &'static str,
    pub err_download: &'static str,
    pub err_digest: &'static str,
    pub err_nodigest: &'static str,
    pub err_package: &'static str,
    pub err_install: &'static str,
    pub err_restart: &'static str,
    pub err_save: &'static str,
}

fn error_text(t: &Strings, e: UpdateError) -> &'static str {
    match e {
        UpdateError::Download => t.err_download,
        UpdateError::BadDigest => t.err_digest,
        UpdateError::NoDigest => t.err_nodigest,
        UpdateError::NoAsset | UpdateError::Extract | UpdateError::BadPackage => t.err_package,
        UpdateError::SwapFailed => t.err_install,
        UpdateError::Restart => t.err_restart,
        UpdateError::SaveFailed => t.err_save,
    }
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
    downloading: "Downloading update…",
    verifying: "Verifying…",
    installing: "Installing…",
    restarting: "Restarting…",
    try_again: "Try again",
    download_manually: "Download manually",
    err_download: "Download failed. Check your connection.",
    err_digest: "The file did not match its checksum and was not installed.",
    err_nodigest: "The release has no checksum, so it was not installed.",
    err_package: "The update package is not valid for this app.",
    err_install: "Could not replace the files. Nothing was changed.",
    err_restart: "Updated, but the app could not restart. Start it again by hand.",
    err_save: "Could not save your data, so the update was not started.",
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
    downloading: "Загружаем обновление…",
    verifying: "Проверяем…",
    installing: "Устанавливаем…",
    restarting: "Перезапускаем…",
    try_again: "Повторить",
    download_manually: "Скачать вручную",
    err_download: "Не удалось загрузить. Проверьте соединение.",
    err_digest: "Файл не совпал с контрольной суммой и не установлен.",
    err_nodigest: "У релиза нет контрольной суммы, поэтому он не установлен.",
    err_package: "Пакет обновления не подходит этому приложению.",
    err_install: "Не удалось заменить файлы. Ничего не изменено.",
    err_restart: "Обновлено, но приложение не перезапустилось. Запустите его вручную.",
    err_save: "Не удалось сохранить данные, поэтому обновление не начато.",
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
    downloading: "Завантажуємо оновлення…",
    verifying: "Перевіряємо…",
    installing: "Встановлюємо…",
    restarting: "Перезапускаємо…",
    try_again: "Повторити",
    download_manually: "Завантажити вручну",
    err_download: "Не вдалося завантажити. Перевірте з'єднання.",
    err_digest: "Файл не збігся з контрольною сумою й не встановлений.",
    err_nodigest: "У релізу немає контрольної суми, тому його не встановлено.",
    err_package: "Пакет оновлення не підходить цьому застосунку.",
    err_install: "Не вдалося замінити файли. Нічого не змінено.",
    err_restart: "Оновлено, але застосунок не перезапустився. Запустіть його вручну.",
    err_save: "Не вдалося зберегти дані, тому оновлення не розпочато.",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Ru => &RU,
        Lang::Uk => &UK,
    }
}

/// Folder of the NIMVORO launcher data: `%APPDATA%\NIMVORO` on Windows,
/// `$XDG_CONFIG_HOME/NIMVORO` (default `~/.config/NIMVORO`) on Linux.
fn launcher_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        Some(PathBuf::from(std::env::var_os("APPDATA")?).join("NIMVORO"))
    }
    #[cfg(not(windows))]
    {
        let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            Some(x) => PathBuf::from(x),
            None => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        };
        Some(base.join("NIMVORO"))
    }
}

/// Path to the launcher (NIMVORO.exe, or the AppImage / binary on Linux), which it records in `launcher.json`.
pub fn launcher_path() -> Option<PathBuf> {
    let text = std::fs::read_to_string(launcher_dir()?.join("launcher.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let p = PathBuf::from(v.get("path")?.as_str()?);
    p.exists().then_some(p)
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
    #[cfg(windows)]
    let _ = Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).creation_flags(CREATE_NO_WINDOW).spawn();
    #[cfg(not(windows))]
    let _ = Command::new("xdg-open").arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
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
