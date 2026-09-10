//! `~/.config/sanae/config.toml`. Every field has a default; the file is optional.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub theme: ThemeConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// paru, yay, or auto (first one found).
    pub aur_helper: String,
    /// sudo, doas, or auto.
    pub privilege: String,
    /// Ask before applying the queue.
    pub confirm: bool,
    /// Look for a newer Sanae at start.
    pub check_updates: bool,
    /// Interface language: a code from i18n::LANGS, or auto.
    pub language: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            aur_helper: "auto".into(),
            privilege: "auto".into(),
            confirm: true,
            check_updates: true,
            language: "auto".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Main accent, hex. Moriya green by default.
    pub accent: String,
    /// Second accent, hex. Lake blue.
    pub accent2: String,
    /// Use Nerd Font glyphs instead of plain Unicode marks.
    pub nerd_font: bool,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self { accent: "#5fd7a7".into(), accent2: "#87afff".into(), nerd_font: false }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "sanae").map(|d| d.config_dir().join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(p) = Self::path() else { return Self::default() };
        match std::fs::read_to_string(&p) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("warning: {} is not valid, using defaults: {e}", p.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(p) = Self::path() else { return Ok(()) };
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(p, text)
    }

    /// The AUR helper to use, if any is installed.
    pub fn aur_helper(&self) -> Option<String> {
        match self.general.aur_helper.as_str() {
            "auto" | "" => ["paru", "yay"].iter().find(|h| in_path(h)).map(|h| h.to_string()),
            other => Some(other.to_string()),
        }
    }

    /// sudo or doas.
    pub fn privilege(&self) -> String {
        match self.general.privilege.as_str() {
            "auto" | "" => {
                if in_path("sudo") {
                    "sudo".into()
                } else if in_path("doas") {
                    "doas".into()
                } else {
                    "sudo".into()
                }
            }
            other => other.to_string(),
        }
    }
}

/// Is `name` an executable somewhere on PATH?
pub fn in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| {
        let p: &Path = &dir.join(name);
        p.is_file()
    })
}
