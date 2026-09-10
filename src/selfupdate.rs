//! Is there a newer Sanae? Asked at start (once every few hours) and updated on request.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cache::Cache;
use crate::queue::Step;

const LATEST: &str = "https://api.github.com/repos/Chidaruma696/Sanae/releases/latest";
pub const BINARY: &str = "https://github.com/Chidaruma696/Sanae/releases/latest/download/sanae-x86_64-linux";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// The latest release tag, e.g. "v0.3.0". Cached for six hours.
pub async fn latest(http: &reqwest::Client, cache: &Cache) -> Result<String> {
    let key = "sanae-latest";
    let text = match cache.get(key, 6 * 3600) {
        Some(t) => t,
        None => {
            let t = http.get(LATEST).send().await.context("reaching GitHub")?.error_for_status()?.text().await?;
            cache.put(key, &t);
            t
        }
    };
    let r: Release = serde_json::from_str(&text).context("reading the release")?;
    Ok(r.tag_name)
}

/// Is `tag` newer than the running binary? Plain numeric comparison of a.b.c.
pub fn is_newer(tag: &str) -> bool {
    let parse =
        |s: &str| -> Vec<u64> { s.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(tag) > parse(env!("CARGO_PKG_VERSION"))
}

/// Replace the running binary with the latest release (needs root for /usr/local/bin).
pub fn update_steps(privilege: &str) -> Vec<Step> {
    let target =
        std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "/usr/local/bin/sanae".into());
    let script = format!(
        "set -e; tmp=$(mktemp); curl -fsSL --max-time 120 {BINARY} -o \"$tmp\" && chmod 755 \"$tmp\" && mv -f \"$tmp\" '{target}' && '{target}' --version"
    );
    vec![Step {
        title: format!("Updating Sanae at {target}"),
        program: privilege.to_string(),
        args: vec!["bash".into(), "-c".into(), script],
    }]
}

#[cfg(test)]
mod tests {
    #[test]
    fn newer_compares_versions_not_strings() {
        assert!(super::is_newer("v99.0.0"));
        assert!(!super::is_newer("v0.0.1"));
        assert!(!super::is_newer(concat!("v", env!("CARGO_PKG_VERSION"))));
    }
}
