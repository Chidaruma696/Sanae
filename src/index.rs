//! One in-memory index of every package Sanae knows about, with fuzzy search.

use std::collections::HashMap;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::model::{Installed, Package, Source};
use crate::sources::pacman::LocalPackage;

#[derive(Default)]
pub struct Index {
    pub packages: Vec<Package>,
    by_name: HashMap<String, usize>,
}

impl Index {
    /// Merge the sync repositories with the local database.
    pub fn build(mut sync: Vec<Package>, local: Vec<LocalPackage>) -> Self {
        let mut by_name: HashMap<String, usize> = HashMap::with_capacity(sync.len() + local.len());
        for (i, p) in sync.iter().enumerate() {
            // Two repos may ship the same name (testing repos); keep the first, pacman does too.
            by_name.entry(p.name.clone()).or_insert(i);
        }
        for l in local {
            match by_name.get(&l.name) {
                Some(&i) => sync[i].installed = Some(l.installed),
                None => {
                    // Not in any repo: an AUR build or a local package. The AUR client
                    // upgrades these to Source::Aur when it recognises them.
                    let idx = sync.len();
                    sync.push(Package {
                        name: l.name.clone(),
                        version: l.installed.version.clone(),
                        source: Source::Local,
                        description: l.description,
                        groups: l.groups,
                        licenses: l.licenses,
                        url: l.url,
                        install_size: l.installed.install_size,
                        download_size: None,
                        votes: None,
                        popularity: None,
                        out_of_date: None,
                        maintainer: None,
                        last_modified: None,
                        installed: Some(l.installed),
                    });
                    by_name.insert(l.name, idx);
                }
            }
        }
        Self { packages: sync, by_name }
    }

    pub fn get(&self, name: &str) -> Option<&Package> {
        self.by_name.get(name).map(|&i| &self.packages[i])
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Package> {
        self.by_name.get(name).copied().map(move |i| &mut self.packages[i])
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// Add or refresh AUR packages (search results, info on foreign packages).
    pub fn merge_aur(&mut self, aur: Vec<Package>) {
        for mut a in aur {
            match self.by_name.get(&a.name) {
                Some(&i) => {
                    let p = &mut self.packages[i];
                    if matches!(p.source, Source::Local | Source::Aur) {
                        a.installed = p.installed.take();
                        *p = a;
                    }
                    // A repo package with the same name as an AUR one: the repo wins.
                }
                None => {
                    self.by_name.insert(a.name.clone(), self.packages.len());
                    self.packages.push(a);
                }
            }
        }
    }

    pub fn installed(&self) -> impl Iterator<Item = &Package> {
        self.packages.iter().filter(|p| p.installed.is_some())
    }

    /// Fuzzy search over name and description. Name matches rank first; exact and prefix matches first of all.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Package> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(q, CaseMatching::Ignore, Normalization::Smart);
        let ql = q.to_lowercase();
        let mut buf = Vec::new();
        let mut scored: Vec<(u32, &Package)> = Vec::new();
        for p in &self.packages {
            let name_l = p.name.to_lowercase();
            let mut score = pattern.score(Utf32Str::new(&p.name, &mut buf), &mut matcher).map(|s| s * 4).unwrap_or(0);
            if score == 0 {
                score = pattern.score(Utf32Str::new(&p.description, &mut buf), &mut matcher).unwrap_or(0);
                if score == 0 {
                    continue;
                }
            }
            if name_l == ql {
                score += 100_000;
            } else if name_l.starts_with(&ql) {
                score += 20_000;
            } else if name_l.contains(&ql) {
                score += 5_000;
            }
            if p.installed.is_some() {
                score += 500;
            }
            scored.push((score, p));
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        scored.into_iter().take(limit).map(|(_, p)| p).collect()
    }

    /// Installed with `explicit` reason, or as dependencies.
    pub fn installed_by_reason(&self, explicit: bool) -> Vec<&Package> {
        self.installed().filter(|p| p.installed.as_ref().is_some_and(|i: &Installed| i.explicit == explicit)).collect()
    }
}
