//! NIMVORO shared module: self-update of a tool that was installed without the launcher.
//!
//! Copied byte for byte into every Rust tool of the suite. No HTTP, zip or hash crates: downloads go through
//! `curl`, archives through `tar` (bsdtar reads zip on Windows), and SHA-256 is implemented here.
//! The behaviour is described in `tasks/selfupdate-design.md`; `nimvoro.rs` wires it into `Updater::apply`.
//!
//! Flow: download -> verify SHA-256 (GitHub `digest`) -> extract to `.update-staging` -> swap files
//! (`*.old` backups, rollback on failure) -> relaunch with `NIMVORO_WAIT_PID` -> next start cleans up.

use serde_json::Value;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
#[cfg(not(windows))]
use std::time::Instant;
use std::time::Duration;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Files that an update never replaces (the launcher language marker and portable user data).
pub const PROTECTED: &[&str] = &[".nimvoro.json", "library.json", "config.json", "settings.json"];
const STAGING: &str = ".update-staging";
const OLD_LIST: &str = ".update-old.txt";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UpdateError {
    NoAsset,
    NoDigest,
    Download,
    BadDigest,
    Extract,
    BadPackage,
    SwapFailed,
    Restart,
    /// The app could not save its data before the update; nothing was changed.
    SaveFailed,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Phase {
    Idle,
    Downloading { done: u64, total: u64 },
    Verifying,
    Installing,
    Restarting,
    Failed(UpdateError),
}

pub type Shared = Arc<Mutex<Phase>>;

pub fn set_phase(s: &Shared, p: Phase) {
    if let Ok(mut g) = s.lock() {
        *g = p;
    }
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub digest: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

/// What a click on "Update" does.
#[derive(Debug, PartialEq)]
pub enum Route {
    /// Hand over to `NIMVORO.exe --update <id>`.
    Launcher(PathBuf),
    SelfUpdate,
    /// The folder is not writable: open the release page.
    OpenPage,
}

pub fn choose_route(launcher: Option<PathBuf>, dir_writable: bool) -> Route {
    match launcher {
        Some(p) => Route::Launcher(p),
        None if dir_writable => Route::SelfUpdate,
        None => Route::OpenPage,
    }
}

/// Env var that replaces the release API URL (tests): `random-wheel` -> `RANDOM_WHEEL_UPDATE_API`.
pub fn env_name(id: &str, suffix: &str) -> String {
    format!("{}_{}", id.to_uppercase().replace('-', "_"), suffix)
}

pub fn api_url(id: &str, repo: &str) -> (String, bool) {
    match std::env::var(env_name(id, "UPDATE_API")) {
        Ok(v) if !v.is_empty() => (v, true),
        _ => (format!("https://api.github.com/repos/{repo}/releases/latest"), false),
    }
}

fn curl() -> Command {
    let mut c = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

pub fn parse_release(v: &Value) -> Option<Release> {
    let tag = v.get("tag_name")?.as_str()?.to_string();
    let html_url = v.get("html_url").and_then(|u| u.as_str()).unwrap_or_default().to_string();
    let assets = v
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(Asset {
                        name: x.get("name")?.as_str()?.to_string(),
                        url: x.get("browser_download_url")?.as_str()?.to_string(),
                        size: x.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                        digest: x.get("digest").and_then(|d| d.as_str()).map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Release { tag, html_url, assets })
}

pub fn fetch_release(id: &str, repo: &str) -> Option<Release> {
    let (url, _) = api_url(id, repo);
    let out = curl()
        .args(["-sfL", "--max-time", "10", "-H", "Accept: application/vnd.github+json", "-A", "NIMVORO-updater", &url])
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_release(&serde_json::from_slice(&out.stdout).ok()?)
}

/// The archive for this platform: `*-win-x64.zip` or `*-linux-x64.tar.gz`.
pub fn pick_asset(r: &Release, windows: bool) -> Option<&Asset> {
    let suffix = if windows { "-win-x64.zip" } else { "-linux-x64.tar.gz" };
    r.assets.iter().find(|a| a.name.ends_with(suffix))
}

/// Folder of the running exe, if it can be written to.
pub fn dir_writable(dir: &Path) -> bool {
    let probe = dir.join(".nimvoro-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

// ───────────────────────────── SHA-256 ─────────────────────────────

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
    0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
    0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
    0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    h: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19],
            buf: Vec::with_capacity(64),
            len: 0,
        }
    }

    fn block(h: &mut [u32; 8], b: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b_, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b_) ^ (a & c) ^ (b_ & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b_;
            b_ = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b_, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        if !self.buf.is_empty() {
            let need = 64 - self.buf.len();
            let take = need.min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == 64 {
                let b = std::mem::take(&mut self.buf);
                Self::block(&mut self.h, &b);
            }
        }
        while data.len() >= 64 {
            Self::block(&mut self.h, &data[..64]);
            data = &data[64..];
        }
        self.buf.extend_from_slice(data);
    }

    pub fn finish(mut self) -> String {
        let bits = self.len * 8;
        let mut pad = vec![0x80u8];
        while (self.buf.len() + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&bits.to_be_bytes());
        let total = self.len;
        self.update(&pad);
        self.len = total;
        self.h.iter().map(|x| format!("{x:08x}")).collect()
    }
}

#[cfg(test)]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut s = Sha256::new();
    s.update(bytes);
    s.finish()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut s = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        s.update(&buf[..n]);
    }
    Ok(s.finish())
}

/// Checks a file against a GitHub `digest` (`sha256:<hex>`).
pub fn verify_digest(path: &Path, digest: Option<&str>) -> Result<(), UpdateError> {
    let want = digest.and_then(|d| d.strip_prefix("sha256:")).filter(|h| h.len() == 64).ok_or(UpdateError::NoDigest)?;
    let got = sha256_file(path).map_err(|_| UpdateError::Download)?;
    if got.eq_ignore_ascii_case(want) { Ok(()) } else { Err(UpdateError::BadDigest) }
}

// ───────────────────────────── swap ─────────────────────────────

fn files_in(root: &Path) -> Result<Vec<PathBuf>, UpdateError> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) -> Result<(), UpdateError> {
        for e in std::fs::read_dir(dir).map_err(|_| UpdateError::Extract)? {
            let e = e.map_err(|_| UpdateError::Extract)?;
            let ft = e.file_type().map_err(|_| UpdateError::Extract)?;
            if ft.is_symlink() {
                return Err(UpdateError::BadPackage);
            }
            if ft.is_dir() {
                walk(&e.path(), root, out)?;
            } else {
                out.push(e.path().strip_prefix(root).map_err(|_| UpdateError::BadPackage)?.to_path_buf());
            }
        }
        Ok(())
    }
    let mut v = Vec::new();
    walk(root, root, &mut v)?;
    v.sort();
    Ok(v)
}

fn is_protected(rel: &Path) -> bool {
    rel.file_name().and_then(|n| n.to_str()).is_some_and(|n| PROTECTED.contains(&n))
}

fn old_name(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

/// Moves the files of `staging` into `dir`. The old files become `*.old`. On failure everything is rolled back.
/// Returns the list of `*.old` files that exist now (to delete on the next start).
pub fn swap(staging: &Path, dir: &Path) -> Result<Vec<PathBuf>, UpdateError> {
    let files = files_in(staging)?;
    // (destination, had an old file that was renamed to *.old)
    let mut journal: Vec<(PathBuf, bool)> = Vec::new();
    let result = (|| -> std::io::Result<()> {
        for rel in &files {
            let dest = dir.join(rel);
            if is_protected(rel) && dest.exists() {
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut had = false;
            if dest.exists() {
                let old = old_name(&dest);
                let _ = std::fs::remove_file(&old);
                std::fs::rename(&dest, &old)?;
                had = true;
            }
            journal.push((dest.clone(), had));
            move_file(&staging.join(rel), &dest)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(journal.into_iter().filter(|(_, had)| *had).map(|(d, _)| old_name(&d)).collect()),
        Err(_) => {
            for (dest, had) in journal.into_iter().rev() {
                let _ = std::fs::remove_file(&dest);
                if had {
                    let _ = std::fs::rename(old_name(&dest), &dest);
                }
            }
            Err(UpdateError::SwapFailed)
        }
    }
}

/// Deletes leftovers of a previous update. Safe to call on every start.
pub fn cleanup(dir: &Path, tmp_root: &Path) {
    if let Ok(text) = std::fs::read_to_string(dir.join(OLD_LIST)) {
        for line in text.lines().filter(|l| l.ends_with(".old")) {
            let p = dir.join(line);
            // Only paths inside the app folder.
            if p.starts_with(dir) && !line.contains("..") {
                let _ = std::fs::remove_file(p);
            }
        }
        let _ = std::fs::remove_file(dir.join(OLD_LIST));
    }
    let _ = std::fs::remove_dir_all(dir.join(STAGING));
    let _ = std::fs::remove_dir_all(tmp_root);
}

// ───────────────────────────── process helpers ─────────────────────────────

#[cfg(windows)]
mod win {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        pub fn WaitForSingleObject(h: isize, ms: u32) -> u32;
        pub fn CloseHandle(h: isize) -> i32;
    }
}

/// Waits until the process `pid` has exited (at most `max`).
pub fn wait_for_exit(pid: u32, max: Duration) {
    #[cfg(windows)]
    unsafe {
        let h = win::OpenProcess(0x0010_0000, 0, pid); // SYNCHRONIZE
        if h != 0 {
            win::WaitForSingleObject(h, max.as_millis() as u32);
            win::CloseHandle(h);
        }
    }
    #[cfg(not(windows))]
    {
        let end = Instant::now() + max;
        while Path::new(&format!("/proc/{pid}")).exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn tmp_root(id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{id}-update"))
}

/// Call first in `main`: waits for the exiting predecessor after an update and removes its leftovers.
pub fn startup(id: &str) {
    if let Some(pid) = std::env::var("NIMVORO_WAIT_PID").ok().and_then(|v| v.parse::<u32>().ok()) {
        // SAFETY: called at the top of `main`, before any other thread exists.
        unsafe { std::env::remove_var("NIMVORO_WAIT_PID") };
        if pid != std::process::id() {
            wait_for_exit(pid, Duration::from_secs(15));
        }
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        cleanup(&dir, &tmp_root(id));
    }
}

fn extract(archive: &Path, to: &Path) -> Result<(), UpdateError> {
    let _ = std::fs::remove_dir_all(to);
    std::fs::create_dir_all(to).map_err(|_| UpdateError::Extract)?;
    let mut cmd;
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let tar = root.join("System32").join("tar.exe");
        cmd = Command::new(if tar.exists() { tar } else { PathBuf::from("tar") });
        cmd.arg("-xf");
    } else {
        cmd = Command::new("tar");
        cmd.arg("-xzf");
    }
    cmd.arg(archive).arg("-C").arg(to).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        _ => Err(UpdateError::Extract),
    }
}

fn download(a: &Asset, to: &Path, shared: &Shared, allow_http: bool) -> Result<(), UpdateError> {
    let ok_url = a.url.starts_with("https://") || (allow_http && a.url.starts_with("http://127.0.0.1"));
    if !ok_url {
        return Err(UpdateError::Download);
    }
    let _ = std::fs::remove_file(to);
    let mut child = curl()
        .args(["-fsSL", "--max-time", "900", "-A", "NIMVORO-updater", "-o"])
        .arg(to)
        .arg(&a.url)
        .spawn()
        .map_err(|_| UpdateError::Download)?;
    loop {
        let done = std::fs::metadata(to).map(|m| m.len()).unwrap_or(0);
        set_phase(shared, Phase::Downloading { done, total: a.size.max(done) });
        match child.try_wait() {
            Ok(Some(s)) if s.success() => break,
            Ok(Some(_)) | Err(_) => return Err(UpdateError::Download),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let len = std::fs::metadata(to).map(|m| m.len()).unwrap_or(0);
    set_phase(shared, Phase::Downloading { done: len, total: len });
    Ok(())
}

/// Everything up to and including the swap. Returns the path to relaunch.
pub fn install(id: &str, release: &Release, shared: &Shared, dir: &Path, exe: &Path, allow_http: bool) -> Result<(), UpdateError> {
    let asset = pick_asset(release, cfg!(windows)).ok_or(UpdateError::NoAsset)?;
    if asset.digest.is_none() {
        return Err(UpdateError::NoDigest);
    }
    let tmp = tmp_root(id);
    std::fs::create_dir_all(&tmp).map_err(|_| UpdateError::Download)?;
    let file = tmp.join(&asset.name);
    download(asset, &file, shared, allow_http)?;
    set_phase(shared, Phase::Verifying);
    if let Err(e) = verify_digest(&file, asset.digest.as_deref()) {
        let _ = std::fs::remove_file(&file);
        return Err(e);
    }
    set_phase(shared, Phase::Installing);
    let staging = dir.join(STAGING);
    let res = (|| {
        extract(&file, &staging)?;
        let name = exe.file_name().ok_or(UpdateError::BadPackage)?;
        if !staging.join(name).is_file() {
            return Err(UpdateError::BadPackage);
        }
        let olds = swap(&staging, dir)?;
        let list: String = olds.iter().filter_map(|p| p.strip_prefix(dir).ok()).map(|p| format!("{}\n", p.display())).collect();
        let _ = std::fs::write(dir.join(OLD_LIST), list);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_file(&file);
    res
}

/// Starts the new exe (with `args`, or with the current arguments); it waits for this process to exit.
pub fn relaunch(exe: &Path, args: Option<&[String]>) -> Result<(), UpdateError> {
    let mut cmd = Command::new(exe);
    match args {
        Some(a) => cmd.args(a),
        None => cmd.args(std::env::args_os().skip(1)),
    };
    cmd
        .env("NIMVORO_WAIT_PID", std::process::id().to_string())
        .stdin(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|_| UpdateError::Restart)
}

/// How the app is started again, and what must stop first (for example a resident process that holds a lock).
#[derive(Clone, Default)]
pub struct Restart {
    /// Arguments of the new process; `None` keeps the current ones.
    pub args: Option<Vec<String>>,
    /// Runs after the files were swapped and before the new process starts.
    pub before: Option<fn()>,
    /// Asks a process without a UI loop (a tray-only resident) to quit by its normal path once the new
    /// process has started. Windowed apps poll `Updater::should_exit` instead.
    pub exit: Option<fn()>,
}

/// Whole update on a worker thread; on success the process exits after the new one has been started.
pub fn run(id: &'static str, release: Release, shared: Shared, restart: Restart) {
    std::thread::spawn(move || {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(_) => return set_phase(&shared, Phase::Failed(UpdateError::BadPackage)),
        };
        let Some(dir) = exe.parent().map(Path::to_path_buf) else {
            return set_phase(&shared, Phase::Failed(UpdateError::BadPackage));
        };
        let allow_http = std::env::var(env_name(id, "UPDATE_API")).is_ok_and(|v| !v.is_empty());
        set_phase(&shared, Phase::Downloading { done: 0, total: 0 });
        if let Err(e) = install(id, &release, &shared, &dir, &exe, allow_http) {
            return set_phase(&shared, Phase::Failed(e));
        }
        if let Some(f) = restart.before {
            f();
        }
        if let Err(e) = relaunch(&exe, restart.args.as_deref()) {
            return set_phase(&shared, Phase::Failed(e));
        }
        // The new process waits for this one (`NIMVORO_WAIT_PID`). Phase `Restarting` tells the UI loop to save and
        // close by the app's normal path (`Updater::should_exit`); a resident without UI gets the `exit` hook.
        set_phase(&shared, Phase::Restarting);
        if let Some(f) = restart.exit {
            f();
        }
        // Last resort if nothing closed the app (everything was saved before the update started).
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("selfupdate-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn release(names: &[&str]) -> Release {
        Release {
            tag: "v1".into(),
            html_url: String::new(),
            assets: names
                .iter()
                .map(|n| Asset { name: n.to_string(), url: format!("https://x/{n}"), size: 1, digest: None })
                .collect(),
        }
    }

    #[test]
    fn picks_platform_asset() {
        let r = release(&["App-win-x64.zip", "App-linux-x64.tar.gz", "notes.txt"]);
        assert_eq!(pick_asset(&r, true).unwrap().name, "App-win-x64.zip");
        assert_eq!(pick_asset(&r, false).unwrap().name, "App-linux-x64.tar.gz");
        assert!(pick_asset(&release(&["notes.txt"]), true).is_none());
    }

    #[test]
    fn parses_release_json() {
        let v: Value = serde_json::from_str(
            r#"{"tag_name":"v2.0.0","html_url":"u","assets":[{"name":"A-win-x64.zip","browser_download_url":"https://d","size":5,"digest":"sha256:ab"}]}"#,
        )
        .unwrap();
        let r = parse_release(&v).unwrap();
        assert_eq!(r.tag, "v2.0.0");
        assert_eq!(r.assets[0].size, 5);
        assert_eq!(r.assets[0].digest.as_deref(), Some("sha256:ab"));
    }

    #[test]
    fn sha256_known_values() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1000];
        let mut s = Sha256::new();
        s.update(&long[..333]);
        s.update(&long[333..]);
        assert_eq!(s.finish(), sha256_hex(&long));
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn digest_good_bad_missing() {
        let d = tmp("digest");
        let f = d.join("f.bin");
        fs::write(&f, b"abc").unwrap();
        let good = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(verify_digest(&f, Some(good)), Ok(()));
        assert_eq!(verify_digest(&f, Some(&good.to_uppercase().replace("SHA256", "sha256"))), Ok(()));
        let bad = format!("sha256:{}", "0".repeat(64));
        assert_eq!(verify_digest(&f, Some(&bad)), Err(UpdateError::BadDigest));
        assert_eq!(verify_digest(&f, None), Err(UpdateError::NoDigest));
        assert_eq!(verify_digest(&f, Some("sha256:abc")), Err(UpdateError::NoDigest));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn route_choice() {
        let l = PathBuf::from("NIMVORO.exe");
        assert_eq!(choose_route(Some(l.clone()), true), Route::Launcher(l.clone()));
        assert_eq!(choose_route(Some(l.clone()), false), Route::Launcher(l));
        assert_eq!(choose_route(None, true), Route::SelfUpdate);
        assert_eq!(choose_route(None, false), Route::OpenPage);
    }

    #[test]
    fn env_names() {
        assert_eq!(env_name("random-wheel", "UPDATE_API"), "RANDOM_WHEEL_UPDATE_API");
        assert_eq!(env_name("snapper", "AUTO_UPDATE"), "SNAPPER_AUTO_UPDATE");
    }

    #[test]
    fn swap_keeps_protected_and_cleans_up() {
        let root = tmp("swap");
        let (app, stage) = (root.join("app"), root.join("app").join(STAGING));
        fs::create_dir_all(&stage).unwrap();
        fs::create_dir_all(stage.join("sub")).unwrap();
        fs::write(app.join("App.exe"), b"old").unwrap();
        fs::write(app.join(".nimvoro.json"), b"{\"language\":\"ru\"}").unwrap();
        fs::write(app.join("library.json"), b"mine").unwrap();
        fs::write(app.join("user.txt"), b"keep").unwrap();
        fs::write(stage.join("App.exe"), b"new").unwrap();
        fs::write(stage.join(".nimvoro.json"), b"{\"language\":\"en\"}").unwrap();
        fs::write(stage.join("library.json"), b"demo").unwrap();
        fs::write(stage.join("sub").join("icon.png"), b"png").unwrap();
        let olds = swap(&stage, &app).unwrap();
        assert_eq!(fs::read(app.join("App.exe")).unwrap(), b"new");
        assert_eq!(fs::read(app.join("App.exe.old")).unwrap(), b"old");
        assert_eq!(fs::read(app.join(".nimvoro.json")).unwrap(), b"{\"language\":\"ru\"}");
        assert_eq!(fs::read(app.join("library.json")).unwrap(), b"mine");
        assert_eq!(fs::read(app.join("user.txt")).unwrap(), b"keep");
        assert_eq!(fs::read(app.join("sub").join("icon.png")).unwrap(), b"png");
        assert_eq!(olds, vec![app.join("App.exe.old")]);
        // cleanup removes exactly the recorded *.old files and the staging folder
        fs::write(app.join("mine.old"), b"user file").unwrap();
        fs::write(app.join(OLD_LIST), "App.exe.old\n").unwrap();
        cleanup(&app, &root.join("tmp"));
        assert!(!app.join("App.exe.old").exists());
        assert!(app.join("mine.old").exists());
        assert!(!stage.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn swap_rolls_back_on_failure() {
        let root = tmp("rollback");
        let (app, stage) = (root.join("app"), root.join("app").join(STAGING));
        fs::create_dir_all(&stage).unwrap();
        fs::write(app.join("A.exe"), b"oldA").unwrap();
        fs::write(app.join("b.dll"), b"oldB").unwrap();
        fs::write(stage.join("A.exe"), b"newA").unwrap();
        fs::write(stage.join("b.dll"), b"newB").unwrap();
        // package has "sub/x", but the app folder has a plain file named "sub": the move fails after A and b were swapped
        fs::create_dir_all(stage.join("sub")).unwrap();
        fs::write(stage.join("sub").join("x"), b"newX").unwrap();
        fs::write(app.join("sub"), b"file").unwrap();
        assert_eq!(swap(&stage, &app), Err(UpdateError::SwapFailed));
        assert_eq!(fs::read(app.join("A.exe")).unwrap(), b"oldA");
        assert_eq!(fs::read(app.join("b.dll")).unwrap(), b"oldB");
        assert!(!app.join("A.exe.old").exists());
        assert_eq!(fs::read(app.join("sub")).unwrap(), b"file");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn symlink_free_walk_and_writable() {
        let d = tmp("walk");
        assert!(dir_writable(&d));
        assert!(!d.join(".nimvoro-write-test").exists());
        assert!(!dir_writable(&d.join("missing")));
        let _ = fs::remove_dir_all(d);
    }
}
