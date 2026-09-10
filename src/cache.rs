//! A tiny file cache under `~/.cache/sanae`. Deleting the directory is always safe.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn open() -> Self {
        let dir = directories::ProjectDirs::from("", "", "sanae")
            .map(|d| d.cache_dir().to_path_buf())
            .unwrap_or_else(|| std::env::temp_dir().join("sanae"));
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    /// A cache that never hits the disk (tests, `--no-cache`).
    pub fn disabled() -> Self {
        Self { dir: PathBuf::new() }
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    fn path(&self, key: &str) -> PathBuf {
        // Keys may contain anything; the file name must not.
        let safe: String = key
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
            .take(120)
            .collect();
        let hash = fxhash(key);
        self.dir.join(format!("{safe}-{hash:016x}"))
    }

    /// The cached text if it is younger than `ttl_secs`.
    pub fn get(&self, key: &str, ttl_secs: u64) -> Option<String> {
        if self.dir.as_os_str().is_empty() {
            return None;
        }
        let p = self.path(key);
        let meta = fs::metadata(&p).ok()?;
        let age = SystemTime::now().duration_since(meta.modified().ok()?).ok()?;
        if age > Duration::from_secs(ttl_secs) {
            return None;
        }
        fs::read_to_string(p).ok()
    }

    pub fn put(&self, key: &str, text: &str) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        let p = self.path(key);
        let tmp = p.with_extension("tmp");
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(tmp, p);
        }
    }

    /// Remove everything.
    pub fn clear(&self) -> std::io::Result<u64> {
        let mut n = 0;
        if self.dir.as_os_str().is_empty() {
            return Ok(0);
        }
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                fs::remove_file(entry.path())?;
                n += 1;
            }
        }
        Ok(n)
    }
}

/// Small, fast, non-cryptographic hash (FNV-1a); collisions only cost a cache miss.
fn fxhash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}
