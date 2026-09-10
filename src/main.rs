//! Sanae · a software store for Arch Linux that lives in the terminal.

mod cache;
mod config;
mod exec;
mod index;
#[cfg(test)]
mod index_tests;
mod model;
mod queue;
mod recipes;
mod sources;
mod ui;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::cache::Cache;
use crate::config::Config;
use crate::index::Index;
use crate::model::{Package, Source};
use crate::queue::{Action, Queue};
use crate::recipes::ApplyOptions;
use crate::sources::aur::{Aur, SearchBy};
use crate::sources::pacman;

#[derive(Parser)]
#[command(name = "sanae", version, about = "A software store for Arch Linux that lives in the terminal (experimental)")]
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
        /// AUR field to search: name, name-desc, maintainer, depends, makedepends, optdepends, provides, keywords, groups.
        #[arg(long, default_value = "name-desc")]
        by: String,
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
    /// Update everything: repositories, then the AUR.
    Update,
    /// Install packages (repositories or AUR, sorted out for you).
    Install { packages: Vec<String> },
    /// Remove packages and their unneeded dependencies.
    Remove { packages: Vec<String> },
    /// Which package owns a file or command.
    Owner { path: String },
    /// The recipes and whether they are applied.
    Recipes,
    /// Apply recipes: install their packages and leave things configured.
    Apply {
        recipes: Vec<String>,
        /// A mounted installation (for example /mnt from the Arch ISO).
        #[arg(long)]
        chroot: Option<PathBuf>,
        /// The user that gets groups, home files and AUR builds.
        #[arg(long)]
        user: Option<String>,
        /// Print the commands instead of running them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove Sanae's cache.
    Clean,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cache = if cli.no_cache { Cache::disabled() } else { Cache::open() };
    let cfg = Config::load();
    match cli.command {
        None => ui::run(cfg, cache).await,
        Some(Cmd::Search { query, aur, repos, installed, limit, by }) => {
            let by = SearchBy::parse(&by).ok_or_else(|| anyhow::anyhow!("unknown --by {by}"))?;
            cmd_search(&query, by, aur, repos, installed, limit, cli.json, cache).await
        }
        Some(Cmd::Info { name }) => cmd_info(&name, cli.json, cache).await,
        Some(Cmd::Installed { explicit, orphans, foreign }) => cmd_installed(explicit, orphans, foreign, cli.json),
        Some(Cmd::Updates) => cmd_updates(cli.json, cache).await,
        Some(Cmd::Update) => exec::run_inherit(&Queue::update_plan(&cfg)),
        Some(Cmd::Install { packages }) => cmd_install(&packages, &cfg, cache).await,
        Some(Cmd::Remove { packages }) => {
            let mut q = Queue::default();
            for p in &packages {
                q.toggle(p, Action::Remove, false);
            }
            exec::run_inherit(&q.plan(&cfg))
        }
        Some(Cmd::Owner { path }) => {
            for p in pacman::owner_of(&path)? {
                println!("{p}");
            }
            Ok(())
        }
        Some(Cmd::Recipes) => cmd_recipes(cli.json),
        Some(Cmd::Apply { recipes, chroot, user, dry_run }) => cmd_apply(&recipes, chroot, user, dry_run, &cfg),
        Some(Cmd::Clean) => {
            let n = cache.clear()?;
            println!("removed {n} cached files from {}", cache.dir().display());
            Ok(())
        }
    }
}

fn load_index() -> Result<Index> {
    let sync = pacman::sync_all().context("reading the package databases (is expac installed?)")?;
    let local = pacman::local_all().context("reading the installed packages")?;
    Ok(Index::build(sync, local))
}

#[allow(clippy::too_many_arguments)]
async fn cmd_search(
    query: &str,
    by: SearchBy,
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
        match aur.search(query, by).await {
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

async fn cmd_install(packages: &[String], cfg: &Config, cache: Cache) -> Result<()> {
    let index = load_index()?;
    let mut q = Queue::default();
    let mut unknown = Vec::new();
    for p in packages {
        match index.get(p) {
            Some(pkg) => q.toggle(p, Action::Install, pkg.source.is_aur()),
            None => unknown.push(p.clone()),
        }
    }
    if !unknown.is_empty() {
        let aur = Aur::new(cache)?;
        let found = aur.info(&unknown).await.unwrap_or_default();
        for p in &unknown {
            if found.iter().any(|r| r.name == *p) {
                q.toggle(p, Action::Install, true);
            } else {
                anyhow::bail!("no package named {p} in the repositories or the AUR");
            }
        }
    }
    let pf = q.preflight();
    if !pf.installs.is_empty() {
        eprintln!("Will install ({}, {} to download):", pf.installs.len(), human(pf.download_bytes));
        for i in &pf.installs {
            eprintln!("  {i}");
        }
    }
    for p in &pf.problems {
        eprintln!("warning: {p}");
    }
    exec::run_inherit(&q.plan(cfg))
}

fn cmd_recipes(json: bool) -> Result<()> {
    let all = recipes::load_all();
    let index = load_index().ok();
    let installed = |p: &str| index.as_ref().and_then(|i| i.get(p)).is_some_and(|p| p.is_installed());
    if json {
        let rows: Vec<serde_json::Value> = all.iter().map(|r| serde_json::json!({ "id": r.id, "name": r.name, "summary": r.summary, "category": r.category, "packages": r.packages, "aur": r.aur, "applied": r.is_applied(&installed) })).collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    for r in &all {
        let mark = if r.is_applied(&installed) { "✔" } else { "○" };
        println!("{mark} {:<22} {:<12} {}", r.id, r.category, r.summary);
    }
    Ok(())
}

fn cmd_apply(ids: &[String], chroot: Option<PathBuf>, user: Option<String>, dry_run: bool, cfg: &Config) -> Result<()> {
    if ids.is_empty() {
        anyhow::bail!("say which recipes: sanae apply fonts docker …  (sanae recipes lists them)");
    }
    let all = recipes::load_all();
    let user = user
        .or_else(|| std::env::var("SUDO_USER").ok())
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "root".into());
    let opts = ApplyOptions {
        chroot,
        user,
        privilege: cfg.privilege(),
        aur_helper: cfg.aur_helper().or_else(|| Some("paru".into())),
    };
    let mut steps = Vec::new();
    let mut done = std::collections::HashSet::new();
    for id in ids {
        let Some(r) = recipes::find(&all, id) else { anyhow::bail!("no recipe named {id}") };
        for need in &r.needs {
            if done.insert(need.clone())
                && let Some(dep) = recipes::find(&all, need)
            {
                steps.extend(dep.plan(&opts));
            }
        }
        if done.insert(id.clone()) {
            steps.extend(r.plan(&opts));
        }
    }
    if dry_run {
        for s in &steps {
            println!("# {}\n{}", s.title, s.command_line());
        }
        return Ok(());
    }
    exec::run_inherit(&steps)
}

fn section(title: &str, items: &[String]) {
    if !items.is_empty() {
        println!("  {title:<12} {}", items.join(", "));
    }
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

pub fn date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| ts.to_string())
}
