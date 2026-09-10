//! The interface: state, keys, background work. Drawing lives in `draw.rs`.

mod draw;
pub mod theme;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::cache::Cache;
use crate::config::Config;
use crate::exec::{ExecEvent, Runner};
use crate::i18n::t;
use crate::index::Index;
use crate::model::{Details, Package, Source, Update};
use crate::queue::{Action, Preflight, Queue, Step};
use crate::recipes::{ApplyOptions, Recipe};
use crate::sources::appstream::{AppInfo, SHELVES};
use crate::sources::aur::{Aur, SearchBy};
use crate::sources::news::NewsItem;
use crate::sources::{appstream, news, pacman, pkgstats};
use theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Store,
    Search,
    Installed,
    Updates,
    Queue,
    Recipes,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 7] =
        [Tab::Store, Tab::Search, Tab::Installed, Tab::Updates, Tab::Queue, Tab::Recipes, Tab::Settings];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Store => t("Store"),
            Tab::Search => t("Search"),
            Tab::Installed => t("Installed"),
            Tab::Updates => t("Updates"),
            Tab::Queue => t("Queue"),
            Tab::Recipes => t("Recipes"),
            Tab::Settings => t("Settings"),
        }
    }
    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Darkside: BlackArch tools by pacman group, as extra Store shelves once the
/// repository is enabled. This synthetic group gathers defensive and detection
/// tools (blue and red together, what Kali calls Purple) from any repository.
pub const PURPLE_GROUP: &str = "purple team";
const PURPLE_TOOLS: &[&str] = &[
    "suricata",
    "zeek",
    "yara",
    "osquery",
    "velociraptor",
    "wazuh-agent",
    "lynis",
    "rkhunter",
    "chkrootkit",
    "clamav",
    "fail2ban",
    "crowdsec",
    "wireshark-qt",
    "arkime",
    "volatility3",
    "sleuthkit",
    "autopsy",
    "chainsaw",
    "hayabusa",
    "sigma-cli",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Info,
    Deps,
    Files,
    Pkgbuild,
}

impl DetailTab {
    const ALL: [DetailTab; 4] = [DetailTab::Info, DetailTab::Deps, DetailTab::Files, DetailTab::Pkgbuild];
    pub fn title(self) -> &'static str {
        match self {
            DetailTab::Info => t("Info"),
            DetailTab::Deps => t("Dependencies"),
            DetailTab::Files => t("Files"),
            DetailTab::Pkgbuild => t("PKGBUILD"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstalledFilter {
    All,
    Explicit,
    Dependencies,
    Orphans,
    Foreign,
}

impl InstalledFilter {
    const ALL: [InstalledFilter; 5] = [
        InstalledFilter::All,
        InstalledFilter::Explicit,
        InstalledFilter::Dependencies,
        InstalledFilter::Orphans,
        InstalledFilter::Foreign,
    ];
    pub fn title(self) -> &'static str {
        match self {
            InstalledFilter::All => t("all"),
            InstalledFilter::Explicit => t("explicit"),
            InstalledFilter::Dependencies => t("dependencies"),
            InstalledFilter::Orphans => t("orphans"),
            InstalledFilter::Foreign => t("AUR / local"),
        }
    }
}

/// Everything that arrives from background work or the terminal.
pub enum Msg {
    Key(KeyEvent),
    Resize,
    Tick,
    AurResults { query: String, packages: Vec<Package> },
    Details { name: String, details: Box<Details> },
    Files { name: String, files: Vec<String> },
    Pkgbuild { name: String, text: String },
    Updates(Vec<Update>),
    News(Vec<NewsItem>),
    Popularity(HashMap<String, f64>),
    Orphans(Vec<String>),
    Preflight(Preflight),
    RecipeStatus { id: String, applied: bool },
    IndexReloaded(Index),
    Exec(ExecEvent),
    Error(String),
    NewRelease(String),
    UpdateReady(std::path::PathBuf),
}

/// A running sequence of commands, shown full screen.
pub struct RunState {
    pub steps: Vec<Step>,
    pub current: usize,
    pub lines: VecDeque<String>,
    pub partial: String,
    pub runner: Option<Runner>,
    pub finished: bool,
    pub failed: bool,
    pub title: String,
    exec_rx: Option<std_mpsc::Receiver<ExecEvent>>,
}

pub struct App {
    pub cfg: Config,
    pub theme: Theme,
    pub index: Index,
    pub apps: HashMap<String, AppInfo>,
    pub popularity: HashMap<String, f64>,
    pub tab: Tab,
    // Search
    pub query: String,
    pub typing: bool,
    pub results: Vec<String>,
    pub search_sel: usize,
    aur_due: Option<Instant>,
    aur_pending: Option<String>,
    // Store
    pub shelf_sel: usize,
    pub shelf_focus: bool,
    pub store_items: Vec<String>,
    pub store_sel: usize,
    /// Darkside: BlackArch groups (name without the prefix, tool count), shown
    /// as Store shelves after the regular ones.
    pub dark_groups: Vec<(String, usize)>,
    // Installed
    pub inst_filter: InstalledFilter,
    pub inst_items: Vec<String>,
    pub inst_sel: usize,
    orphans: Option<Vec<String>>,
    // Updates
    pub updates: Option<Vec<Update>>,
    pub news: Vec<NewsItem>,
    pub upd_sel: usize,
    // Queue
    pub queue: Queue,
    pub queue_sel: usize,
    pub preflight: Option<Preflight>,
    preflight_dirty: bool,
    // Recipes
    pub recipes: Vec<Recipe>,
    pub recipe_status: HashMap<String, bool>,
    pub recipe_sel: usize,
    // Settings
    pub settings_sel: usize,
    pub new_release: Option<String>,
    // Details
    pub detail_tab: DetailTab,
    pub details: HashMap<String, Details>,
    pub files: HashMap<String, Vec<String>>,
    pub pkgbuilds: HashMap<String, String>,
    pub detail_scroll: u16,
    requested: HashSet<String>,
    // Run screen
    pub run: Option<RunState>,
    pub show_help: bool,
    pub status: String,
    pub busy: Option<String>,
    pub should_quit: bool,
    pub size: (u16, u16),
    tx: mpsc::UnboundedSender<Msg>,
    http: reqwest::Client,
    aur: Arc<Aur>,
    cache: Cache,
}

/// The entry point: load, loop, restore.
pub async fn run(cfg: Config, cache: Cache) -> Result<()> {
    let index = tokio::task::spawn_blocking(load_index).await??;
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let http = reqwest::Client::builder()
        .user_agent(concat!("sanae/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()?;
    let aur = Arc::new(Aur::new(cache.clone())?);
    let theme = Theme::from_config(&cfg.theme);
    let apps = tokio::task::spawn_blocking(|| appstream::load().unwrap_or_default()).await?;
    let mut app = App {
        cfg,
        theme,
        index,
        apps,
        popularity: HashMap::new(),
        tab: Tab::Store,
        query: String::new(),
        typing: false,
        results: Vec::new(),
        search_sel: 0,
        aur_due: None,
        aur_pending: None,
        shelf_sel: 0,
        shelf_focus: true,
        store_items: Vec::new(),
        store_sel: 0,
        dark_groups: Vec::new(),
        inst_filter: InstalledFilter::Explicit,
        inst_items: Vec::new(),
        inst_sel: 0,
        orphans: None,
        updates: None,
        news: Vec::new(),
        upd_sel: 0,
        queue: Queue::default(),
        queue_sel: 0,
        preflight: None,
        preflight_dirty: true,
        recipes: crate::recipes::load_all(),
        recipe_status: HashMap::new(),
        recipe_sel: 0,
        settings_sel: 0,
        new_release: None,
        detail_tab: DetailTab::Info,
        details: HashMap::new(),
        files: HashMap::new(),
        pkgbuilds: HashMap::new(),
        detail_scroll: 0,
        requested: HashSet::new(),
        run: None,
        show_help: false,
        status: String::new(),
        busy: None,
        should_quit: false,
        size: (80, 24),
        tx: tx.clone(),
        http,
        aur,
        cache,
    };
    if app.apps.is_empty() {
        app.status = t("No AppStream data: install archlinux-appstream-data for the store shelves.").into();
    }
    app.refresh_store();
    app.refresh_installed();
    app.fetch_popularity();
    app.fetch_updates();
    app.check_recipes();
    app.refresh_dark_groups();
    app.check_self_update();

    // Terminal events on a plain thread; everything else is async.
    let tx_keys = tx.clone();
    std::thread::spawn(move || {
        loop {
            match event::poll(Duration::from_millis(150)) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(k)) if k.kind == KeyEventKind::Press || k.kind == KeyEventKind::Repeat => {
                        if tx_keys.send(Msg::Key(k)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Resize(_, _)) => {
                        let _ = tx_keys.send(Msg::Resize);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                Ok(false) => {
                    if tx_keys.send(Msg::Tick).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut terminal = ratatui::init();
    let result = main_loop(&mut terminal, &mut app, &mut rx).await;
    ratatui::restore();
    result
}

async fn main_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
) -> Result<()> {
    loop {
        let area = terminal.size()?;
        app.size = (area.width, area.height);
        terminal.draw(|f| draw::draw(f, app))?;
        let Some(msg) = rx.recv().await else { break };
        app.handle(msg);
        // Drain what else is waiting so a burst of output draws once.
        while let Ok(msg) = rx.try_recv() {
            app.handle(msg);
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn load_index() -> Result<Index> {
    let sync = pacman::sync_all()?;
    let local = pacman::local_all()?;
    Ok(Index::build(sync, local))
}

impl App {
    // ---------------------------------------------------------------- data

    /// Recipes shown on the Recipes tab (software), in order.
    pub fn software_recipes(&self) -> Vec<&Recipe> {
        self.recipes.iter().filter(|r| r.category != crate::recipes::SOURCES_CATEGORY).collect()
    }

    /// Recipes shown on the Settings tab (package sources), in order.
    pub fn source_recipes(&self) -> Vec<&Recipe> {
        self.recipes.iter().filter(|r| r.category == crate::recipes::SOURCES_CATEGORY).collect()
    }

    /// Settings rows before the sources: id, label, value.
    pub fn settings_rows(&self) -> Vec<(&'static str, String, String)> {
        let helper = match self.cfg.general.aur_helper.as_str() {
            "auto" | "" => tfmt!("auto ({})", self.cfg.aur_helper().unwrap_or_else(|| t("none found").into())),
            h => h.to_string(),
        };
        vec![
            (
                "check_updates",
                t("Check for a newer Sanae at start").into(),
                if self.cfg.general.check_updates { t("on").into() } else { t("off").into() },
            ),
            (
                "self_update",
                t("Update Sanae now").into(),
                match &self.new_release {
                    Some(tag) => tfmt!("{} available (you run v{})", tag, env!("CARGO_PKG_VERSION")),
                    None => tfmt!("v{} · Enter fetches the latest release", env!("CARGO_PKG_VERSION")),
                },
            ),
            ("aur_helper", t("AUR helper").into(), helper),
            (
                "privilege",
                t("Administrator tool").into(),
                format!("{} ({})", self.cfg.general.privilege, self.cfg.privilege()),
            ),
            (
                "nerd_font",
                t("Nerd Font marks").into(),
                if self.cfg.theme.nerd_font { t("on").into() } else { t("off").into() },
            ),
            (
                "language",
                t("Language").into(),
                match self.cfg.general.language.as_str() {
                    "auto" | "" => tfmt!("auto ({})", crate::i18n::current()),
                    l => l.to_string(),
                },
            ),
        ]
    }

    fn check_self_update(&self) {
        if !self.cfg.general.check_updates {
            return;
        }
        let (http, cache, tx) = (self.http.clone(), self.cache.clone(), self.tx.clone());
        tokio::spawn(async move {
            if let Ok(tag) = crate::selfupdate::latest(&http, &cache).await
                && crate::selfupdate::is_newer(&tag)
            {
                let _ = tx.send(Msg::NewRelease(tag));
            }
        });
    }

    fn settings_len(&self) -> usize {
        self.settings_rows().len() + self.source_recipes().len()
    }

    /// Enter on a Settings row.
    fn settings_activate(&mut self) {
        let rows = self.settings_rows();
        if self.settings_sel < rows.len() {
            match rows[self.settings_sel].0 {
                "check_updates" => self.cfg.general.check_updates = !self.cfg.general.check_updates,
                "self_update" => {
                    if self.busy.is_some() {
                        return;
                    }
                    self.busy = Some(t("Downloading the latest Sanae…").into());
                    let dir = if self.cache.dir().as_os_str().is_empty() {
                        std::env::temp_dir()
                    } else {
                        self.cache.dir().clone()
                    };
                    if crate::selfupdate::has_wget() {
                        self.busy = None;
                        let steps =
                            crate::selfupdate::wget_steps(&self.cfg.privilege(), &dir, &crate::selfupdate::target());
                        self.start_run(t("Updating Sanae"), steps);
                        return;
                    }
                    let tx = self.tx.clone();
                    tokio::spawn(async move {
                        match crate::selfupdate::download(&dir).await {
                            Ok(p) => {
                                let _ = tx.send(Msg::UpdateReady(p));
                            }
                            Err(e) => {
                                let _ = tx.send(Msg::Error(tfmt!("update: {}", format!("{e:#}"))));
                            }
                        }
                    });
                    return;
                }
                "aur_helper" => {
                    self.cfg.general.aur_helper = match self.cfg.general.aur_helper.as_str() {
                        "auto" | "" => "paru".into(),
                        "paru" => "yay".into(),
                        _ => "auto".into(),
                    }
                }
                "privilege" => {
                    self.cfg.general.privilege = match self.cfg.general.privilege.as_str() {
                        "auto" | "" => "sudo".into(),
                        "sudo" => "doas".into(),
                        _ => "auto".into(),
                    }
                }
                "nerd_font" => {
                    self.cfg.theme.nerd_font = !self.cfg.theme.nerd_font;
                    self.theme = Theme::from_config(&self.cfg.theme);
                }
                "language" => {
                    let langs = crate::i18n::LANGS;
                    let cur = self.cfg.general.language.as_str();
                    self.cfg.general.language = match langs.iter().position(|(c, _)| *c == cur) {
                        None => langs[0].0.into(),
                        Some(i) if i + 1 < langs.len() => langs[i + 1].0.into(),
                        Some(_) => "auto".into(),
                    };
                    crate::i18n::set(&self.cfg.general.language);
                }
                _ => {}
            }
            match self.cfg.save() {
                Ok(()) => self.status = t("Settings saved.").into(),
                Err(e) => self.status = tfmt!("could not save the settings: {}", e),
            }
            return;
        }
        let i = self.settings_sel - rows.len();
        let Some(r) = self.source_recipes().get(i).map(|r| (*r).clone()) else { return };
        self.run_recipe(r);
    }

    fn refresh_store(&mut self) {
        if let Some(d) = self.dark_shelf() {
            self.store_items = self.dark_tools(d);
            if self.store_sel >= self.store_items.len() {
                self.store_sel = 0;
            }
            return;
        }
        let shelf = self.shelf_name(self.shelf_sel);
        let mut items: Vec<(&AppInfo, f64)> = self
            .apps
            .values()
            .filter(|a| self.index.get(&a.pkgname).is_some())
            .filter(|a| match shelf {
                "Featured" => !self.index.get(&a.pkgname).is_some_and(|p| p.is_installed()),
                s => a.shelf() == s,
            })
            .map(|a| (a, self.popularity.get(&a.pkgname).copied().unwrap_or(0.0)))
            .collect();
        items.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.name.cmp(&b.0.name))
        });
        let limit = if shelf == "Featured" { 60 } else { usize::MAX };
        self.store_items = items.into_iter().take(limit).map(|(a, _)| a.pkgname.clone()).collect();
        if self.store_sel >= self.store_items.len() {
            self.store_sel = 0;
        }
    }

    /// Rebuild the Darkside groups from the index (after a load or reload).
    fn refresh_dark_groups(&mut self) {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for p in &self.index.packages {
            if !matches!(&p.source, Source::Repo(r) if r == "blackarch") {
                continue;
            }
            for g in &p.groups {
                if let Some(n) = g.strip_prefix("blackarch-") {
                    *counts.entry(n.to_string()).or_default() += 1;
                }
            }
        }
        let mut groups: Vec<(String, usize)> = counts.into_iter().collect();
        groups.sort_by(|a, b| a.0.cmp(&b.0));
        if !groups.is_empty() {
            let purple = PURPLE_TOOLS.iter().filter(|n| self.index.get(n).is_some()).count();
            groups.insert(0, (PURPLE_GROUP.into(), purple));
        }
        self.dark_groups = groups;
        if self.shelf_sel >= self.shelf_count() {
            self.shelf_sel = 0;
        }
        self.refresh_store();
    }

    /// The Darkside group behind the selected shelf, if it is one.
    pub fn dark_shelf(&self) -> Option<usize> {
        self.shelf_sel.checked_sub(SHELVES.len() + 1).filter(|d| *d < self.dark_groups.len())
    }

    /// The tools of a Darkside group, most installed first.
    fn dark_tools(&self, d: usize) -> Vec<String> {
        let Some((group, _)) = self.dark_groups.get(d) else { return Vec::new() };
        let mut items: Vec<(&Package, f64)> = if group == PURPLE_GROUP {
            PURPLE_TOOLS.iter().filter_map(|n| self.index.get(n)).collect::<Vec<_>>()
        } else {
            let g = format!("blackarch-{group}");
            self.index.packages.iter().filter(|p| p.groups.contains(&g)).collect()
        }
        .into_iter()
        .map(|p| (p, self.popularity.get(&p.name).copied().unwrap_or(0.0)))
        .collect();
        items.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.name.cmp(&b.0.name))
        });
        items.into_iter().map(|(p, _)| p.name.clone()).collect()
    }

    /// Queue every tool of the selected Darkside shelf that is not installed yet.
    fn queue_dark_group(&mut self) {
        if self.dark_shelf().is_none() {
            return;
        }
        let names: Vec<String> = self
            .store_items
            .iter()
            .filter(|n| self.index.get(n).is_some_and(|p| !p.is_installed()) && self.queue.get(n).is_none())
            .cloned()
            .collect();
        for n in &names {
            let aur = self.index.get(n).is_some_and(|p| p.source.is_aur());
            self.queue.toggle(n, Action::Install, aur);
        }
        self.preflight_dirty = true;
        self.status = tfmt!("{} tools queued", names.len());
    }

    /// The shelf's name; every Darkside shelf answers "Darkside" (see `dark_group_at`).
    pub fn shelf_name(&self, i: usize) -> &'static str {
        if i == 0 {
            "Featured"
        } else if i <= SHELVES.len() {
            SHELVES[i - 1].0
        } else {
            "Darkside"
        }
    }

    /// The BlackArch group (name, tool count) behind shelf `i`, if it is a Darkside one.
    pub fn dark_group_at(&self, i: usize) -> Option<&(String, usize)> {
        i.checked_sub(SHELVES.len() + 1).and_then(|d| self.dark_groups.get(d))
    }

    pub fn shelf_count(&self) -> usize {
        SHELVES.len() + 1 + self.dark_groups.len()
    }

    fn refresh_installed(&mut self) {
        let mut names: Vec<String> = match self.inst_filter {
            InstalledFilter::All => self.index.installed().map(|p| p.name.clone()).collect(),
            InstalledFilter::Explicit => self.index.installed_by_reason(true).iter().map(|p| p.name.clone()).collect(),
            InstalledFilter::Dependencies => {
                self.index.installed_by_reason(false).iter().map(|p| p.name.clone()).collect()
            }
            InstalledFilter::Orphans => match &self.orphans {
                Some(o) => o.clone(),
                None => {
                    let tx = self.tx.clone();
                    tokio::task::spawn_blocking(move || {
                        let _ = tx.send(Msg::Orphans(pacman::orphans().unwrap_or_default()));
                    });
                    Vec::new()
                }
            },
            InstalledFilter::Foreign => self
                .index
                .installed()
                .filter(|p| matches!(p.source, Source::Aur | Source::Local))
                .map(|p| p.name.clone())
                .collect(),
        };
        names.sort();
        self.inst_items = names;
        if self.inst_sel >= self.inst_items.len() {
            self.inst_sel = 0;
        }
    }

    fn refresh_search(&mut self) {
        self.results = self.index.search(&self.query, 200).into_iter().map(|p| p.name.clone()).collect();
        if self.search_sel >= self.results.len() {
            self.search_sel = 0;
        }
    }

    fn fetch_popularity(&self) {
        let (http, cache, tx) = (self.http.clone(), self.cache.clone(), self.tx.clone());
        tokio::spawn(async move {
            match pkgstats::top(&http, &cache).await {
                Ok(map) => {
                    let _ = tx.send(Msg::Popularity(map));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("popularity: {e:#}")));
                }
            }
        });
    }

    fn fetch_updates(&mut self) {
        let tx = self.tx.clone();
        let (http, cache, aur) = (self.http.clone(), self.cache.clone(), self.aur.clone());
        let foreign: Vec<(String, String)> = self
            .index
            .installed()
            .filter(|p| matches!(p.source, Source::Aur | Source::Local))
            .map(|p| (p.name.clone(), p.version.clone()))
            .collect();
        tokio::spawn(async move {
            match news::fetch(&http, &cache).await {
                Ok(items) => {
                    let _ = tx.send(Msg::News(items));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Error(format!("news: {e:#}")));
                }
            }
            let mut ups =
                tokio::task::spawn_blocking(pacman::updates).await.ok().and_then(Result::ok).unwrap_or_default();
            let names: Vec<String> = foreign.iter().map(|(n, _)| n.clone()).collect();
            if !names.is_empty()
                && let Ok(found) = aur.info(&names).await
            {
                for r in found {
                    if let Some((_, cur)) = foreign.iter().find(|(n, _)| *n == r.name)
                        && pacman::vercmp(cur, &r.version) == std::cmp::Ordering::Less
                    {
                        ups.push(Update {
                            name: r.name.clone(),
                            current: cur.clone(),
                            new: r.version.clone(),
                            source: Source::Aur,
                        });
                    }
                }
            }
            let _ = tx.send(Msg::Updates(ups));
        });
    }

    fn check_recipes(&self) {
        let installed: HashSet<String> = self.index.installed().map(|p| p.name.clone()).collect();
        for r in self.recipes.clone() {
            let tx = self.tx.clone();
            let installed = installed.clone();
            tokio::task::spawn_blocking(move || {
                let applied = r.is_applied(&|p| installed.contains(p));
                let _ = tx.send(Msg::RecipeStatus { id: r.id.clone(), applied });
            });
        }
    }

    fn request_details(&mut self, name: &str) {
        if self.details.contains_key(name) || self.requested.contains(name) {
            return;
        }
        let Some(p) = self.index.get(name) else { return };
        self.requested.insert(name.to_string());
        let tx = self.tx.clone();
        let n = name.to_string();
        let installed = p.is_installed();
        if matches!(p.source, Source::Repo(_)) || installed {
            tokio::task::spawn_blocking(move || {
                if let Ok(d) = pacman::details(&n, installed) {
                    let _ = tx.send(Msg::Details { name: n, details: Box::new(d) });
                }
            });
        } else {
            let aur = self.aur.clone();
            tokio::spawn(async move {
                if let Ok(found) = aur.info(std::slice::from_ref(&n)).await
                    && let Some(r) = found.first()
                {
                    let _ = tx.send(Msg::Details { name: n, details: Box::new(r.to_details()) });
                }
            });
        }
    }

    fn request_files(&mut self, name: &str) {
        if self.files.contains_key(name) {
            return;
        }
        let Some(p) = self.index.get(name) else { return };
        if p.source.is_aur() && !p.is_installed() {
            self.files.insert(name.to_string(), vec![t("(AUR packages list their files only once installed)").into()]);
            return;
        }
        let (tx, n, installed) = (self.tx.clone(), name.to_string(), p.is_installed());
        self.files.insert(name.to_string(), vec![t("loading…").into()]);
        tokio::task::spawn_blocking(move || {
            let files = pacman::files(&n, installed).unwrap_or_else(|e| vec![format!("{e:#}")]);
            let _ = tx.send(Msg::Files { name: n, files });
        });
    }

    fn request_pkgbuild(&mut self, name: &str) {
        if self.pkgbuilds.contains_key(name) {
            return;
        }
        let Some(p) = self.index.get(name) else { return };
        if !p.source.is_aur() {
            self.pkgbuilds.insert(name.to_string(), t("(only AUR packages have a PKGBUILD to show)").into());
            return;
        }
        let base = self.details.get(name).and_then(|d| d.package_base.clone()).unwrap_or_else(|| name.to_string());
        self.pkgbuilds.insert(name.to_string(), t("loading…").into());
        let (tx, n, aur) = (self.tx.clone(), name.to_string(), self.aur.clone());
        tokio::spawn(async move {
            let text = aur.pkgbuild(&base).await.unwrap_or_else(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Pkgbuild { name: n, text });
        });
    }

    fn request_preflight(&mut self) {
        if !self.preflight_dirty {
            return;
        }
        self.preflight_dirty = false;
        let q = self.queue.clone();
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(Msg::Preflight(q.preflight()));
        });
    }

    fn reload_index(&mut self) {
        self.busy = Some(t("Reading the package databases…").into());
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || match load_index() {
            Ok(i) => {
                let _ = tx.send(Msg::IndexReloaded(i));
            }
            Err(e) => {
                let _ = tx.send(Msg::Error(format!("{e:#}")));
            }
        });
    }

    // ------------------------------------------------------------ selection

    /// The package under the cursor on the current tab.
    pub fn current_name(&self) -> Option<&str> {
        match self.tab {
            Tab::Search => self.results.get(self.search_sel).map(String::as_str),
            Tab::Store => self.store_items.get(self.store_sel).map(String::as_str),
            Tab::Installed => self.inst_items.get(self.inst_sel).map(String::as_str),
            Tab::Updates => self.updates.as_ref().and_then(|u| u.get(self.upd_sel)).map(|u| u.name.as_str()),
            Tab::Queue => self.queue.items.get(self.queue_sel).map(|i| i.name.as_str()),
            Tab::Recipes | Tab::Settings => None,
        }
    }

    fn list_len(&self) -> usize {
        match self.tab {
            Tab::Search => self.results.len(),
            Tab::Store => {
                if self.shelf_focus {
                    self.shelf_count()
                } else {
                    self.store_items.len()
                }
            }
            Tab::Installed => self.inst_items.len(),
            Tab::Updates => self.updates.as_ref().map_or(0, Vec::len),
            Tab::Queue => self.queue.len(),
            Tab::Recipes => self.software_recipes().len(),
            Tab::Settings => self.settings_len(),
        }
    }

    fn sel_mut(&mut self) -> &mut usize {
        match self.tab {
            Tab::Search => &mut self.search_sel,
            Tab::Store => {
                if self.shelf_focus {
                    &mut self.shelf_sel
                } else {
                    &mut self.store_sel
                }
            }
            Tab::Installed => &mut self.inst_sel,
            Tab::Updates => &mut self.upd_sel,
            Tab::Queue => &mut self.queue_sel,
            Tab::Recipes => &mut self.recipe_sel,
            Tab::Settings => &mut self.settings_sel,
        }
    }

    fn move_sel(&mut self, delta: i32) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let sel = self.sel_mut();
        let next = (*sel as i64 + i64::from(delta)).clamp(0, len as i64 - 1) as usize;
        *sel = next;
        self.detail_scroll = 0;
        if self.tab == Tab::Store && self.shelf_focus {
            self.refresh_store();
        }
        self.on_selection_changed();
    }

    fn on_selection_changed(&mut self) {
        if let Some(name) = self.current_name().map(String::from) {
            self.request_details(&name);
            match self.detail_tab {
                DetailTab::Files => self.request_files(&name),
                DetailTab::Pkgbuild => self.request_pkgbuild(&name),
                _ => {}
            }
        }
    }

    fn toggle_queue(&mut self, action: Action) {
        let Some(name) = self.current_name().map(String::from) else { return };
        let Some(p) = self.index.get(&name) else { return };
        let aur = p.source.is_aur() || matches!(p.source, Source::Local);
        let action = match (action, p.is_installed()) {
            (Action::Install, true) => Action::Remove,
            (a, _) => a,
        };
        self.queue.toggle(&name, action, aur);
        self.preflight_dirty = true;
        self.status = tfmt!("{} in the queue", self.queue.len());
    }

    // --------------------------------------------------------------- running

    fn start_run(&mut self, title: &str, steps: Vec<Step>) {
        if steps.is_empty() {
            self.status = t("Nothing to do.").into();
            return;
        }
        self.run = Some(RunState {
            steps,
            current: 0,
            lines: VecDeque::new(),
            partial: String::new(),
            runner: None,
            finished: false,
            failed: false,
            title: title.to_string(),
            exec_rx: None,
        });
        self.spawn_current_step();
    }

    fn spawn_current_step(&mut self) {
        let (rows, cols) = (self.size.1.saturating_sub(6).max(5), self.size.0.saturating_sub(4).max(20));
        let Some(run) = self.run.as_mut() else { return };
        let Some(step) = run.steps.get(run.current).cloned() else { return };
        run.lines.push_back(format!("▸ {}", step.title));
        run.lines.push_back(format!("  $ {}", step.command_line()));
        let (etx, erx) = std_mpsc::channel();
        match Runner::spawn(&step, rows, cols, etx) {
            Ok(r) => {
                run.runner = Some(r);
                run.exec_rx = Some(erx);
                // Bridge the std channel into the async one.
                let tx = self.tx.clone();
                let rx = run.exec_rx.take().expect("just set");
                std::thread::spawn(move || {
                    while let Ok(ev) = rx.recv() {
                        if tx.send(Msg::Exec(ev)).is_err() {
                            break;
                        }
                    }
                });
            }
            Err(e) => {
                run.lines.push_back(format!("✖ {e:#}"));
                run.finished = true;
                run.failed = true;
            }
        }
    }

    fn on_exec(&mut self, ev: ExecEvent) {
        let Some(run) = self.run.as_mut() else { return };
        match ev {
            ExecEvent::Line(l) => {
                run.partial.clear();
                run.lines.push_back(l);
                while run.lines.len() > 2000 {
                    run.lines.pop_front();
                }
            }
            ExecEvent::Partial(p) => run.partial = p,
            ExecEvent::Done(code) => {
                run.partial.clear();
                run.runner = None;
                if code != 0 {
                    run.lines.push_back(format!("✖ exited with {code}"));
                    run.failed = true;
                    run.finished = true;
                } else if run.current + 1 < run.steps.len() {
                    run.current += 1;
                    self.spawn_current_step();
                } else {
                    run.lines.push_back("✔ done".into());
                    run.finished = true;
                }
            }
        }
    }

    fn close_run(&mut self) {
        if let Some(run) = self.run.take() {
            if !run.failed {
                self.queue.clear();
                self.preflight_dirty = true;
            }
            self.details.clear();
            self.requested.clear();
            self.orphans = None;
            self.updates = None;
            self.reload_index();
        }
    }

    // ------------------------------------------------------------------ keys

    pub fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Key(k) => self.on_key(k),
            Msg::Resize => {
                if let Some(run) = &self.run
                    && let Some(r) = &run.runner
                {
                    r.resize(self.size.1.saturating_sub(6).max(5), self.size.0.saturating_sub(4).max(20));
                }
            }
            Msg::Tick => {
                if let Some(due) = self.aur_due
                    && Instant::now() >= due
                {
                    self.aur_due = None;
                    let q = self.query.clone();
                    if q.len() >= 2 {
                        self.aur_pending = Some(q.clone());
                        let (aur, tx) = (self.aur.clone(), self.tx.clone());
                        tokio::spawn(async move {
                            match aur.search(&q, SearchBy::NameDesc).await {
                                Ok(found) => {
                                    let _ = tx.send(Msg::AurResults {
                                        query: q,
                                        packages: found.iter().map(|r| r.to_package()).collect(),
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(Msg::Error(format!("AUR: {e:#}")));
                                }
                            }
                        });
                    }
                }
            }
            Msg::AurResults { query, packages } => {
                if self.aur_pending.as_deref() == Some(query.as_str()) {
                    self.aur_pending = None;
                }
                self.index.merge_aur(packages);
                if self.tab == Tab::Search && self.query == query {
                    let keep = self.results.get(self.search_sel).cloned();
                    self.refresh_search();
                    if let Some(k) = keep
                        && let Some(pos) = self.results.iter().position(|n| *n == k)
                    {
                        self.search_sel = pos;
                    }
                }
            }
            Msg::Details { name, details } => {
                self.details.insert(name, *details);
            }
            Msg::Files { name, files } => {
                self.files.insert(name, files);
            }
            Msg::Pkgbuild { name, text } => {
                self.pkgbuilds.insert(name, text);
            }
            Msg::Updates(u) => {
                self.updates = Some(u);
                self.upd_sel = 0;
            }
            Msg::News(n) => self.news = n,
            Msg::Popularity(map) => {
                for (name, pop) in &map {
                    if let Some(p) = self.index.get_mut(name)
                        && !p.source.is_aur()
                    {
                        p.popularity = Some(*pop);
                    }
                }
                self.popularity = map;
                self.refresh_store();
            }
            Msg::Orphans(o) => {
                self.orphans = Some(o);
                if self.inst_filter == InstalledFilter::Orphans {
                    self.refresh_installed();
                }
            }
            Msg::Preflight(p) => self.preflight = Some(p),
            Msg::RecipeStatus { id, applied } => {
                self.recipe_status.insert(id, applied);
            }
            Msg::IndexReloaded(i) => {
                self.index = i;
                self.busy = None;
                self.refresh_search();
                self.refresh_installed();
                self.refresh_store();
                self.refresh_dark_groups();
                self.fetch_updates();
                self.check_recipes();
                self.status = t("Package databases reloaded.").into();
            }
            Msg::Exec(ev) => self.on_exec(ev),
            Msg::Error(e) => {
                self.busy = None;
                self.status = e;
            }
            Msg::UpdateReady(path) => {
                self.busy = None;
                let steps =
                    crate::selfupdate::install_steps(&self.cfg.privilege(), &path, &crate::selfupdate::target());
                self.start_run(t("Updating Sanae"), steps);
            }
            Msg::NewRelease(tag) => {
                self.status = tfmt!("Sanae {} is out: Settings (7) → Update Sanae now", tag);
                self.new_release = Some(tag);
            }
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        if self.run.is_some() {
            self.on_key_run(k);
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(k.code, KeyCode::Char('c') | KeyCode::Char('q')) {
            self.should_quit = true;
            return;
        }
        if self.tab == Tab::Search && self.typing {
            self.on_key_typing(k);
            return;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.tab == Tab::Search && !self.query.is_empty() && k.code == KeyCode::Esc {
                    self.query.clear();
                    self.refresh_search();
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char(c @ '1'..='7') => self.switch_tab(Tab::ALL[(c as u8 - b'1') as usize]),
            KeyCode::Char('/') => {
                self.switch_tab(Tab::Search);
                self.typing = true;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::PageDown => self.move_sel(15),
            KeyCode::PageUp => self.move_sel(-15),
            KeyCode::Home | KeyCode::Char('g') => self.move_sel(-100_000),
            KeyCode::End | KeyCode::Char('G') => self.move_sel(100_000),
            KeyCode::Tab => self.next_detail_tab(),
            KeyCode::Left | KeyCode::Char('h') => {
                if self.tab == Tab::Store {
                    self.shelf_focus = true;
                } else {
                    self.detail_scroll = self.detail_scroll.saturating_sub(5);
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.tab == Tab::Store {
                    self.shelf_focus = false;
                    self.on_selection_changed();
                } else {
                    self.detail_scroll = self.detail_scroll.saturating_add(5);
                }
            }
            KeyCode::Enter => match self.tab {
                Tab::Store if self.shelf_focus => {
                    self.shelf_focus = false;
                    self.on_selection_changed();
                }
                Tab::Recipes => self.apply_recipe(),
                Tab::Settings => self.settings_activate(),
                Tab::Queue => self.apply_queue(),
                Tab::Updates => self.update_all(),
                _ => self.next_detail_tab(),
            },
            KeyCode::Char(' ') => match self.tab {
                Tab::Recipes => self.apply_recipe(),
                Tab::Settings => self.settings_activate(),
                Tab::Updates => {}
                _ => self.toggle_queue(Action::Install),
            },
            KeyCode::Char('d') | KeyCode::Delete => match self.tab {
                Tab::Queue => {
                    if let Some(n) = self.current_name().map(String::from) {
                        self.queue.remove(&n);
                        self.preflight_dirty = true;
                        if self.queue_sel >= self.queue.len() && self.queue_sel > 0 {
                            self.queue_sel -= 1;
                        }
                    }
                }
                _ => self.toggle_queue(Action::Remove),
            },
            KeyCode::Char('i') => {
                if let Some(n) = self.current_name().map(String::from)
                    && let Some(p) = self.index.get(&n)
                {
                    if p.is_installed() {
                        self.status = tfmt!("{} is already installed", n);
                    } else {
                        let aur = p.source.is_aur();
                        let mut q = Queue::default();
                        q.toggle(&n, Action::Install, aur);
                        let steps = q.plan(&self.cfg);
                        self.start_run(&tfmt!("Installing {}", n), steps);
                    }
                }
            }
            KeyCode::Char('A') if self.tab == Tab::Store => self.queue_dark_group(),
            KeyCode::Char('a') => self.apply_queue(),
            KeyCode::Char('u') => self.update_all(),
            KeyCode::Char('c') if self.tab == Tab::Queue => {
                self.queue.clear();
                self.preflight_dirty = true;
            }
            KeyCode::Char('f') if self.tab == Tab::Installed => {
                let i = InstalledFilter::ALL.iter().position(|f| *f == self.inst_filter).unwrap_or(0);
                self.inst_filter = InstalledFilter::ALL[(i + 1) % InstalledFilter::ALL.len()];
                self.inst_sel = 0;
                self.refresh_installed();
                self.on_selection_changed();
            }
            KeyCode::Char('o') if self.tab == Tab::Installed => {
                let orphans = self.orphans.clone().unwrap_or_default();
                if orphans.is_empty() {
                    self.status = t("No orphans (or not checked yet: press f until 'orphans').").into();
                } else {
                    for o in &orphans {
                        self.queue.toggle(o, Action::Remove, false);
                    }
                    self.preflight_dirty = true;
                    self.status = tfmt!("{} orphans queued for removal", orphans.len());
                }
            }
            KeyCode::Char('r') => {
                self.reload_index();
            }
            _ => {}
        }
    }

    fn on_key_typing(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Down | KeyCode::Tab => {
                self.typing = false;
                self.on_selection_changed();
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refresh_search();
                self.aur_due = Some(Instant::now() + Duration::from_millis(400));
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.clear();
                self.refresh_search();
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.search_sel = 0;
                self.refresh_search();
                self.aur_due = Some(Instant::now() + Duration::from_millis(400));
            }
            _ => {}
        }
    }

    fn on_key_run(&mut self, k: KeyEvent) {
        let finished = self.run.as_ref().is_some_and(|r| r.finished);
        if finished {
            if matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.close_run();
            }
            return;
        }
        let Some(run) = self.run.as_mut() else { return };
        let Some(runner) = run.runner.as_mut() else { return };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char('c') if ctrl => {
                runner.write(b"\x03");
                runner.kill();
            }
            KeyCode::Char(c) => {
                let mut buf = [0u8; 4];
                runner.write(c.encode_utf8(&mut buf).as_bytes());
            }
            KeyCode::Enter => runner.write(b"\r"),
            KeyCode::Backspace => runner.write(b"\x7f"),
            KeyCode::Tab => runner.write(b"\t"),
            _ => {}
        }
    }

    fn switch_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.typing = false;
        self.detail_scroll = 0;
        match tab {
            Tab::Queue => self.request_preflight(),
            Tab::Installed => self.refresh_installed(),
            _ => {}
        }
        self.on_selection_changed();
    }

    fn next_detail_tab(&mut self) {
        let i = DetailTab::ALL.iter().position(|t| *t == self.detail_tab).unwrap_or(0);
        self.detail_tab = DetailTab::ALL[(i + 1) % DetailTab::ALL.len()];
        self.detail_scroll = 0;
        self.on_selection_changed();
    }

    fn apply_queue(&mut self) {
        if self.queue.is_empty() {
            self.status = t("The queue is empty: mark packages with space.").into();
            return;
        }
        let steps = self.queue.plan(&self.cfg);
        self.start_run(t("Applying the queue"), steps);
    }

    fn update_all(&mut self) {
        let attention = self.news.iter().filter(|n| n.needs_attention()).count();
        let steps = Queue::update_plan(&self.cfg);
        self.status = if attention > 0 {
            tfmt!("{} news items ask for attention; read them on the Updates tab", attention)
        } else {
            String::new()
        };
        self.start_run(t("Updating the system"), steps);
    }

    fn apply_recipe(&mut self) {
        let Some(r) = self.software_recipes().get(self.recipe_sel).map(|r| (*r).clone()) else { return };
        self.run_recipe(r);
    }

    fn run_recipe(&mut self, r: Recipe) {
        let user = std::env::var("SUDO_USER").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "root".into());
        let opts =
            ApplyOptions { chroot: None, user, privilege: self.cfg.privilege(), aur_helper: self.cfg.aur_helper() };
        let mut steps = Vec::new();
        for need in &r.needs {
            if let Some(dep) = crate::recipes::find(&self.recipes, need) {
                steps.extend(dep.plan(&opts));
            }
        }
        steps.extend(r.plan(&opts));
        self.start_run(&tfmt!("Recipe: {}", r.name), steps);
    }
}
