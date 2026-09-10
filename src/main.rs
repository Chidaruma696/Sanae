//! Sanae · a software store for Arch Linux that lives in the terminal.

// Parts of the data layer are only reached by the interface, which lands with milestone 2.
#![allow(dead_code)]

mod cache;
mod index;
#[cfg(test)]
mod index_tests;
mod model;
mod sources;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::cache::Cache;
use crate::index::Index;
use crate::model::{Package, Source};
use crate::sources::aur::{Aur, SearchBy};
use crate::sources::pacman;

#[derive(Parser)]
#[command(name = "sanae", version, about = "A software store for Arch Linux that lives in the terminal")]
struct Cli {
    /// Machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    /// Do not read or write the cache.
    #[arg(long, global = true)]
    no_cache: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Search the repositories and the AUR.
    Search {
        query: String,
        /// Only the AUR.
        #[arg(long)]
        aur: bool,
        /// Only the repositories.
        #[arg(long)]
        repos: bool,
        /// Only installed packages.
        #[arg(long)]
        installed: bool,
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },
    /// Everything about one package.
    Info { name: String },
    /// Installed packages.
    Installed {
        /// Installed on purpose, not as dependencies.
        #[arg(long)]
        explicit: bool,
        /// Dependencies nothing needs any more.
        #[arg(long)]
        orphans: bool,
        /// From the AUR or built locally.
        #[arg(long)]
        foreign: bool,
    },
    /// Available updates from the repositories and the AUR.
    Updates,
    /// Which package owns a file or command.
    Owner { path: String },
    /// Remove Sanae's cache.
    Clean,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cache = if cli.no_cache { Cache::disabled() } else { Cache::open() };
    match cli.command {
        None => {
            // The TUI arrives with milestone 2.
            eprintln!(
                "The interface is not here yet. Try: sanae search <text>, sanae info <package>, sanae installed, sanae updates"
            );
            Ok(())
        }
        Some(Cmd::Search { query, aur, repos, installed, limit }) => {
            cmd_search(&query, aur, repos, installed, limit, cli.json, cache).await
        }
        Some(Cmd::Info { name }) => cmd_info(&name, cli.json, cache).await,
        Some(Cmd::Installed { explicit, orphans, foreign }) => cmd_installed(explicit, orphans, foreign, cli.json),
        Some(Cmd::Updates) => cmd_updates(cli.json, cache).await,
        Some(Cmd::Owner { path }) => {
            for p in pacman::owner_of(&path)? {
                println!("{p}");
            }
            Ok(())
        }
        Some(Cmd::Clean) => {
            let n = cache.clear()?;
            println!("removed {n} cached files from {}", cache.dir().display());
            Ok(())
        }
    }
}

fn load_index() -> Result<Index> {
    let sync = pacman::sync_all().context("reading the package databases")?;
    let local = pacman::local_all().context("reading the installed packages")?;
    Ok(Index::build(sync, local))
}

async fn cmd_search(
    query: &str,
    aur_only: bool,
    repos_only: bool,
    installed_only: bool,
    limit: usize,
    json: bool,
    cache: Cache,
) -> Result<()> {
    let mut index = load_index()?;
    if !repos_only {
        let aur = Aur::new(cache)?;
        match aur.search(query, SearchBy::NameDesc).await {
            Ok(found) => index.merge_aur(found.iter().map(|r| r.to_package()).collect()),
            Err(e) => eprintln!("warning: {e:#}"),
        }
    }
    let results: Vec<&Package> = index
        .search(query, limit * 4)
        .into_iter()
        .filter(|p| !aur_only || p.source.is_aur())
        .filter(|p| !repos_only || matches!(p.source, Source::Repo(_)))
        .filter(|p| !installed_only || p.is_installed())
        .take(limit)
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }
    if results.is_empty() {
        println!("nothing matches \"{query}\"");
        return Ok(());
    }
    for p in results {
        let mark = if p.has_update() {
            "↑"
        } else if p.is_installed() {
            "✔"
        } else {
            " "
        };
        let extra = match p.source {
            Source::Aur => format!("  ★{} ", p.votes.unwrap_or(0)),
            _ => String::new(),
        };
        println!(
            "{mark} {:<10} {:<32} {:<14}{extra} {}",
            p.source.label(),
            p.name,
            p.version,
            truncate(&p.description, 70)
        );
    }
    Ok(())
}

async fn cmd_info(name: &str, json: bool, cache: Cache) -> Result<()> {
    let mut index = load_index()?;
    let mut details = None;
    if let Some(p) = index.get(name) {
        let installed = p.is_installed();
        if matches!(p.source, Source::Repo(_)) || installed {
            details = pacman::details(name, installed).ok();
        }
    }
    if index.get(name).is_none_or(|p| !matches!(p.source, Source::Repo(_))) {
        let aur = Aur::new(cache)?;
        if let Ok(found) = aur.info(&[name.to_string()]).await
            && let Some(r) = found.first()
        {
            index.merge_aur(vec![r.to_package()]);
            let d = r.to_details();
            details = Some(match details {
                Some(mut local) => {
                    local.keywords = d.keywords;
                    local
                }
                None => d,
            });
        }
    }
    let Some(p) = index.get(name) else {
        anyhow::bail!("no package named {name} in the repositories or the AUR");
    };
    if json {
        println!("{}", serde_json::json!({ "package": p, "details": details }));
        return Ok(());
    }
    println!("{}  {}  [{}]", p.name, p.version, p.source.label());
    println!("  {}", p.description);
    if let Some(u) = &p.url {
        println!("  url          {u}");
    }
    if !p.licenses.is_empty() {
        println!("  licenses     {}", p.licenses.join(", "));
    }
    if !p.groups.is_empty() {
        println!("  groups       {}", p.groups.join(", "));
    }
    if let Some(s) = p.install_size {
        println!("  installed    {}", human(s));
    }
    if let Some(s) = p.download_size {
        println!("  download     {}", human(s));
    }
    if let Some(v) = p.votes {
        println!("  votes        {v}   popularity {:.2}", p.popularity.unwrap_or(0.0));
    }
    if let Some(m) = &p.maintainer {
        println!("  maintainer   {m}");
    }
    if let Some(t) = p.out_of_date {
        println!("  OUT OF DATE  since {}", date(t));
    }
    match &p.installed {
        Some(i) => println!(
            "  status       installed {} ({}){}",
            i.version,
            if i.explicit { "explicitly" } else { "as a dependency" },
            i.install_date.map(|t| format!(", on {}", date(t))).unwrap_or_default()
        ),
        None => println!("  status       not installed"),
    }
    if let Some(d) = details {
        section("depends on", &d.depends);
        section("optional", &d.opt_depends);
        section("build needs", &d.make_depends);
        section("provides", &d.provides);
        section("conflicts", &d.conflicts);
        section("required by", &d.required_by);
        section("keywords", &d.keywords);
        if let Some(pk) = d.packager {
            println!("  packager     {pk}");
        }
        if let Some(b) = d.build_date {
            println!("  built        {}", date(b));
        }
    }
    Ok(())
}

fn cmd_installed(explicit: bool, orphans: bool, foreign: bool, json: bool) -> Result<()> {
    let index = load_index()?;
    let names: Option<Vec<String>> = if orphans {
        Some(pacman::orphans()?)
    } else if foreign {
        Some(pacman::foreign()?)
    } else {
        None
    };
    let mut list: Vec<&Package> = match &names {
        Some(ns) => ns.iter().filter_map(|n| index.get(n)).collect(),
        None => index.installed().collect(),
    };
    if explicit {
        list.retain(|p| p.installed.as_ref().is_some_and(|i| i.explicit));
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    if json {
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }
    for p in &list {
        let i = p.installed.as_ref().expect("installed");
        println!(
            "{:<10} {:<32} {:<16} {:>9}  {}",
            p.source.label(),
            p.name,
            i.version,
            i.install_size.map(human).unwrap_or_default(),
            if i.explicit { "explicit" } else { "dependency" }
        );
    }
    eprintln!("{} packages", list.len());
    Ok(())
}

async fn cmd_updates(json: bool, cache: Cache) -> Result<()> {
    let mut ups = pacman::updates().context("checkupdates (install pacman-contrib)")?;
    // AUR: compare every foreign package with the RPC.
    let foreign = pacman::foreign()?;
    if !foreign.is_empty() {
        let index = load_index()?;
        let aur = Aur::new(cache)?;
        match aur.info(&foreign).await {
            Ok(found) => {
                for r in found {
                    if let Some(p) = index.get(&r.name)
                        && let Some(i) = &p.installed
                        && pacman::vercmp(&i.version, &r.version) == std::cmp::Ordering::Less
                    {
                        ups.push(model::Update {
                            name: r.name.clone(),
                            current: i.version.clone(),
                            new: r.version.clone(),
                            source: Source::Aur,
                        });
                    }
                }
            }
            Err(e) => eprintln!("warning: {e:#}"),
        }
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&ups)?);
        return Ok(());
    }
    if ups.is_empty() {
        println!("everything is up to date");
        return Ok(());
    }
    for u in &ups {
        println!("{:<6} {:<32} {} -> {}", if u.source.is_aur() { "aur" } else { "repo" }, u.name, u.current, u.new);
    }
    eprintln!("{} updates", ups.len());
    Ok(())
}

fn section(title: &str, items: &[String]) {
    if !items.is_empty() {
        println!("  {title:<12} {}", items.join(", "));
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n - 1).collect();
        format!("{cut}…")
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

fn date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| ts.to_string())
}
