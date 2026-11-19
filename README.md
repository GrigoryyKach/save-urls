# Save URLs

A link library for Windows and Linux that lives in the tray. Copy the address of an interesting site, click the tray icon, and the link is saved;
the app takes its title from the page itself. In the window you can sort links into folders, describe them and find them quickly.

Written in Rust (eframe/egui): a single executable with no dependencies, and the library is stored only on your computer.
The look follows the shared NIMVORO base: `src/theme.rs` (copied from `../random_wheel` unchanged), a graphite neutral ramp,
IBM Plex Sans and Plex Mono (bundled, Latin and Cyrillic), hairline cards and sentence-case headings. Save URLs' own accent is the gold of its folder icon,
and every link row starts with a domain monogram tile that takes one quiet tint per folder.

## Languages

The interface is available in English (default), Russian and Ukrainian. Pick the language in Settings → Appearance; it applies immediately,
including the tray menu, tooltip and notifications.
The starting language is taken from the `language` setting in `library.json`, then from the `.nimvoro.json` marker next to the exe (written by the NIMVORO launcher),
and falls back to English. The result is saved to the library settings.

## Theme

Settings → Appearance → Theme: System (default, follows the Windows setting live; on Linux it follows the desktop setting), Light or Dark.

## Platforms

- **Windows** 10/11 x64: `SaveUrls-win-x64.zip` from the release, or via the NIMVORO launcher.
- **Linux** x86_64, X11 and Wayland (built on Ubuntu 22.04, glibc 2.35 or newer): `SaveUrls-linux-x64.tar.gz` from the release, or via the NIMVORO launcher.

## Run

```bash
cargo run --release
```

The ready exe is `target/release/SaveUrls.exe` (`target/release/SaveUrls` on Linux); you can copy it anywhere.
The `--background` flag starts the program straight into the tray without a window (this is how it starts with Windows or the desktop session).

### Linux

Unpack the archive and run `./save-urls`. Runtime needs: GTK 3 and `libayatana-appindicator3` (tray icon), `xdg-desktop-portal` (the export file dialog), `curl` (page titles) and `xdg-utils`
(opening links). To build from source:

```bash
sudo apt install build-essential pkg-config libgtk-3-dev libayatana-appindicator3-dev libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libxdo-dev
cargo build --release
```

Linux notes:

- **Tray.** The icon is a StatusNotifier / AppIndicator icon. KDE, XFCE and most panels show it; **GNOME needs the "AppIndicator and KStatusNotifierItem Support" extension**.
  The AppIndicator protocol has no click event, so on Linux the icon only opens its menu (save the link, open the library, recent links, quit).
  If no tray host is running, closing the window quits the program instead of hiding it.
- **Hotkey.** Ctrl+Alt+S works on X11 (the `global-hotkey` crate). Wayland gives programs no global hotkeys: the Settings switch then reports the shortcut as unavailable.
  Instead, bind a system shortcut to the command `save-urls --save-clipboard`
  (GNOME: Settings → Keyboard → Custom Shortcuts; KDE: System Settings → Shortcuts → Custom Shortcuts).
  The command saves the link from the clipboard: through the running program, or on its own when nothing is running.
- **Clipboard.** On X11 and on Wayland compositors with the data-control protocol (KDE, Sway, Hyprland) the program watches the clipboard; GNOME on Wayland
  does not allow that, so use the tray menu, the shortcut above or Ctrl+V inside the window.
- **Notifications** use the desktop notification service; clicking one does not open the link (there is no click callback).
- **Page titles** are downloaded through `curl` and honour its proxy settings.
- **Autostart** writes `~/.config/autostart/save-urls.desktop` with `--background`.

## How to save links

- **Click the tray icon** (Windows) to save the link from the clipboard. If there is no link in the clipboard, the window opens.
  In Settings the click can be switched to "Open library".
- **Ctrl+Alt+S** in any program does the same (can be turned off in Settings; Windows and X11, see the Linux notes).
- **Right-click the icon** for a menu: save (with the address from the clipboard), open the library, 8 recent links, quit.
- **In the window**: Ctrl+V, the "Add link" button, or the "In the clipboard: ..." bar above the list.

After saving from the tray, a system notification appears (on Windows); clicking it opens the link so you can add a description or pick a folder.
The same link is not saved twice (compared without `https://`, `www.` and a trailing `/`).

The link is found in any copied text: "look: https://...", `www.site.com`, or just `github.com/user/repo` or `t.me/channel`.

## What the window does

- **Folders** with your own names: "+" in the sidebar, double-click to rename, right-click to rename,
  reorder, make a folder the default or delete it (links are not deleted, they move to "No folder").
- **Drag and drop**: drag a link card onto a folder.
- **Link**: a title, an optional description and a folder. Clicking the card opens the link in the browser;
  on hover there are "open", "copy", "edit" and "delete" buttons, and the right-click menu offers the same plus "Move to...".
  Deleting can be undone with the "Undo" button.
- **Sorting**: newest or oldest first, by title A→Z and Z→A, by website.
- **Search** over the title, description and address at once, with the matches highlighted. It forgives typos ("githab"),
  skipped letters ("gthb"), treats "ё" as "е", and understands a query typed in the wrong layout ("пшерги" finds GitHub). Esc clears the search.
- **Title from the page**: after saving, the program downloads the page and takes `<title>` (or `og:title`).
  If you renamed the link yourself, your title is never overwritten. Can be turned off in Settings.
- **Bookmark export** to HTML, which Chrome, Firefox and Edge can import.
- **Autostart** with the system (Windows: a key in `HKCU\...\Run`; Linux: a `.desktop` file in `~/.config/autostart`), enabled in Settings.

Closing the window with the cross hides it in the tray. To quit completely, use "Quit" in the icon menu or the button in Settings.
Starting the program again does not create a second instance; it shows the window of the running program.

Window shortcuts: Ctrl+V saves the link from the clipboard, Ctrl+N adds a link, Ctrl+F searches.

## Where the library is stored

`%APPDATA%\Save URLs\library.json` on Windows, `~/.config/Save URLs/library.json` on Linux: links, folders and settings (including language and theme). Writes are atomic (through a temporary file),
and once per run the previous version is copied to `library.bak.json`. If the file turns out to be damaged, the program does not overwrite it
but sets it aside as `library.broken-<time>.json` and says so.

**Portable mode**: if `library.json` lies next to the executable, the program works with it (handy for a USB stick).
A portable copy has its own single-instance guard, so it can run next to an installed copy.

The program goes online only for the title of a saved page (on Windows through the system WinHTTP, honouring the Windows proxy; on Linux through `curl`).

## Layout

```
src/main.rs     start-up, single instance, window, icon (loaded from assets/icon.png)
src/app.rs      UI: sidebar with folders, list, search, link editor, settings, notices
src/strings.rs  interface strings (English, Russian, Ukrainian) and the current language
src/kit.rs      components on top of theme.rs: icons, navigation rows, menus, input fields, dropdowns, the monogram tile
src/theme.rs    shared: light/dark palette, theme mode, widgets
src/i18n.rs     shared: language, string tables, plurals, language/theme rows
src/nimvoro.rs  shared: update check and hand-off to the launcher
src/win.rs      Windows: tray icon in its own thread, clipboard, hotkey, notifications, autostart, WinHTTP
src/linux.rs    Linux: same interface as win.rs (tray-icon on a GTK thread, arboard, global-hotkey, notify-rust, curl, a unix socket for the single instance)
src/store.rs    data model and library.json
src/search.rs   search: match scoring, typos, layout, highlighting
src/urlx.rs     finding a link in text, the domain, parsing the title from HTML
build.rs        embeds assets/icon.ico and version info into the exe (windres for the GNU toolchain, rc.exe for MSVC)
```

`theme.rs`, `support.rs`, `i18n.rs`, `nimvoro.rs` and the fonts in `assets/fonts` (IBM Plex, SIL OFL) are shared, copied byte for byte into the other Rust tools of the suite. Their API is described in the file headers.

## Tests

```bash
cargo test
```

The title download test goes online, so it runs separately: `cargo test -- --ignored`.

For manual checks of a debug build there is the `SAVE_URLS_DEBUG` variable, for example
`SAVE_URLS_DEBUG="q:githab"`, `"page:settings"`, `"edit:10"`, `"folder:1"`, `"theme:light"`, `"lang:ru"`.

## Updates and NIMVORO

The tool is part of the [NIMVORO](https://github.com/GrigoryyKach/nimvoro) suite: the launcher installs it in one click and keeps it updated.
At start-up the program checks the latest [release](https://github.com/GrigoryyKach/save-urls/releases/latest) (the "Check on startup" switch in Settings, "System" section).
If a new version exists, the "Update" button hands the update to the launcher, and without the launcher it opens the release page.

The release is built by GitHub Actions when a tag is pushed: bump the version in `Cargo.toml`, then `git tag v{version} && git push --tags`.

## Support

If you find this app useful, you can support its development: <https://donatello.to/MicsGame>
