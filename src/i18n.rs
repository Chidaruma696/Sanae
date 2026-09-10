//! Interface language. Strings are written in English in the code; `t()` looks
//! them up in the table of the active language and falls back to English.
//!
//! Tables live in `i18n/<code>.txt`: one `English<TAB>Translation` per line.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

static LANG: RwLock<&'static str> = RwLock::new("en");
static ES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static DE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static FR: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static IT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static PT: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static JA: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static RU: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

const ES_TABLE: &str = include_str!("../i18n/es.txt");
const DE_TABLE: &str = include_str!("../i18n/de.txt");
const FR_TABLE: &str = include_str!("../i18n/fr.txt");
const IT_TABLE: &str = include_str!("../i18n/it.txt");
const PT_TABLE: &str = include_str!("../i18n/pt.txt");
const JA_TABLE: &str = include_str!("../i18n/ja.txt");
const RU_TABLE: &str = include_str!("../i18n/ru.txt");

/// Languages Sanae ships with: code and native name.
pub const LANGS: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Español"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("pt", "Português"),
    ("ja", "日本語"),
    ("ru", "Русский"),
];

fn table(code: &str) -> Option<&'static HashMap<&'static str, &'static str>> {
    match code {
        "es" => Some(ES.get_or_init(|| parse(ES_TABLE))),
        "de" => Some(DE.get_or_init(|| parse(DE_TABLE))),
        "fr" => Some(FR.get_or_init(|| parse(FR_TABLE))),
        "it" => Some(IT.get_or_init(|| parse(IT_TABLE))),
        "pt" => Some(PT.get_or_init(|| parse(PT_TABLE))),
        "ja" => Some(JA.get_or_init(|| parse(JA_TABLE))),
        "ru" => Some(RU.get_or_init(|| parse(RU_TABLE))),
        _ => None,
    }
}

fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(k, v)| (k, v.trim_end_matches('\r')))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

/// Activate a language: a code from `LANGS`, or "auto" (from LANG / LC_ALL).
pub fn set(code: &str) {
    let code = match code {
        "auto" | "" => guess(),
        c => c,
    };
    let code: &'static str = LANGS.iter().find(|(c, _)| *c == code).map(|(c, _)| *c).unwrap_or("en");
    *LANG.write().expect("lang lock") = code;
}

pub fn current() -> &'static str {
    *LANG.read().expect("lang lock")
}

/// The language the environment asks for.
pub fn guess() -> &'static str {
    let env = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    LANGS.iter().skip(1).find(|(c, _)| env.starts_with(c)).map(|(c, _)| *c).unwrap_or("en")
}

/// Translate a string literal. Unknown strings come back unchanged.
pub fn t(s: &'static str) -> &'static str {
    match table(current()) {
        Some(map) => map.get(s).copied().unwrap_or(s),
        None => s,
    }
}

/// Translate a template with `{}` slots and fill them in order.
pub fn tf(template: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::from(t(template));
    for a in args {
        out = out.replacen("{}", &a.to_string(), 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_parse_and_fall_back() {
        set("ja");
        assert_eq!(t("Store"), "ストア");
        set("ru");
        assert_eq!(t("Settings"), "Настройки");
        set("es");
        assert_eq!(t("Store"), "Tienda");
        assert_eq!(t("this string does not exist"), "this string does not exist");
        assert_eq!(tf(" {} results ", &[&3]), " 3 resultados ");
        set("en");
        assert_eq!(t("Store"), "Store");
    }

    #[test]
    fn every_table_line_has_a_tab() {
        for (code, _) in LANGS.iter().skip(1) {
            let text = match *code {
                "es" => ES_TABLE,
                "de" => DE_TABLE,
                "fr" => FR_TABLE,
                "it" => IT_TABLE,
                "pt" => PT_TABLE,
                "ja" => JA_TABLE,
                "ru" => RU_TABLE,
                _ => unreachable!(),
            };
            for (n, l) in text.lines().enumerate() {
                if l.is_empty() || l.starts_with('#') {
                    continue;
                }
                assert!(l.contains('\t'), "i18n/{code}.txt line {}: no tab: {l}", n + 1);
            }
        }
    }
}
