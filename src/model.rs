//! The one package type every source maps into.

use serde::{Deserialize, Serialize};

/// Where a package comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "name")]
pub enum Source {
    /// A pacman repository: core, extra, multilib, chaotic-aur, a custom one…
    Repo(String),
    /// The Arch User Repository.
    Aur,
    /// Installed but in no repository and not on the AUR (a local build, an old AUR name).
    Local,
}

impl Source {
    pub fn label(&self) -> &str {
        match self {
            Source::Repo(r) => r,
            Source::Aur => "aur",
            Source::Local => "local",
        }
    }

    pub fn is_aur(&self) -> bool {
        matches!(self, Source::Aur)
    }
}

/// What the local database knows about an installed package.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    /// Installed on purpose (`pacman -S pkg`) rather than pulled in as a dependency.
    pub explicit: bool,
    /// Unix timestamp.
    pub install_date: Option<i64>,
    pub install_size: Option<u64>,
}

/// A package as shown in lists. Details that need another round trip live in [`Details`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub source: Source,
    pub description: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub licenses: Vec<String>,
    pub url: Option<String>,
    /// Bytes once installed (repos and local database).
    pub install_size: Option<u64>,
    /// Bytes to download (repos only).
    pub download_size: Option<u64>,
    /// AUR: votes.
    pub votes: Option<u32>,
    /// AUR: popularity score. pkgstats: percentage of systems with it installed.
    pub popularity: Option<f64>,
    /// AUR: flagged out of date at this Unix timestamp.
    pub out_of_date: Option<i64>,
    pub maintainer: Option<String>,
    /// AUR: last modification, Unix timestamp.
    pub last_modified: Option<i64>,
    pub installed: Option<Installed>,
}

impl Package {
    pub fn is_installed(&self) -> bool {
        self.installed.is_some()
    }

    /// The installed version differs from the available one.
    pub fn has_update(&self) -> bool {
        match &self.installed {
            Some(i) => i.version != self.version,
            None => false,
        }
    }
}

/// Everything else about one package, fetched on demand.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Details {
    pub depends: Vec<String>,
    pub opt_depends: Vec<String>,
    pub make_depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub required_by: Vec<String>,
    pub optional_for: Vec<String>,
    pub packager: Option<String>,
    pub build_date: Option<i64>,
    pub architecture: Option<String>,
    pub package_base: Option<String>,
    pub keywords: Vec<String>,
}

/// One line of `checkupdates` or an AUR version comparison.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Update {
    pub name: String,
    pub current: String,
    pub new: String,
    pub source: Source,
}
