//! The Arch User Repository, through its RPC v5 interface.
//! <https://aur.archlinux.org/rpc/v5/…>

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cache::Cache;
use crate::model::{Details, Package, Source};

const RPC: &str = "https://aur.archlinux.org/rpc/v5";
const USER_AGENT: &str = concat!("sanae/", env!("CARGO_PKG_VERSION"));
/// The RPC rejects overly long URLs; 100 names per `info` call is safe.
const INFO_CHUNK: usize = 100;

/// What `search` matches against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchBy {
    Name,
    #[default]
    NameDesc,
    Maintainer,
    Depends,
    MakeDepends,
    OptDepends,
    Provides,
    Keywords,
    Groups,
}

impl SearchBy {
    fn as_str(self) -> &'static str {
        match self {
            SearchBy::Name => "name",
            SearchBy::NameDesc => "name-desc",
            SearchBy::Maintainer => "maintainer",
            SearchBy::Depends => "depends",
            SearchBy::MakeDepends => "makedepends",
            SearchBy::OptDepends => "optdepends",
            SearchBy::Provides => "provides",
            SearchBy::Keywords => "keywords",
            SearchBy::Groups => "groups",
        }
    }
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    results: Vec<RpcPackage>,
    #[serde(default)]
    error: Option<String>,
}

/// The RPC's package record. Field names are the AUR's.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RpcPackage {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub package_base: Option<String>,
    #[serde(default)]
    pub num_votes: u32,
    #[serde(default)]
    pub popularity: f64,
    #[serde(default)]
    pub out_of_date: Option<i64>,
    #[serde(default)]
    pub maintainer: Option<String>,
    #[serde(default)]
    pub last_modified: Option<i64>,
    #[serde(default, rename = "URL")]
    pub url: Option<String>,
    #[serde(default)]
    pub license: Vec<String>,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub make_depends: Vec<String>,
    #[serde(default)]
    pub opt_depends: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub replaces: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
}

impl RpcPackage {
    pub fn to_package(&self) -> Package {
        Package {
            name: self.name.clone(),
            version: self.version.clone(),
            source: Source::Aur,
            description: self.description.clone().unwrap_or_default(),
            groups: self.groups.clone(),
            licenses: self.license.clone(),
            url: self.url.clone(),
            install_size: None,
            download_size: None,
            votes: Some(self.num_votes),
            popularity: Some(self.popularity),
            out_of_date: self.out_of_date,
            maintainer: self.maintainer.clone(),
            last_modified: self.last_modified,
            installed: None,
        }
    }

    pub fn to_details(&self) -> Details {
        Details {
            depends: self.depends.clone(),
            opt_depends: self.opt_depends.clone(),
            make_depends: self.make_depends.clone(),
            provides: self.provides.clone(),
            conflicts: self.conflicts.clone(),
            replaces: self.replaces.clone(),
            required_by: Vec::new(),
            optional_for: Vec::new(),
            packager: None,
            build_date: None,
            architecture: None,
            package_base: self.package_base.clone(),
            keywords: self.keywords.clone(),
        }
    }
}

pub struct Aur {
    http: reqwest::Client,
    cache: Cache,
}

impl Aur {
    pub fn new(cache: Cache) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .context("building the HTTP client")?;
        Ok(Self { http, cache })
    }

    async fn get(&self, url: &str, cache_key: &str, ttl_secs: u64) -> Result<RpcResponse> {
        if let Some(text) = self.cache.get(cache_key, ttl_secs) {
            if let Ok(r) = serde_json::from_str::<RpcResponse>(&text) {
                return Ok(r);
            }
        }
        let text = self
            .http
            .get(url)
            .send()
            .await
            .context("reaching the AUR")?
            .error_for_status()
            .context("the AUR answered with an error")?
            .text()
            .await?;
        let r: RpcResponse = serde_json::from_str(&text).context("reading the AUR's answer")?;
        if let Some(e) = &r.error {
            bail!("AUR: {e}");
        }
        self.cache.put(cache_key, &text);
        Ok(r)
    }

    /// Search the AUR. Results are cached for ten minutes.
    pub async fn search(&self, query: &str, by: SearchBy) -> Result<Vec<RpcPackage>> {
        let q = query.trim();
        if q.len() < 2 {
            return Ok(Vec::new());
        }
        let url = format!("{RPC}/search/{}?by={}", urlencode(q), by.as_str());
        let key = format!("search-{}-{}", by.as_str(), q);
        Ok(self.get(&url, &key, 600).await?.results)
    }

    /// Full records for the given names, in batches. Cached for ten minutes.
    pub async fn info(&self, names: &[String]) -> Result<Vec<RpcPackage>> {
        let mut all = Vec::with_capacity(names.len());
        for chunk in names.chunks(INFO_CHUNK) {
            let args: Vec<String> = chunk.iter().map(|n| format!("arg[]={}", urlencode(n))).collect();
            let url = format!("{RPC}/info?{}", args.join("&"));
            let key = format!("info-{}", chunk.join(","));
            all.extend(self.get(&url, &key, 600).await?.results);
        }
        Ok(all)
    }

    /// The PKGBUILD of a package base, as text.
    pub async fn pkgbuild(&self, package_base: &str) -> Result<String> {
        let url = format!("https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h={}", urlencode(package_base));
        let key = format!("pkgbuild-{package_base}");
        if let Some(t) = self.cache.get(&key, 3600) {
            return Ok(t);
        }
        let text = self.http.get(&url).send().await?.error_for_status()?.text().await?;
        self.cache.put(&key, &text);
        Ok(text)
    }
}

/// Percent-encode the few characters that matter in a query component.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'+' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
