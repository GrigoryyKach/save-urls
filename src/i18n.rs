//! NIMVORO shared module: interface language (English / Russian / Ukrainian).
//!
//! Copied byte for byte into every Rust (egui) tool of the suite. It knows nothing about any
//! particular app: the app keeps its own strings in its own file.
//!
//! # Usage
//!
//! ```ignore
//! // 1. App strings: one struct, three constants, one `Table`.
//! pub struct S { pub title: &'static str, pub items: [&'static str; 3] }
//! const EN: S = S { title: "Hello", items: ["{n} item", "{n} items", "{n} items"] };
//! const RU: S = S { /* ... */ };
//! const UK: S = S { /* ... */ };
//! static TABLE: Table<S> = Table { en: &EN, ru: &RU, uk: &UK };
//! let s = TABLE.get(lang);               // `lang: Lang`
//!
//! // 2. Starting language: config -> `.nimvoro.json` next to the exe -> English.
//! let lang = i18n::resolve(cfg.language.as_deref());
//! cfg.language = Some(lang.code().to_string()); // persist the result
//!
//! // 3. Plurals: `i18n::plural(lang, n, &s.items)` -> "5 items" (the `{n}` placeholder is replaced).
//!
//! // 4. Settings page widgets (labels come from this file, so they are translated everywhere):
//! if i18n::language_row(ui, &mut lang, &pal) { /* save, apply live */ }
//! if i18n::theme_row(ui, &mut mode, lang, &pal) { theme::apply(ctx, mode); /* save */ }
//! ```
//!
//! `{n}` is the only placeholder understood by [`plural`]; other formatting is up to the app.

use crate::theme::{self, Pal, ThemeMode};
use eframe::egui::{self, Align, Layout, RichText, Ui};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    En,
    Ru,
    Uk,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Ru, Lang::Uk];

    /// Code stored in configs and in the `.nimvoro.json` marker.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
            Lang::Uk => "uk",
        }
    }

    /// Name of the language in that language.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
            Lang::Uk => "Українська",
        }
    }

    /// Parses `en`, `ru`, `uk` (case-insensitive; also `en-US`, `ru_RU`, `ua`).
    pub fn from_code(code: &str) -> Option<Lang> {
        let c = code.trim().to_ascii_lowercase();
        let c = c.split(['-', '_']).next().unwrap_or_default();
        match c {
            "en" => Some(Lang::En),
            "ru" => Some(Lang::Ru),
            "uk" | "ua" => Some(Lang::Uk),
            _ => None,
        }
    }

    /// Picks one of three inline strings; handy for one-off labels.
    pub fn pick<'a>(self, en: &'a str, ru: &'a str, uk: &'a str) -> &'a str {
        match self {
            Lang::En => en,
            Lang::Ru => ru,
            Lang::Uk => uk,
        }
    }
}

/// Three static string tables, one per language. `T` is the app's own struct of `&'static str`.
pub struct Table<T: 'static> {
    pub en: &'static T,
    pub ru: &'static T,
    pub uk: &'static T,
}

impl<T> Table<T> {
    pub fn get(&self, lang: Lang) -> &'static T {
        match lang {
            Lang::En => self.en,
            Lang::Ru => self.ru,
            Lang::Uk => self.uk,
        }
    }
}

// ───────────────────────── starting language ─────────────────────────

/// `language` from the marker `<exe dir>\.nimvoro.json` written by the NIMVORO launcher.
pub fn marker_language() -> Option<Lang> {
    let exe = std::env::current_exe().ok()?;
    let text = std::fs::read_to_string(exe.parent()?.join(".nimvoro.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    Lang::from_code(v.get("language")?.as_str()?)
}

/// Contract rule: `language` from the app config wins, then the launcher marker, then English.
pub fn resolve(config_language: Option<&str>) -> Lang {
    config_language.and_then(Lang::from_code).or_else(marker_language).unwrap_or(Lang::En)
}

// ───────────────────────── plurals ─────────────────────────

/// Index of the plural form: 0 = one, 1 = few, 2 = many/other.
/// English has only "one" and "other" (forms 1 and 2 are the same word, pass it twice).
pub fn plural_index(lang: Lang, n: usize) -> usize {
    match lang {
        Lang::En => {
            if n == 1 {
                0
            } else {
                2
            }
        }
        Lang::Ru | Lang::Uk => {
            let (m10, m100) = (n % 10, n % 100);
            if m10 == 1 && m100 != 11 {
                0
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                1
            } else {
                2
            }
        }
    }
}

/// `forms` = [one, few, many]; every form may contain `{n}`, which is replaced by the number.
pub fn plural(lang: Lang, n: usize, forms: &[&str; 3]) -> String {
    forms[plural_index(lang, n)].replace("{n}", &n.to_string())
}

// ───────────────────────── shared labels ─────────────────────────

pub fn language_label(lang: Lang) -> &'static str {
    lang.pick("Language", "Язык", "Мова")
}

pub fn theme_label(lang: Lang) -> &'static str {
    lang.pick("Theme", "Тема", "Тема")
}

pub fn theme_name(mode: ThemeMode, lang: Lang) -> &'static str {
    match mode {
        ThemeMode::System => lang.pick("System", "Системная", "Системна"),
        ThemeMode::Light => lang.pick("Light", "Светлая", "Світла"),
        ThemeMode::Dark => lang.pick("Dark", "Тёмная", "Темна"),
    }
}

// ───────────────────────── settings widgets ─────────────────────────

fn combo_row(ui: &mut Ui, label: &str, p: &Pal, add: impl FnOnce(&mut Ui) -> bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).font(theme::reg(14.0)).color(p.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = add(ui);
        });
    });
    changed
}

/// Row "Language [English v]". Returns `true` when the user picked another language.
pub fn language_row(ui: &mut Ui, lang: &mut Lang, p: &Pal) -> bool {
    let current = *lang;
    combo_row(ui, language_label(current), p, |ui| {
        let mut changed = false;
        egui::ComboBox::from_id_salt("nimvoro-language")
            .width(150.0)
            .selected_text(RichText::new(current.native_name()).font(theme::reg(13.5)))
            .show_ui(ui, |ui| {
                for l in Lang::ALL {
                    if ui.selectable_label(l == *lang, RichText::new(l.native_name()).font(theme::reg(13.5))).clicked() && l != *lang {
                        *lang = l;
                        changed = true;
                    }
                }
            });
        changed
    })
}

/// Row "Theme [System v]". Returns `true` when the user picked another mode.
pub fn theme_row(ui: &mut Ui, mode: &mut ThemeMode, lang: Lang, p: &Pal) -> bool {
    let current = *mode;
    combo_row(ui, theme_label(lang), p, |ui| {
        let mut changed = false;
        egui::ComboBox::from_id_salt("nimvoro-theme")
            .width(150.0)
            .selected_text(RichText::new(theme_name(current, lang)).font(theme::reg(13.5)))
            .show_ui(ui, |ui| {
                for m in ThemeMode::ALL {
                    if ui.selectable_label(m == *mode, RichText::new(theme_name(m, lang)).font(theme::reg(13.5))).clicked() && m != *mode {
                        *mode = m;
                        changed = true;
                    }
                }
            });
        changed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codes() {
        assert_eq!(Lang::from_code("RU"), Some(Lang::Ru));
        assert_eq!(Lang::from_code("uk-UA"), Some(Lang::Uk));
        assert_eq!(Lang::from_code("de"), None);
    }

    #[test]
    fn resolves_config_first() {
        assert_eq!(resolve(Some("uk")), Lang::Uk);
        assert_eq!(resolve(Some("xx")).code().len(), 2); // falls through to marker or English
    }

    #[test]
    fn plural_forms() {
        let f = ["{n} участник", "{n} участника", "{n} участников"];
        assert_eq!(plural(Lang::Ru, 1, &f), "1 участник");
        assert_eq!(plural(Lang::Ru, 3, &f), "3 участника");
        assert_eq!(plural(Lang::Ru, 11, &f), "11 участников");
        assert_eq!(plural(Lang::Uk, 22, &f), "22 участника");
        let e = ["{n} entry", "{n} entries", "{n} entries"];
        assert_eq!(plural(Lang::En, 1, &e), "1 entry");
        assert_eq!(plural(Lang::En, 0, &e), "0 entries");
    }
}
