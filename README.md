# Save URLs

A link library for Windows that lives in the tray. Copy the address of an interesting site, click the tray icon, and the link is saved;
the app takes its title from the page itself. In the window you can sort links into folders, describe them and find them quickly.

Written in Rust (eframe/egui): a single exe with no dependencies, and the library is stored only on your computer.
The look matches Random Wheel and Smart Switcher: the shared `src/theme.rs` (copied from `../random_wheel` unchanged),
the brand gradient `#4f6bff → #8a5bff`, Segoe UI, cards and capitalised section headings.

## Languages

The interface is available in English (default), Russian and Ukrainian. Pick the language in Settings → Appearance; it applies immediately,
including the tray menu, tooltip and notifications.
The starting language is taken from the `language` setting in `library.json`, then from the `.nimvoro.json` marker next to the exe (written by the NIMVORO launcher),
and falls back to English. The result is saved to the library settings.

## Theme

Settings → Appearance → Theme: System (default, follows Windows live), Light or Dark.

## Run

```bash
cargo run --release
```

The ready exe is `target/release/SaveUrls.exe`; you can copy it anywhere.
The `--background` flag starts the program straight into the tray without a window (this is how it starts with Windows).

## How to save links

- **Click the tray icon** to save the link from the clipboard. If there is no link in the clipboard, the window opens.
  In Settings the click can be switched to "Open library".
- **Ctrl+Alt+S** in any program does the same (can be turned off in Settings).
- **Right-click the icon** for a menu: save (with the address from the clipboard), open the library, 8 recent links, quit.
- **In the window**: Ctrl+V, the "Add link" button, or the "In the clipboard: ..." bar above the list.

After saving from the tray, a Windows notification appears; clicking it opens the link so you can add a description or pick a folder.
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
- **Autostart** with Windows (a key in `HKCU\...\Run`, enabled in Settings).

Closing the window with the cross hides it in the tray. To quit completely, use "Quit" in the icon menu or the button in Settings.
Starting the exe again does not create a second instance; it shows the window of the running program.

Window shortcuts: Ctrl+V saves the link from the clipboard, Ctrl+N adds a link, Ctrl+F searches.

## Where the library is stored

`%APPDATA%\Save URLs\library.json`: links, folders and settings (including language and theme). Writes are atomic (through a temporary file),
and once per run the previous version is copied to `library.bak.json`. If the file turns out to be damaged, the program does not overwrite it
but sets it aside as `library.broken-<time>.json` and says so.

**Portable mode**: if `library.json` lies next to the exe, the program works with it (handy for a USB stick).
A portable copy has its own single-instance guard, so it can run next to an installed copy.

The program goes online only for the title of a saved page (through the system WinHTTP, honouring the Windows proxy).

## Layout

```
src/main.rs     start-up, single instance, window, icon (loaded from assets/icon.png)
src/app.rs      UI: sidebar with folders, list, search, link editor, settings, notices
src/strings.rs  interface strings (English, Russian, Ukrainian) and the current language
src/kit.rs      components on top of theme.rs: icons, navigation rows, menus, input fields, dropdowns
src/theme.rs    shared: light/dark palette, theme mode, widgets
src/i18n.rs     shared: language, string tables, plurals, language/theme rows
src/nimvoro.rs  shared: update check and hand-off to the launcher
src/win.rs      Windows: tray icon in its own thread, clipboard, hotkey, notifications, autostart, WinHTTP
src/store.rs    data model and library.json
src/search.rs   search: match scoring, typos, layout, highlighting
src/urlx.rs     finding a link in text, the domain, parsing the title from HTML
build.rs        embeds assets/icon.ico and version info into the exe (windres for the GNU toolchain, rc.exe for MSVC)
```

`theme.rs`, `i18n.rs` and `nimvoro.rs` are shared modules, copied byte for byte into the other Rust tools of the suite. Their API is described in the file headers.

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
