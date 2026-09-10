//! The pacman databases, read through `expac` and `pacman` themselves.
//!
//! Sanae never links libalpm: pacman's own tools are stable across pacman
//! releases and this keeps Sanae a single static binary.

use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::model::{Details, Installed, Package, Source, Update};

/// Field separator inside one record and list separator inside one field.
const FS: char = '\t';
const LS: &str = "\x1f";

fn run(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("could not run {program} (is it installed?)"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        // pacman -Ql on a missing package and friends: return what we have.
        if out.stdout.is_empty() {
            bail!("{program} failed: {}", err.trim());
        }
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn list(field: &str) -> Vec<String> {
    field.split(LS).map(str::trim).filter(|s| !s.is_empty() && *s != "None").map(String::from).collect()
}

fn num<T: std::str::FromStr>(field: &str) -> Option<T> {
    field.trim().parse().ok()
}

fn opt(field: &str) -> Option<String> {
    let s = field.trim();
    if s.is_empty() || s == "None" { None } else { Some(s.to_string()) }
}

/// Every package in every enabled sync repository, in one pass.
pub fn sync_all() -> Result<Vec<Package>> {
    // %g is the PGP signature; groups are %G.
    let fmt = "%n\t%v\t%r\t%d\t%G\t%m\t%k\t%u\t%L";
    let out = run("expac", &["-S", "-l", LS, fmt])?;
    Ok(parse_sync(&out))
}

pub fn parse_sync(out: &str) -> Vec<Package> {
    let mut pkgs = Vec::with_capacity(16_000);
    for line in out.lines() {
        let f: Vec<&str> = line.split(FS).collect();
        if f.len() < 9 {
            continue;
        }
        pkgs.push(Package {
            name: f[0].to_string(),
            version: f[1].to_string(),
            source: Source::Repo(f[2].to_string()),
            description: f[3].to_string(),
            groups: list(f[4]),
            install_size: num(f[5]),
            download_size: num(f[6]),
            url: opt(f[7]),
            licenses: list(f[8]),
            votes: None,
            popularity: None,
            out_of_date: None,
            maintainer: None,
            last_modified: None,
            installed: None,
        });
    }
    pkgs
}

/// One installed package as the local database sees it.
#[derive(Clone, Debug)]
pub struct LocalPackage {
    pub name: String,
    pub description: String,
    pub url: Option<String>,
    pub licenses: Vec<String>,
    pub groups: Vec<String>,
    pub installed: Installed,
}

/// Every installed package.
pub fn local_all() -> Result<Vec<LocalPackage>> {
    let fmt = "%n\t%v\t%w\t%l\t%m\t%d\t%u\t%L\t%G";
    let out = run("expac", &["-Q", "-l", LS, "--timefmt", "%s", fmt])?;
    Ok(parse_local(&out))
}

pub fn parse_local(out: &str) -> Vec<LocalPackage> {
    let mut pkgs = Vec::with_capacity(2_000);
    for line in out.lines() {
        let f: Vec<&str> = line.split(FS).collect();
        if f.len() < 9 {
            continue;
        }
        pkgs.push(LocalPackage {
            name: f[0].to_string(),
            description: f[5].to_string(),
            url: opt(f[6]),
            licenses: list(f[7]),
            groups: list(f[8]),
            installed: Installed {
                version: f[1].to_string(),
                explicit: f[2].trim().starts_with("Explicitly"),
                install_date: num(f[3]),
                install_size: num(f[4]),
            },
        });
    }
    pkgs
}

/// Dependencies, provides, packager… for one package. `installed` picks the local database.
pub fn details(name: &str, installed: bool) -> Result<Details> {
    let fmt = "%D\t%O\t%J\t%P\t%H\t%T\t%N\t%W\t%p\t%b\t%a\t%e";
    let db = if installed { "-Q" } else { "-S" };
    let out = run("expac", &[db, "-l", LS, "--timefmt", "%s", fmt, name])?;
    let line = out.lines().next().unwrap_or_default();
    let f: Vec<&str> = line.split(FS).collect();
    if f.len() < 12 {
        bail!("no details for {name}");
    }
    Ok(Details {
        depends: list(f[0]),
        opt_depends: list(f[1]),
        make_depends: list(f[2]),
        provides: list(f[3]),
        conflicts: list(f[4]),
        replaces: list(f[5]),
        required_by: list(f[6]),
        optional_for: list(f[7]),
        packager: opt(f[8]),
        build_date: num(f[9]),
        architecture: opt(f[10]),
        package_base: opt(f[11]),
        keywords: Vec::new(),
    })
}

/// Files of a package: from the local database when installed, from the file database otherwise.
pub fn files(name: &str, installed: bool) -> Result<Vec<String>> {
    let out = if installed { run("pacman", &["-Qlq", name])? } else { run("pacman", &["-Flq", name])? };
    Ok(out.lines().map(String::from).collect())
}

/// Packages installed as dependencies that nothing needs any more.
pub fn orphans() -> Result<Vec<String>> {
    match run("pacman", &["-Qdtq"]) {
        Ok(out) => Ok(out.lines().map(String::from).collect()),
        // pacman exits 1 when there are no orphans.
        Err(_) => Ok(Vec::new()),
    }
}

/// Installed packages that are in no sync repository (AUR builds, local packages).
pub fn foreign() -> Result<Vec<String>> {
    match run("pacman", &["-Qmq"]) {
        Ok(out) => Ok(out.lines().map(String::from).collect()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Repository updates, via `checkupdates` (pacman-contrib), which never touches the real databases.
pub fn updates() -> Result<Vec<Update>> {
    let out = match run("checkupdates", &[]) {
        Ok(out) => out,
        // checkupdates exits 2 when there is nothing to do.
        Err(e) if e.to_string().contains("failed:") && e.to_string().trim_end().ends_with("failed:") => String::new(),
        Err(e) => return Err(e),
    };
    Ok(parse_updates(&out))
}

pub fn parse_updates(out: &str) -> Vec<Update> {
    let mut ups = Vec::new();
    for line in out.lines() {
        // "name 1.0-1 -> 1.1-1"
        let mut it = line.split_whitespace();
        let (Some(name), Some(cur), Some(_arrow), Some(new)) = (it.next(), it.next(), it.next(), it.next()) else {
            continue;
        };
        ups.push(Update {
            name: name.to_string(),
            current: cur.to_string(),
            new: new.to_string(),
            source: Source::Repo(String::new()),
        });
    }
    ups
}

/// Which package owns a file or command (`pacman -F`).
pub fn owner_of(path_or_name: &str) -> Result<Vec<String>> {
    let out = run("pacman", &["-Fq", path_or_name])?;
    Ok(out.lines().map(String::from).collect())
}

/// Compare two pacman versions with pacman's own rules (`vercmp`).
pub fn vercmp(a: &str, b: &str) -> std::cmp::Ordering {
    match run("vercmp", &[a, b]) {
        Ok(out) => match out.trim() {
            "-1" => std::cmp::Ordering::Less,
            "1" => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        },
        Err(_) => a.cmp(b),
    }
}
