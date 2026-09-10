//! Is there a newer Sanae? Asked at start and updated on request.
//!
//! The download goes through wget when it is installed, as a visible step in
//! the run window (progress and errors on screen; it retries on its own, and
//! curl dies with exit 56 halfway through on flaky links such as VirtualBox
//! NAT). Without wget, Sanae's own HTTP client downloads with retries. Never
//! through curl. The privilege tool only runs `install`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cache::Cache;
use crate::queue::Step;

const LATEST: &str = "https://api.github.com/repos/Chidaruma696/Sanae/releases/latest";
pub const BINARY: &str = "https://github.com/Chidaruma696/Sanae/releases/latest/download/sanae-x86_64-linux";
/// The same binary on the `bin` branch, as a github.com archive (codeload):
/// reachable on networks that reset githubusercontent.com.
pub const ARCHIVE: &str = "https://github.com/Chidaruma696/Sanae/archive/refs/heads/bin.tar.gz";
/// The `bin` branch through jsDelivr, the last resort (cached up to 12 h).
pub const MIRROR: &str = "https://cdn.jsdelivr.net/gh/Chidaruma696/Sanae@bin/sanae-x86_64-linux";
const VERSION_MIRROR: &str = "https://cdn.jsdelivr.net/gh/Chidaruma696/Sanae@bin/VERSION";
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
            let t = match http.get(LATEST).send().await.and_then(|r| r.error_for_status()) {
                Ok(r) => r.text().await.context("reading GitHub's answer")?,
                Err(e) => match wget_text(LATEST).await {
                    Ok(t) => t,
                    Err(_) => {
                        // The API is unreachable: the VERSION file on the bin branch, via jsDelivr.
                        let v = match http.get(VERSION_MIRROR).send().await.and_then(|r| r.error_for_status()) {
                            Ok(r) => r.text().await.unwrap_or_default(),
                            Err(_) => {
                                wget_text(VERSION_MIRROR).await.with_context(|| format!("reaching GitHub ({e})"))?
                            }
                        };
                        format!("{{\"tag_name\":\"{}\"}}", v.trim())
                    }
                },
            };
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

/// Is wget on the PATH? Then the update runs as visible steps (`wget_steps`).
pub fn has_wget() -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join("wget").is_file())).unwrap_or(false)
}

/// Fetch a small text URL with wget (the release check falls back to this).
async fn wget_text(url: &str) -> Result<String> {
    let out = tokio::process::Command::new("wget")
        .args(["-qO-", "--tries=2", "--timeout=20", url])
        .output()
        .await
        .context("running wget")?;
    if !out.status.success() {
        bail!("wget exited with {}", out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The whole update as steps for the run window, when wget is installed:
/// download from the first source that works (a fresh file every time, never
/// resumed on top of an old one), verify, install, show the version, clean up.
pub fn wget_steps(privilege: &str, dir: &Path, target: &str) -> Vec<Step> {
    std::fs::create_dir_all(dir).ok();
    let path = dir.join("sanae-update");
    let script = r#"set -o pipefail; P=__P__; rm -f "$P" "$P.tgz"
get() { wget --tries=3 --waitretry=5 --timeout=60 --progress=dot:mega -O "$2" "$1"; }
ok() { test "$(head -c 4 "$P")" = $'\x7fELF' && test "$(stat -c %s "$P")" -gt __MIN__; }
{ echo ":: GitHub release"; get __BINARY__ "$P" && ok; } \
|| { echo ":: GitHub archive of the bin branch"; rm -f "$P"; get __ARCHIVE__ "$P.tgz" && tar xzOf "$P.tgz" --wildcards '*/sanae-x86_64-linux' > "$P" && ok; } \
|| { echo ":: jsDelivr mirror"; rm -f "$P"; get __MIRROR__ "$P" && ok; }
rc=$?; rm -f "$P.tgz"; exit $rc"#
        .replace("__P__", &sq(&path.display().to_string()))
        .replace("__MIN__", &MIN_SIZE.to_string())
        .replace("__BINARY__", BINARY)
        .replace("__ARCHIVE__", ARCHIVE)
        .replace("__MIRROR__", MIRROR);
    let mut steps = vec![Step {
        title: "Downloading the latest Sanae".into(),
        program: "bash".into(),
        args: vec!["-c".into(), script],
    }];
    steps.extend(install_steps(privilege, &path, target));
    steps
}

/// Download the latest release binary into `dir` with the built-in client
/// (no wget) and check that it looks like a real ELF binary.
pub async fn download(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).ok();
    let path = dir.join("sanae-update");
    let _ = std::fs::remove_file(&path);
    download_builtin(&path).await?;
    Ok(path)
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
    for (i, url) in [BINARY, MIRROR, BINARY, MIRROR].into_iter().enumerate() {
        let attempt = i as u32 + 1;
        match fetch(&http, url).await {
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

async fn fetch(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = http.get(url).send().await.with_context(|| format!("reaching {url}"))?.error_for_status()?;
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
