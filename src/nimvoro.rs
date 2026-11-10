//! NIMVORO: проверка обновлений через GitHub Releases и передача обновления лаунчеру.
//!
//! Общий модуль для всех Rust-утилит набора, копируется без изменений.
//! Не зависит от egui и сторонних HTTP-библиотек: запрос делает встроенный в Windows `curl.exe`.
//! Контракт описан в `nimvoro/docs/ADDING_A_TOOL.md`.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, PartialEq, Debug)]
pub enum UpdateState {
    /// Проверка ещё не запускалась.
    Idle,
    Checking,
    UpToDate,
    Available { version: String, url: String },
    Failed,
}

#[derive(Clone)]
pub struct Updater {
    /// id утилиты в каталоге NIMVORO.
    pub id: &'static str,
    /// `owner/repo` на GitHub.
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

    /// Фоновая проверка. `on_done` вызывается из фонового потока (например, `ctx.request_repaint()`).
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

    /// Запускает обновление: через лаунчер, если он установлен, иначе открывает страницу релиза.
    /// Возвращает `true`, если обновление передано лаунчеру и утилите пора завершиться.
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

    /// Текст для строки состояния в настройках.
    pub fn status_text(&self) -> String {
        match self.state() {
            UpdateState::Idle => format!("Установлена версия {}", self.current),
            UpdateState::Checking => "Проверяем обновления…".into(),
            UpdateState::UpToDate => format!("Версия {} — последняя", self.current),
            UpdateState::Available { version, .. } => format!("Доступна версия {version}"),
            UpdateState::Failed => "Не удалось проверить обновления".into(),
        }
    }
}

/// Путь к NIMVORO.exe, который лаунчер записывает в `%APPDATA%\NIMVORO\launcher.json`.
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

/// `true`, если `candidate` новее `current` (semver без pre-release).
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
