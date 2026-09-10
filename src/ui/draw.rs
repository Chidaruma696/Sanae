//! Rendering. Nothing in here changes state except list offsets.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};

use super::{App, DetailTab, Tab};
use crate::i18n::t;
use crate::model::Package;
use crate::queue::Action;
use crate::{human, truncate};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    if app.run.is_some() {
        draw_run(f, app, area);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(8), Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    draw_tabs(f, app, rows[0]);
    let main = rows[1];
    match app.tab {
        Tab::Recipes => draw_recipes(f, app, main),
        Tab::Settings => draw_settings(f, app, main),
        Tab::Queue => draw_queue(f, app, main),
        _ => {
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
                .split(main);
            match app.tab {
                Tab::Store => draw_store(f, app, parts[0]),
                Tab::Search => draw_search(f, app, parts[0]),
                Tab::Installed => draw_installed(f, app, parts[0]),
                Tab::Updates => draw_updates(f, app, parts[0]),
                _ => {}
            }
            draw_details(f, app, parts[1]);
        }
    }
    draw_status(f, app, rows[2]);
    draw_keys(f, app, rows[3]);
    if app.show_help {
        draw_help(f, app, area);
    }
}

fn block<'a>(app: &App, title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(app.theme.border(focused))
        .title(title.into())
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let extra = match t {
                Tab::Updates => app
                    .updates
                    .as_ref()
                    .map(|u| if u.is_empty() { String::new() } else { format!(" ({})", u.len()) })
                    .unwrap_or_else(|| " (…)".into()),
                Tab::Queue if !app.queue.is_empty() => format!(" ({})", app.queue.len()),
                _ => String::new(),
            };
            Line::from(vec![
                Span::styled(format!("{}", i + 1), app.theme.key()),
                Span::raw(format!(" {}{extra}", t.title())),
            ])
        })
        .collect();
    let brand = Span::styled(" 早苗 Sanae ", app.theme.title());
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .highlight_style(app.theme.highlight())
        .divider(Span::styled(" · ", app.theme.dim()));
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(13), Constraint::Min(10), Constraint::Length(14)])
        .split(area);
    f.render_widget(Paragraph::new(Line::from(brand)), cols[0]);
    f.render_widget(tabs, cols[1]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(t("? help  q quit"), app.theme.dim()))).alignment(Alignment::Right),
        cols[2],
    );
}

/// One package as a list row.
fn package_row<'a>(app: &App, p: &'a Package, width: u16) -> Line<'a> {
    let t = &app.theme;
    let mark = match app.queue.get(&p.name).map(|q| q.action) {
        Some(Action::Install) => Span::styled(t.queued_install(), t.accent2()),
        Some(Action::Remove) => Span::styled(t.queued_remove(), t.bad()),
        None if p.has_update() => Span::styled(t.update(), t.warn()),
        None if p.is_installed() => Span::styled(t.installed(), t.ok()),
        None => Span::raw(" "),
    };
    let human_name = app.apps.get(&p.name).map(|a| a.name.as_str());
    let name_col = match human_name {
        Some(h) if h != p.name => format!("{h}  ({})", p.name),
        _ => p.name.clone(),
    };
    let source = Span::styled(
        format!("{:<9}", truncate(p.source.label(), 9)),
        if p.source.is_aur() { t.warn() } else { t.dim() },
    );
    let pop = match p.popularity {
        Some(v) if !p.source.is_aur() && v > 0.0 => format!("{v:>5.1}%"),
        _ if p.source.is_aur() => format!("★{:<5}", p.votes.unwrap_or(0)),
        _ => "      ".into(),
    };
    let desc_room = usize::from(width).saturating_sub(9 + 2 + 34 + 8 + 4);
    let summary = app.apps.get(&p.name).map(|a| a.summary.as_str()).filter(|s| !s.is_empty()).unwrap_or(&p.description);
    Line::from(vec![
        mark,
        Span::raw(" "),
        source,
        Span::styled(format!("{:<34}", truncate(&name_col, 33)), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(pop, t.dim()),
        Span::raw("  "),
        Span::styled(truncate(summary, desc_room.max(10)), t.dim()),
    ])
}

fn draw_list(f: &mut Frame, app: &App, area: Rect, names: &[String], sel: usize, title: Line, focused: bool) {
    let inner_width = area.width.saturating_sub(4);
    let items: Vec<ListItem> = names
        .iter()
        .map(|n| match app.index.get(n) {
            Some(p) => ListItem::new(package_row(app, p, inner_width)),
            None => ListItem::new(Line::from(Span::styled(n.clone(), app.theme.dim()))),
        })
        .collect();
    let list = List::new(items)
        .block(block(app, title, focused))
        .highlight_style(app.theme.highlight())
        .highlight_symbol("▸ ");
    let mut state = ListState::default().with_selected(if names.is_empty() { None } else { Some(sel) });
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);
    let hint = if app.typing { t("type · Enter or ↓ to the results") } else { t("/ to type · Esc clears") };
    let pending = if app.aur_pending.is_some() {
        Span::styled(t("  searching the AUR…"), app.theme.warn())
    } else {
        Span::raw("")
    };
    let input = Paragraph::new(Line::from(vec![
        Span::styled("🔍 ", app.theme.accent()),
        Span::raw(app.query.clone()),
        Span::styled(if app.typing { "▏" } else { "" }, app.theme.accent()),
        pending,
    ]))
    .block(block(
        app,
        Line::from(vec![
            Span::styled(t(" Search "), app.theme.title()),
            Span::styled(format!(" {hint} "), app.theme.dim()),
        ]),
        app.typing,
    ));
    f.render_widget(input, rows[0]);
    let title = Line::from(vec![Span::styled(tfmt!(" {} results ", app.results.len()), app.theme.title())]);
    draw_list(f, app, rows[1], &app.results, app.search_sel, title, !app.typing);
}

fn draw_store(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(20)])
        .split(area);
    let shelves: Vec<ListItem> = (0..app.shelf_count())
        .map(|i| {
            let name = app.shelf_name(i);
            if name == "Darkside" {
                return ListItem::new(Line::from(Span::styled("☠ Darkside", app.theme.warn())));
            }
            let icon = match name {
                "Featured" => "★",
                "Internet" => "🌐",
                "Multimedia" => "🎵",
                "Graphics" => "🎨",
                "Office" => "📄",
                "Development" => "🛠",
                "Games" => "🎮",
                "Education" => "📚",
                "System" => "⚙",
                "Utilities" => "🧰",
                _ => "·",
            };
            ListItem::new(Line::from(format!("{icon} {}", t(name))))
        })
        .collect();
    let list = List::new(shelves)
        .block(block(app, Line::from(Span::styled(t(" Shelves "), app.theme.title())), app.shelf_focus))
        .highlight_style(app.theme.highlight())
        .highlight_symbol("▸ ");
    let mut st = ListState::default().with_selected(Some(app.shelf_sel));
    f.render_stateful_widget(list, cols[0], &mut st);
    let shelf = app.shelf_name(app.shelf_sel);
    if app.dark_shelf() && app.dark_open.is_none() {
        draw_dark_groups(f, app, cols[1]);
        return;
    }
    let title = if let Some((g, _)) = app.dark_open.and_then(|d| app.dark_groups.get(d)) {
        let purple = g == super::PURPLE_GROUP;
        let label = if purple { t("Purple team").to_string() } else { g.clone() };
        let sub = if purple {
            t("detection, forensics and hardening · from any repository")
        } else {
            t("by popularity · A queues the whole group · Esc back")
        };
        Line::from(vec![
            Span::styled(tfmt!(" ☠ Darkside · {} · {} tools ", label, app.store_items.len()), app.theme.warn()),
            Span::styled(format!(" {sub} "), app.theme.dim()),
        ])
    } else {
        let sub = if shelf == "Featured" { t("most installed apps you do not have yet") } else { t("by popularity") };
        Line::from(vec![
            Span::styled(tfmt!(" {} · {} apps ", t(shelf), app.store_items.len()), app.theme.title()),
            Span::styled(format!(" {sub} "), app.theme.dim()),
        ])
    };
    if app.apps.is_empty() && !app.dark_shelf() {
        let p = Paragraph::new(t("No AppStream catalog found.\n\nInstall archlinux-appstream-data (sudo pacman -S archlinux-appstream-data) and press r.\nThe Search tab works without it."))
            .wrap(Wrap { trim: false })
            .block(block(app, title, !app.shelf_focus));
        f.render_widget(p, cols[1]);
        return;
    }
    draw_list(f, app, cols[1], &app.store_items, app.store_sel, title, !app.shelf_focus);
}

/// The Darkside shelf before a group is opened: BlackArch's groups with their tool counts.
fn draw_dark_groups(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .dark_groups
        .iter()
        .map(|(g, n)| {
            let purple = g == super::PURPLE_GROUP;
            let label = if purple { t("Purple team").to_string() } else { g.clone() };
            let what = if purple { t("detection, forensics and hardening · from any repository") } else { "" };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {label:<18}"), app.theme.warn()),
                Span::styled(format!("{n:>5}  "), app.theme.dim()),
                Span::styled(what, app.theme.dim()),
            ]))
        })
        .collect();
    let title = Line::from(vec![
        Span::styled(tfmt!(" ☠ Darkside · {} groups ", app.dark_groups.len()), app.theme.warn()),
        Span::styled(format!(" {} ", t("BlackArch tools · Enter opens a group")), app.theme.dim()),
    ]);
    let list = List::new(items)
        .block(block(app, title, !app.shelf_focus))
        .highlight_style(app.theme.highlight())
        .highlight_symbol("▸ ");
    let mut st = ListState::default().with_selected(Some(app.store_sel.min(app.dark_groups.len().saturating_sub(1))));
    f.render_stateful_widget(list, area, &mut st);
}

fn draw_installed(f: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(tfmt!(" Installed · {} ", app.inst_filter.title()), app.theme.title()),
        Span::styled(
            tfmt!(" {} packages · f changes the filter · o queues the orphans ", app.inst_items.len()),
            app.theme.dim(),
        ),
    ]);
    draw_list(f, app, area, &app.inst_items, app.inst_sel, title, true);
}

fn draw_updates(f: &mut Frame, app: &App, area: Rect) {
    let has_news = !app.news.is_empty();
    let rows = if has_news {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(3)])
            .split(area)
    } else {
        Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(3)]).split(area)
    };
    if has_news {
        let lines: Vec<Line> = app
            .news
            .iter()
            .take(4)
            .map(|n| {
                let style = if n.needs_attention() { app.theme.warn() } else { app.theme.dim() };
                Line::from(vec![
                    Span::styled(format!("{:<17}", n.date), app.theme.dim()),
                    Span::styled(if n.needs_attention() { "⚠ " } else { "  " }, style),
                    Span::styled(n.title.clone(), style),
                ])
            })
            .collect();
        let p = Paragraph::new(Text::from(lines)).block(block(
            app,
            Line::from(vec![
                Span::styled(t(" Arch news "), app.theme.title()),
                Span::styled(t(" read before updating "), app.theme.dim()),
            ]),
            false,
        ));
        f.render_widget(p, rows[0]);
    }
    let list_area = if has_news { rows[1] } else { rows[0] };
    match &app.updates {
        None => {
            let p = Paragraph::new(t("Checking for updates…")).block(block(
                app,
                Line::from(Span::styled(t(" Updates "), app.theme.title())),
                true,
            ));
            f.render_widget(p, list_area);
        }
        Some(u) if u.is_empty() => {
            let p = Paragraph::new(t("Everything is up to date.")).block(block(
                app,
                Line::from(Span::styled(t(" Updates "), app.theme.title())),
                true,
            ));
            f.render_widget(p, list_area);
        }
        Some(u) => {
            let items: Vec<ListItem> = u
                .iter()
                .map(|up| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<6}", if up.source.is_aur() { "aur" } else { "repo" }),
                            if up.source.is_aur() { app.theme.warn() } else { app.theme.dim() },
                        ),
                        Span::styled(
                            format!("{:<32}", truncate(&up.name, 31)),
                            Style::new().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!("{} → ", up.current), app.theme.dim()),
                        Span::styled(up.new.clone(), app.theme.accent()),
                    ]))
                })
                .collect();
            let title = Line::from(vec![
                Span::styled(tfmt!(" {} updates ", u.len()), app.theme.title()),
                Span::styled(t(" u or Enter updates everything "), app.theme.dim()),
            ]);
            let list = List::new(items)
                .block(block(app, title, true))
                .highlight_style(app.theme.highlight())
                .highlight_symbol("▸ ");
            let mut st = ListState::default().with_selected(Some(app.upd_sel));
            f.render_stateful_widget(list, list_area, &mut st);
        }
    }
}

fn draw_queue(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let items: Vec<ListItem> = app
        .queue
        .items
        .iter()
        .map(|i| {
            let (mark, style) = match i.action {
                Action::Install => (app.theme.queued_install(), app.theme.accent2()),
                Action::Remove => (app.theme.queued_remove(), app.theme.bad()),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::styled(format!("{:<9}", if i.aur { "aur" } else { "repo" }), app.theme.dim()),
                Span::raw(i.name.clone()),
            ]))
        })
        .collect();
    let title = Line::from(vec![
        Span::styled(tfmt!(" Queue · {} ", app.queue.len()), app.theme.title()),
        Span::styled(t(" a applies · d drops one · c clears "), app.theme.dim()),
    ]);
    let list =
        List::new(items).block(block(app, title, true)).highlight_style(app.theme.highlight()).highlight_symbol("▸ ");
    let mut st = ListState::default().with_selected(if app.queue.is_empty() { None } else { Some(app.queue_sel) });
    f.render_stateful_widget(list, cols[0], &mut st);

    let mut lines: Vec<Line> = Vec::new();
    match &app.preflight {
        None if app.queue.is_empty() => lines.push(Line::from(Span::styled(
            t("Mark packages with space anywhere; they show up here."),
            app.theme.dim(),
        ))),
        None => lines.push(Line::from(t("Asking pacman what it would do…"))),
        Some(p) => {
            if !p.installs.is_empty() {
                lines.push(Line::from(Span::styled(
                    tfmt!("Install ({}), {} to download", p.installs.len(), human(p.download_bytes)),
                    app.theme.accent(),
                )));
                for i in &p.installs {
                    lines.push(Line::from(format!("  {i}")));
                }
            }
            if !p.removes.is_empty() {
                lines.push(Line::from(Span::styled(tfmt!("Remove ({})", p.removes.len()), app.theme.bad())));
                for r in &p.removes {
                    lines.push(Line::from(format!("  {r}")));
                }
            }
            for pr in &p.problems {
                lines.push(Line::from(Span::styled(format!("⚠ {pr}"), app.theme.warn())));
            }
            lines.push(Line::from(""));
            for s in app.queue.plan(&app.cfg) {
                lines.push(Line::from(Span::styled(format!("$ {}", s.command_line()), app.theme.dim())));
            }
        }
    }
    let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).scroll((app.detail_scroll, 0)).block(block(
        app,
        Line::from(Span::styled(t(" What will happen "), app.theme.title())),
        false,
    ));
    f.render_widget(p, cols[1]);
}

fn draw_recipes(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let recipes = app.software_recipes();
    let items: Vec<ListItem> = recipes
        .iter()
        .map(|r| {
            let (mark, style) = match app.recipe_status.get(&r.id) {
                Some(true) => (app.theme.installed(), app.theme.ok()),
                Some(false) => ("○", app.theme.dim()),
                None => ("…", app.theme.dim()),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::styled(format!("{:<12}", truncate(&r.category, 12)), app.theme.dim()),
                Span::raw(r.name.clone()),
            ]))
        })
        .collect();
    let title = Line::from(vec![
        Span::styled(t(" Recipes "), app.theme.title()),
        Span::styled(t(" install and leave it configured · Enter applies "), app.theme.dim()),
    ]);
    let list =
        List::new(items).block(block(app, title, true)).highlight_style(app.theme.highlight()).highlight_symbol("▸ ");
    let mut st = ListState::default().with_selected(if recipes.is_empty() { None } else { Some(app.recipe_sel) });
    f.render_stateful_widget(list, cols[0], &mut st);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(r) = recipes.get(app.recipe_sel) {
        lines.push(Line::from(Span::styled(r.name.clone(), app.theme.title())));
        lines.push(Line::from(r.summary.clone()));
        lines.push(Line::from(""));
        if !r.packages.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(t("packages   "), app.theme.accent()),
                Span::raw(r.packages.join(" ")),
            ]));
        }
        if !r.aur.is_empty() {
            lines.push(Line::from(vec![Span::styled(t("aur        "), app.theme.warn()), Span::raw(r.aur.join(" "))]));
        }
        if !r.services.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(t("services   "), app.theme.accent()),
                Span::raw(r.services.join(" ")),
            ]));
        }
        if !r.groups.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(t("groups     "), app.theme.accent()),
                Span::raw(r.groups.join(" ")),
            ]));
        }
        if !r.files.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(t("files      "), app.theme.accent()),
                Span::raw(r.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>().join(" ")),
            ]));
        }
        if !r.env.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(t("environment"), app.theme.accent()),
                Span::raw(format!(" {}", r.env.join(" "))),
            ]));
        }
        if !r.commands.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(t("commands   "), app.theme.accent()),
                Span::raw(r.commands.iter().map(|c| c.run.clone()).collect::<Vec<_>>().join(" ; ")),
            ]));
        }
        if let Some(n) = &r.notes {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(n.clone(), app.theme.dim())));
        }
        match app.recipe_status.get(&r.id) {
            Some(true) => lines
                .push(Line::from(Span::styled(t("Already applied. Enter applies it again (safe)."), app.theme.ok()))),
            Some(false) => lines.push(Line::from(Span::styled(t("Not applied yet."), app.theme.dim()))),
            None => {}
        }
    }
    let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(block(
        app,
        Line::from(Span::styled(t(" What it does "), app.theme.title())),
        false,
    ));
    f.render_widget(p, cols[1]);
}

fn draw_details(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(name) = app.current_name().map(String::from) else {
        let p = Paragraph::new(Line::from(Span::styled(t("Nothing selected."), app.theme.dim()))).block(block(
            app,
            Line::from(Span::styled(t(" Details "), app.theme.title())),
            false,
        ));
        f.render_widget(p, area);
        return;
    };
    let Some(pkg) = app.index.get(&name).cloned() else { return };
    let th = app.theme.clone();
    let tabs: Vec<Span> = DetailTab::ALL
        .iter()
        .map(|d| {
            if *d == app.detail_tab {
                Span::styled(format!(" {} ", d.title()), th.highlight())
            } else {
                Span::styled(format!(" {} ", d.title()), th.dim())
            }
        })
        .collect();
    let mut title =
        vec![Span::styled(format!(" {} ", pkg.name), th.title()), Span::styled(format!("{} ", pkg.version), th.dim())];
    title.extend(tabs);
    title.push(Span::styled(t(" Tab switches "), th.dim()));

    let mut lines: Vec<Line> = Vec::new();
    match app.detail_tab {
        DetailTab::Info => {
            let app_info = app.apps.get(&name);
            if let Some(a) = app_info {
                lines.push(Line::from(vec![
                    Span::styled(a.name.clone(), Style::new().add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  ·  {}", a.summary), th.dim()),
                ]));
            }
            lines.push(Line::from(pkg.description.clone()));
            lines.push(Line::from(""));
            let mut facts: Vec<(&str, String)> = Vec::new();
            facts.push((t("source"), pkg.source.label().to_string()));
            match &pkg.installed {
                Some(i) => facts.push((
                    t("installed"),
                    format!(
                        "{} · {}{}",
                        i.version,
                        if i.explicit { t("explicitly") } else { t("as a dependency") },
                        i.install_date.map(|d| format!(" · {}", crate::date(d))).unwrap_or_default()
                    ),
                )),
                None => facts.push((t("installed"), t("no").into())),
            }
            if let Some(s) = pkg.install_size {
                facts.push((
                    t("size"),
                    format!(
                        "{}{}",
                        tfmt!("{} installed", human(s)),
                        pkg.download_size.map(|d| tfmt!(", {} download", human(d))).unwrap_or_default()
                    ),
                ));
            }
            if let Some(p) = pkg.popularity {
                facts.push((
                    t("popularity"),
                    if pkg.source.is_aur() {
                        tfmt!("{} · {} votes", format!("{p:.2}"), pkg.votes.unwrap_or(0))
                    } else {
                        tfmt!("{}% of Arch systems have it", format!("{p:.1}"))
                    },
                ));
            }
            if !pkg.licenses.is_empty() {
                facts.push((t("license"), pkg.licenses.join(", ")));
            }
            if let Some(u) = &pkg.url {
                facts.push((t("url"), u.clone()));
            }
            if let Some(m) = &pkg.maintainer {
                facts.push((t("maintainer"), m.clone()));
            }
            if let Some(o) = pkg.out_of_date {
                facts.push((t("flagged"), tfmt!("OUT OF DATE since {}", crate::date(o))));
            }
            if !pkg.groups.is_empty() {
                facts.push((t("groups"), pkg.groups.join(", ")));
            }
            if let Some(a) = app_info {
                if !a.categories.is_empty() {
                    facts.push((t("categories"), a.categories.join(", ")));
                }
                if !a.screenshots.is_empty() {
                    facts.push((t("screenshots"), a.screenshots.join("  ")));
                }
            }
            if let Some(d) = app.details.get(&name)
                && let Some(b) = d.build_date
            {
                facts.push((
                    t("built"),
                    format!(
                        "{}{}",
                        crate::date(b),
                        d.packager.as_ref().map(|p| tfmt!(" by {}", p)).unwrap_or_default()
                    ),
                ));
            }
            for (k, v) in facts {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<12}", crate::i18n::t(k)), th.accent()),
                    Span::raw(v),
                ]));
            }
        }
        DetailTab::Deps => match app.details.get(&name) {
            None => lines.push(Line::from(Span::styled(t("loading…"), th.dim()))),
            Some(d) => {
                let section = |lines: &mut Vec<Line>, title: &str, items: &[String]| {
                    if items.is_empty() {
                        return;
                    }
                    lines.push(Line::from(Span::styled(title.to_string(), th.accent())));
                    let mut spans = vec![Span::raw("  ")];
                    for it in items {
                        let bare = it.split(['>', '<', '=', ':']).next().unwrap_or(it);
                        let inst = app.index.get(bare).is_some_and(|p| p.is_installed());
                        spans.push(Span::styled(it.clone(), if inst { th.ok() } else { Style::new() }));
                        spans.push(Span::raw("  "));
                    }
                    lines.push(Line::from(spans));
                };
                section(&mut lines, t("depends on"), &d.depends);
                section(&mut lines, t("optional"), &d.opt_depends);
                section(&mut lines, t("build needs"), &d.make_depends);
                section(&mut lines, t("provides"), &d.provides);
                section(&mut lines, t("conflicts"), &d.conflicts);
                section(&mut lines, t("required by"), &d.required_by);
                section(&mut lines, t("optional for"), &d.optional_for);
                if lines.is_empty() {
                    lines.push(Line::from(Span::styled(t("no dependencies"), th.dim())));
                } else {
                    lines.push(Line::from(Span::styled(t("green = installed"), th.dim())));
                }
            }
        },
        DetailTab::Files => match app.files.get(&name) {
            None => lines.push(Line::from(Span::styled(t("loading…"), th.dim()))),
            Some(files) => {
                lines.push(Line::from(Span::styled(tfmt!("{} files", files.len()), th.dim())));
                lines.extend(files.iter().map(|f| Line::from(f.clone())));
            }
        },
        DetailTab::Pkgbuild => match app.pkgbuilds.get(&name) {
            None => lines.push(Line::from(Span::styled(t("loading…"), th.dim()))),
            Some(text) => lines.extend(text.lines().map(|l| Line::from(l.to_string()))),
        },
    }
    let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).scroll((app.detail_scroll, 0)).block(block(
        app,
        Line::from(title),
        false,
    ));
    f.render_widget(p, area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let left = if let Some(b) = &app.busy {
        Span::styled(format!(" ⏳ {b}"), app.theme.warn())
    } else if !app.status.is_empty() {
        Span::styled(format!(" {}", app.status), app.theme.accent2())
    } else {
        Span::styled(t(" Made by Chidaruma · like it? star it at github.com/Chidaruma696"), app.theme.dim())
    };
    f.render_widget(Paragraph::new(Line::from(left)), area);
    if let Some(tag) = &app.new_release {
        let note = tfmt!("⬆ Sanae {} is out · 7 → Update Sanae now ", tag);
        let w = (note.chars().count() as u16).min(area.width);
        let right = Rect { x: area.x + area.width - w, width: w, ..area };
        f.render_widget(Paragraph::new(Line::from(Span::styled(note, app.theme.warn()))), right);
    }
}

/// The keys that matter on this tab, always visible.
fn draw_keys(f: &mut Frame, app: &App, area: Rect) {
    let keys: &[(&str, &str)] = match app.tab {
        Tab::Store if app.dark_shelf() && app.dark_open.is_none() => {
            &[("←→", t("shelves/apps")), ("↑↓", t("move")), ("Enter", t("open")), ("?", t("help")), ("q", t("quit"))]
        }
        Tab::Store if app.dark_open.is_some() => &[
            ("Esc", t("back")),
            ("↑↓", t("move")),
            ("space", t("mark")),
            ("A", t("queue the group")),
            ("i", t("install now")),
            ("Tab", t("details")),
            ("a", t("apply queue")),
            ("?", t("help")),
            ("q", t("quit")),
        ],
        Tab::Store => &[
            ("←→", t("shelves/apps")),
            ("↑↓", t("move")),
            ("space", t("mark")),
            ("i", t("install now")),
            ("Enter", t("open")),
            ("Tab", t("details")),
            ("a", t("apply queue")),
            ("?", t("help")),
            ("q", t("quit")),
        ],
        Tab::Search => &[
            ("/", t("type")),
            ("↑↓", t("move")),
            ("space", t("mark")),
            ("d", t("mark remove")),
            ("i", t("install now")),
            ("Tab", t("details")),
            ("a", t("apply queue")),
            ("?", t("help")),
            ("q", t("quit")),
        ],
        Tab::Installed => &[
            ("f", t("filter")),
            ("↑↓", t("move")),
            ("space/d", t("mark remove")),
            ("o", t("queue orphans")),
            ("Tab", t("details")),
            ("a", t("apply queue")),
            ("?", t("help")),
            ("q", t("quit")),
        ],
        Tab::Updates => &[
            ("u/Enter", t("update everything")),
            ("↑↓", t("move")),
            ("Tab", t("details")),
            ("r", t("reload")),
            ("?", t("help")),
            ("q", t("quit")),
        ],
        Tab::Queue => &[
            ("a/Enter", t("apply")),
            ("d", t("drop line")),
            ("c", t("clear")),
            ("←→", t("scroll")),
            ("?", t("help")),
            ("q", t("quit")),
        ],
        Tab::Recipes => &[("↑↓", t("move")), ("Enter", t("apply recipe")), ("?", t("help")), ("q", t("quit"))],
        Tab::Settings if app.sources_open => {
            &[("Esc", t("back")), ("↑↓", t("move")), ("Enter", t("enable")), ("?", t("help")), ("q", t("quit"))]
        }
        Tab::Settings => &[("↑↓", t("move")), ("Enter", t("change / open")), ("?", t("help")), ("q", t("quit"))],
    };
    let mut spans = Vec::new();
    for (k, what) in keys {
        spans.push(Span::styled(format!(" {k} "), app.theme.highlight()));
        spans.push(Span::styled(format!(" {what}  "), app.theme.dim()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_settings(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);
    let rows = app.settings_rows();
    let sources = app.source_recipes();
    let mut items: Vec<ListItem> = Vec::new();
    if app.sources_open {
        for r in &sources {
            let (mark, style) = match app.recipe_status.get(&r.id) {
                Some(true) => (app.theme.installed(), app.theme.ok()),
                Some(false) => ("○", app.theme.dim()),
                None => ("…", app.theme.dim()),
            };
            let name = source_short(&r.id);
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), style),
                Span::raw(name.to_string()),
            ])));
        }
    } else {
        for (_, label, value) in &rows {
            items.push(ListItem::new(Line::from(vec![
                Span::raw(format!("{label:<26}")),
                Span::styled(value.clone(), app.theme.accent()),
            ])));
        }
        let on = sources.iter().filter(|r| app.recipe_status.get(&r.id) == Some(&true)).count();
        let val = if on > 0 { tfmt!("{} enabled", on) } else { t("Flatpak · Chaotic-AUR · BlackArch…").into() };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(format!("{:<26}", t("Package sources ▸"))),
            Span::styled(val, app.theme.dim()),
        ])));
    }
    let header = if app.sources_open {
        t(" Package sources · Enter enables one · Esc back ")
    } else {
        t(" Enter changes a setting or opens the sources ")
    };
    let title =
        Line::from(vec![Span::styled(t(" Settings "), app.theme.title()), Span::styled(header, app.theme.dim())]);
    let list = List::new(items)
        .block(block(app, title, true))
        .highlight_style(app.theme.highlight())
        .highlight_symbol("\u{25b8} ");
    let mut st = ListState::default().with_selected(Some(app.settings_sel));
    f.render_stateful_widget(list, cols[0], &mut st);

    let mut lines: Vec<Line> = Vec::new();
    if app.sources_open {
        if let Some(r) = sources.get(app.settings_sel) {
            lines.push(Line::from(Span::styled(r.name.clone(), app.theme.title())));
            lines.push(Line::from(""));
            lines.push(Line::from(source_blurb(&r.id)));
            lines.push(Line::from(""));
            if !r.packages.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(t("packages   "), app.theme.accent()),
                    Span::raw(r.packages.join(" ")),
                ]));
            }
            if !r.aur.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(t("aur        "), app.theme.warn()),
                    Span::raw(r.aur.join(" ")),
                ]));
            }
            if let Some(n) = &r.notes {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(n.clone(), app.theme.dim())));
            }
            match app.recipe_status.get(&r.id) {
                Some(true) => lines.push(Line::from(Span::styled(t("Already enabled."), app.theme.ok()))),
                Some(false) => {
                    lines.push(Line::from(Span::styled(t("Not enabled. Enter enables it."), app.theme.dim())))
                }
                None => {}
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            t("\u{26a0} These are not reviewed by Arch Linux. A package from them can break an update or ship anything."),
            app.theme.warn(),
        )));
        lines.push(Line::from(Span::styled(
            t("  Enable only what you understand, and read PKGBUILDs before building."),
            app.theme.warn(),
        )));
    } else if app.settings_sel < rows.len() {
        let (id, label, _) = &rows[app.settings_sel];
        lines.push(Line::from(Span::styled(label.clone(), app.theme.title())));
        lines.push(Line::from(""));
        let text = match *id {
            "check_updates" => t(
                "Once every few hours Sanae asks GitHub whether a newer release exists and tells you in the status line. Nothing is downloaded until you ask.",
            ),
            "self_update" => t(
                "Downloads the latest release binary and replaces this one (needs your password). Sanae is a single file, so this is the whole update.",
            ),
            "aur_helper" => {
                t("The program that builds AUR packages for you: paru or yay. Auto picks whichever is installed.")
            }
            "privilege" => t("How Sanae becomes root to run pacman: sudo or doas. Auto picks whichever is installed."),
            "nerd_font" => t("Use Nerd Font glyphs for the marks in lists. Only if your terminal font has them."),
            "language" => t("The language of this interface. Auto follows the system language."),
            _ => "",
        };
        lines.push(Line::from(text));
    } else {
        lines.push(Line::from(Span::styled(t("Package sources"), app.theme.title())));
        lines.push(Line::from(""));
        lines.push(Line::from(t(
            "Extra places to install software from, each off by default. Enter opens the list; every one explains what it is and what it is good for before you turn it on.",
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(t("Chaotic-AUR"), app.theme.ok()),
            Span::raw(t(" is the handy one; ")),
            Span::styled(t("BlackArch"), app.theme.warn()),
            Span::raw(t(" is only for security work.")),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        t("Sanae and Reimu are made by Chidaruma. Do you like them? Visit github.com/Chidaruma696 and leave a star."),
        app.theme.dim(),
    )));
    let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(block(
        app,
        Line::from(Span::styled(t(" About "), app.theme.title())),
        false,
    ));
    f.render_widget(p, cols[1]);
}

/// A short name for a source in the list.
fn source_short(id: &str) -> &'static str {
    match id {
        "source-flatpak" => "Flatpak (Flathub)",
        "source-snap" => "Snap",
        "source-chaotic-aur" => "Chaotic-AUR",
        "source-liquorix" => "Liquorix kernel",
        "source-blackarch" => "BlackArch",
        "source-alhp" => "ALHP (v3/v4)",
        _ => "source",
    }
}

/// A plain-words explanation of what a source is and what it is good for.
fn source_blurb(id: &str) -> &'static str {
    match id {
        "source-chaotic-aur" => t(
            "The one most people want. Thousands of AUR programs already compiled by its maintainers, so browsers, editors, games and drivers install in seconds instead of building for minutes. Widely used and handy; still a third party, so a package could break an update.",
        ),
        "source-flatpak" => t(
            "Flatpak with Flathub: desktop apps that run sandboxed, apart from the system, and update on their own. Good for closed-source or GUI apps you want kept separate from pacman. Built by their publishers, not by Arch.",
        ),
        "source-snap" => t(
            "Canonical's take on the same idea, from the AUR. On Arch it has fewer apps than Flatpak and is heavier; enable it only if a program you need ships as a snap and nothing else.",
        ),
        "source-liquorix" => t(
            "A desktop-tuned kernel (linux-lqx) for smoother audio, gaming and video when the machine is busy. Nice on a workstation, but you must add it to your bootloader after installing. A third-party kernel.",
        ),
        "source-alhp" => t(
            "Arch's own packages rebuilt for newer CPUs (x86-64-v3 or v4) for a small speed gain. Helps only on a 2015-or-later CPU and is safe to skip. Community project; its packages lag Arch by a few hours.",
        ),
        "source-blackarch" => t(
            "Adds 2 800+ security and penetration-testing tools: scanners, exploits, forensics, wireless. It is meant for security work, the tools are dual-use, and the repository is large. If you are not sure you want it, you do not need it. Once enabled, the \u{2620} Darkside shelf in the Store lists these tools by group.",
        ),
        _ => "",
    }
}

fn draw_run(f: &mut Frame, app: &App, area: Rect) {
    let Some(run) = &app.run else { return };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    let step = run.steps.get(run.current);
    let head = Line::from(vec![
        Span::styled(format!(" {} ", run.title), app.theme.title()),
        Span::styled(tfmt!(" step {}/{} ", run.current + 1, run.steps.len()), app.theme.dim()),
        Span::raw(step.map(|s| s.title.clone()).unwrap_or_default()),
    ]);
    f.render_widget(Paragraph::new(head).block(block(app, Line::from(""), true)), rows[0]);
    let inner_h = usize::from(rows[1].height.saturating_sub(2));
    let inner_w = usize::from(rows[1].width.saturating_sub(2)).max(10);
    // Wrap by hand so the window really ends at the newest line (or at the scroll position).
    let wrap = |s: &str| -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        if chars.is_empty() {
            return vec![String::new()];
        }
        chars.chunks(inner_w).map(|c| c.iter().collect()).collect()
    };
    let mut rows_out: Vec<(String, bool)> =
        run.lines.iter().flat_map(|l| wrap(l).into_iter().map(|r| (r, false))).collect();
    if !run.partial.is_empty() {
        rows_out.extend(wrap(&run.partial).into_iter().map(|r| (r, true)));
    }
    let total = rows_out.len();
    let end = total.saturating_sub(run.scroll.min(total.saturating_sub(inner_h)));
    let start = end.saturating_sub(inner_h);
    let shown: Vec<Line> = rows_out[start..end]
        .iter()
        .map(
            |(r, partial)| {
                if *partial { Line::from(Span::styled(r.clone(), app.theme.accent())) } else { Line::from(r.clone()) }
            },
        )
        .collect();
    let above = if start > 0 { tfmt!(" ↑ {} more · PgUp/PgDn scroll ", start) } else { String::new() };
    let p = Paragraph::new(Text::from(shown)).block(block(
        app,
        Line::from(vec![Span::styled(t(" output "), app.theme.dim()), Span::styled(above, app.theme.dim())]),
        false,
    ));
    f.render_widget(p, rows[1]);
    let log = tfmt!(" · full log: {}", app.run_log_path().display());
    let foot = if run.finished {
        if run.failed {
            Span::styled(format!("{}{log}", t(" ✖ failed · Enter or Esc to go back · ↑↓ scroll")), app.theme.bad())
        } else if run.restart_after {
            Span::styled(
                t(" ✔ updated · Enter restarts Sanae with the new version · Esc keeps this one "),
                app.theme.ok(),
            )
        } else {
            Span::styled(format!("{}{log}", t(" ✔ done · Enter or Esc to go back · ↑↓ scroll")), app.theme.ok())
        }
    } else {
        Span::styled(
            t(" running · type here to answer prompts (sudo password) · PgUp/PgDn scroll · Ctrl+C cancels "),
            app.theme.dim(),
        )
    };
    f.render_widget(Paragraph::new(Line::from(foot)), rows[2]);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let w = area.width.min(64);
    let h = area.height.min(25);
    let popup = Rect { x: (area.width - w) / 2, y: (area.height - h) / 2, width: w, height: h };
    f.render_widget(Clear, popup);
    let k = |s: &str| Span::styled(format!("{s:<10}"), app.theme.key());
    let lines = vec![
        Line::from(vec![
            k("1-7"),
            Span::raw(t("tabs: Store · Search · Installed · Updates · Queue · Recipes · Settings")),
        ]),
        Line::from(vec![k("/"), Span::raw(t("search (Esc or Enter leaves the input)"))]),
        Line::from(vec![k("↑↓ j k"), Span::raw(t("move · PgUp/PgDn · g/G first/last"))]),
        Line::from(vec![k("← →"), Span::raw(t("Store: shelves / apps · elsewhere: scroll details"))]),
        Line::from(vec![k("space"), Span::raw(t("mark for install (or remove, if installed)"))]),
        Line::from(vec![k("d"), Span::raw(t("mark for removal · in the queue: drop the line"))]),
        Line::from(vec![k("i"), Span::raw(t("install the selected package right now"))]),
        Line::from(vec![k("a"), Span::raw(t("apply the queue"))]),
        Line::from(vec![k("u"), Span::raw(t("update everything (repos, then AUR)"))]),
        Line::from(vec![k("Tab"), Span::raw(t("details: Info · Dependencies · Files · PKGBUILD"))]),
        Line::from(vec![k("f / o"), Span::raw(t("Installed: filter · queue all orphans"))]),
        Line::from(vec![k("r"), Span::raw(t("reload the package databases"))]),
        Line::from(vec![k("q"), Span::raw(t("quit"))]),
        Line::from(vec![
            k("7"),
            Span::raw(t(
                "Settings: preferences, and Package sources ▸ (Flatpak, Chaotic-AUR, BlackArch…) each explained",
            )),
        ]),
        Line::from(vec![
            k("☠ A"),
            Span::raw(t(
                "Darkside: with the BlackArch repository enabled, a ☠ shelf in the Store lists its tools by group · Enter opens a group, A queues it",
            )),
        ]),
        Line::from(""),
        Line::from(Span::styled(t("Made by Chidaruma · github.com/Chidaruma696"), app.theme.accent())),
        Line::from(Span::styled(
            t("While a command runs, keys go to it (sudo asks there). Ctrl+C cancels."),
            app.theme.dim(),
        )),
        Line::from(Span::styled(
            tfmt!("{} packages known · config: {}", app.index.len(), app.cfg_path()),
            app.theme.dim(),
        )),
    ];
    let p = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(block(
        app,
        Line::from(Span::styled(t(" Keys · any key closes "), app.theme.title())),
        true,
    ));
    f.render_widget(p, popup);
}

impl App {
    fn cfg_path(&self) -> String {
        crate::config::Config::path().map(|p| p.display().to_string()).unwrap_or_default()
    }
}
