//! Interface language. Strings are written in English in the code; `t()` looks
//! them up in the table of the active language and falls back to English.
//!
//! Tables live in `i18n/<code>.txt`: one `English<TAB>Translation` per line.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

static LANG: RwLock<&'static str> = RwLock::new("en");
static ES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

const ES_TABLE: &str = include_str!("../i18n/es.txt");

/// Languages Sanae ships with: code and native name.
pub const LANGS: &[(&str, &str)] = &[("en", "English"), ("es", "Español")];

fn table(code: &str) -> Option<&'static HashMap<&'static str, &'static str>> {
    match code {
        "es" => Some(ES.get_or_init(|| parse(ES_TABLE))),
        _ => None,
    }
}

fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(k, v)| (k, v.trim_end_matches('')))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

/// Activate a language: "en", "es", or "auto" (from LANG / LC_ALL).
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
    if env.starts_with("es") { "es" } else { "en" }
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
    fn spanish_table_parses_and_falls_back() {
        set("es");
        assert_eq!(t("Store"), "Tienda");
        assert_eq!(t("this string does not exist"), "this string does not exist");
        assert_eq!(tf(" {} results ", &[&3]), " 3 resultados ");
        set("en");
        assert_eq!(t("Store"), "Store");
    }

    #[test]
    fn every_spanish_line_has_a_tab() {
        for (n, l) in ES_TABLE.lines().enumerate() {
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            assert!(l.contains('\t'), "i18n/es.txt line {}: no tab: {l}", n + 1);
        }
    }
}
