//! Is there a newer Sanae? Asked at start and updated on request.
//!
//! The download goes through wget when it is installed (it resumes and retries
//! on its own; curl dies with exit 56 halfway through on flaky links such as
//! VirtualBox NAT), and through Sanae's own HTTP client with retries otherwise.
//! Never through curl. The privilege tool only runs `install`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cache::Cache;
use crate::queue::Step;

const LATEST: &str = "https://api.github.com/repos/Chidaruma696/Sanae/releases/latest";
pub const BINARY: &str = "https://github.com/Chidaruma696/Sanae/releases/latest/download/sanae-x86_64-linux";
/// A real Sanae binary is a few megabytes; anything smaller is an error page.
const MIN_SIZE: usize = 1024 * 1024;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// The latest release tag, e.g. "v0.3.0". Cached for ten minutes so that a
/// release published after the last look is still noticed the next time.
pub async fn latest(http: &reqwest::Client, cache: &Cache) -> Result<String> {
    let key = "sanae-latest";
    let text = match cache.get(key, 600) {
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

/// Where the running binary lives (what gets replaced).
pub fn target() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "/usr/local/bin/sanae".into())
}

/// Download the latest release binary into `dir` and check that it looks like
/// a real ELF binary. wget when available, the built-in client otherwise.
pub async fn download(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).ok();
    let path = dir.join("sanae-update");
    if which_wget() {
        let status = tokio::process::Command::new("wget")
            .args(["-c", "--tries=5", "--waitretry=5", "--timeout=60", "-q", "-O"])
            .arg(&path)
            .arg(BINARY)
            .status()
            .await
            .context("running wget")?;
        if status.success() {
            let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            check(&bytes)?;
            return Ok(path);
        }
        let _ = std::fs::remove_file(&path);
    }
    download_builtin(&path).await?;
    Ok(path)
}

fn which_wget() -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join("wget").is_file())).unwrap_or(false)
}

fn check(bytes: &[u8]) -> Result<()> {
    if bytes.len() < MIN_SIZE {
        bail!("the download is too small ({} bytes): not a Sanae binary", bytes.len());
    }
    if &bytes[..4] != b"\x7fELF" {
        bail!("the download is not an ELF binary");
    }
    Ok(())
}

async fn download_builtin(path: &Path) -> Result<()> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("sanae/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .context("building the HTTP client")?;
    let mut last = anyhow::anyhow!("no attempt made");
    for attempt in 1..=4u32 {
        match fetch(&http).await {
            Ok(bytes) => {
                std::fs::write(path, &bytes).with_context(|| format!("writing {}", path.display()))?;
                return Ok(());
            }
            Err(e) => {
                last = e;
                tokio::time::sleep(Duration::from_secs(2 * u64::from(attempt))).await;
            }
        }
    }
    Err(last.context("downloading the latest Sanae (4 attempts)"))
}

async fn fetch(http: &reqwest::Client) -> Result<Vec<u8>> {
    let resp = http.get(BINARY).send().await.context("reaching GitHub releases")?.error_for_status()?;
    let bytes = resp.bytes().await.context("receiving the binary")?;
    check(&bytes)?;
    Ok(bytes.to_vec())
}

/// Put the downloaded file in place (needs root for /usr/local/bin) and show
/// the new version.
pub fn install_steps(privilege: &str, downloaded: &Path, target: &str) -> Vec<Step> {
    let src = downloaded.display().to_string();
    vec![
        Step {
            title: format!("Installing Sanae at {target}"),
            program: privilege.to_string(),
            args: vec!["install".into(), "-m".into(), "755".into(), src.clone(), target.to_string()],
        },
        Step::new("New version", target, &["--version"]),
        Step::new("Cleaning up", "rm", &["-f", &src]),
    ]
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
