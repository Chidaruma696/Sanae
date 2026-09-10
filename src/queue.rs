//! The queue of changes and the commands that apply it.

use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Install,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    pub name: String,
    pub action: Action,
    pub aur: bool,
}

/// One command to run, with a title for the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
}

impl Step {
    pub fn new(title: impl Into<String>, program: impl Into<String>, args: &[&str]) -> Self {
        Self { title: title.into(), program: program.into(), args: args.iter().map(|s| s.to_string()).collect() }
    }

    pub fn command_line(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

#[derive(Clone, Debug, Default)]
pub struct Queue {
    pub items: Vec<QueueItem>,
}

impl Queue {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.name == name)
    }

    /// Mark for install/remove, or unmark if already marked with the same action.
    pub fn toggle(&mut self, name: &str, action: Action, aur: bool) {
        if let Some(pos) = self.items.iter().position(|i| i.name == name) {
            let same = self.items[pos].action == action;
            self.items.remove(pos);
            if same {
                return;
            }
        }
        self.items.push(QueueItem { name: name.to_string(), action, aur });
    }

    pub fn remove(&mut self, name: &str) {
        self.items.retain(|i| i.name != name);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    fn names(&self, action: Action, aur: bool) -> Vec<String> {
        self.items.iter().filter(|i| i.action == action && i.aur == aur).map(|i| i.name.clone()).collect()
    }

    /// The commands that apply this queue, in order: removals, repo installs, AUR installs.
    pub fn plan(&self, cfg: &Config) -> Vec<Step> {
        let sudo = cfg.privilege();
        let mut steps = Vec::new();
        let mut removes = self.names(Action::Remove, false);
        removes.extend(self.names(Action::Remove, true));
        if !removes.is_empty() {
            let mut args = vec!["pacman".to_string(), "-Rs".into(), "--noconfirm".into()];
            args.extend(removes.iter().cloned());
            steps.push(Step { title: format!("Removing {}", removes.join(" ")), program: sudo.clone(), args });
        }
        let repo = self.names(Action::Install, false);
        if !repo.is_empty() {
            let mut args = vec!["pacman".to_string(), "-S".into(), "--needed".into(), "--noconfirm".into()];
            args.extend(repo.iter().cloned());
            steps.push(Step { title: format!("Installing {}", repo.join(" ")), program: sudo.clone(), args });
        }
        let aur = self.names(Action::Install, true);
        if !aur.is_empty() {
            match cfg.aur_helper() {
                Some(helper) => {
                    let mut args = vec!["-S".to_string(), "--needed".into(), "--noconfirm".into()];
                    if helper == "paru" {
                        args.push("--skipreview".into());
                    }
                    args.extend(aur.iter().cloned());
                    steps.push(Step {
                        title: format!("Building from the AUR: {}", aur.join(" ")),
                        program: helper,
                        args,
                    });
                }
                None => {
                    steps.push(Step::new(format!("No AUR helper (paru or yay) for: {}", aur.join(" ")), "false", &[]))
                }
            }
        }
        steps
    }

    /// The commands for a full system update: repositories, then the AUR.
    pub fn update_plan(cfg: &Config) -> Vec<Step> {
        let sudo = cfg.privilege();
        let mut steps = vec![Step::new("Updating the repositories", sudo, &["pacman", "-Syu", "--noconfirm"])];
        if let Some(helper) = cfg.aur_helper() {
            let mut args = vec!["-Sua", "--noconfirm"];
            if helper == "paru" {
                args.push("--skipreview");
            }
            steps.push(Step::new("Updating AUR packages", helper, &args));
        }
        steps
    }

    /// What pacman would do, without doing it: downloads and their sizes, removals.
    /// Needs no root. AUR packages are listed as-is (their dependencies are only known at build time).
    pub fn preflight(&self) -> Preflight {
        let mut pf = Preflight::default();
        let repo = self.names(Action::Install, false);
        if !repo.is_empty() {
            let out = Command::new("pacman")
                .args(["-S", "--print", "--print-format", "%r/%n %v %s", "--needed"])
                .args(&repo)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    for line in String::from_utf8_lossy(&o.stdout).lines() {
                        let mut it = line.split_whitespace();
                        let (Some(name), Some(ver), Some(size)) = (it.next(), it.next(), it.next()) else { continue };
                        let bytes: u64 = size.parse().unwrap_or(0);
                        pf.download_bytes += bytes;
                        pf.installs.push(format!("{name} {ver}"));
                    }
                }
                Ok(o) => pf.problems.push(String::from_utf8_lossy(&o.stderr).trim().to_string()),
                Err(e) => pf.problems.push(e.to_string()),
            }
        }
        for n in self.names(Action::Install, true) {
            pf.installs.push(format!("aur/{n} (built locally)"));
        }
        let mut removes = self.names(Action::Remove, false);
        removes.extend(self.names(Action::Remove, true));
        if !removes.is_empty() {
            let out =
                Command::new("pacman").args(["-Rs", "--print", "--print-format", "%n %v"]).args(&removes).output();
            match out {
                Ok(o) if o.status.success() => {
                    pf.removes.extend(String::from_utf8_lossy(&o.stdout).lines().map(String::from));
                }
                _ => pf.removes.extend(removes),
            }
        }
        pf
    }
}

#[derive(Clone, Debug, Default)]
pub struct Preflight {
    pub installs: Vec<String>,
    pub removes: Vec<String>,
    pub download_bytes: u64,
    pub problems: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_marks_and_unmarks() {
        let mut q = Queue::default();
        q.toggle("vim", Action::Install, false);
        assert_eq!(q.len(), 1);
        q.toggle("vim", Action::Install, false);
        assert!(q.is_empty(), "same action again unmarks");
        q.toggle("vim", Action::Install, false);
        q.toggle("vim", Action::Remove, false);
        assert_eq!(q.get("vim").unwrap().action, Action::Remove, "other action replaces");
    }

    #[test]
    fn plan_orders_removals_repo_then_aur() {
        let mut q = Queue::default();
        q.toggle("spotify", Action::Install, true);
        q.toggle("vim", Action::Install, false);
        q.toggle("nano", Action::Remove, false);
        let mut cfg = Config::default();
        cfg.general.privilege = "sudo".into();
        cfg.general.aur_helper = "paru".into();
        let steps = q.plan(&cfg);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].command_line(), "sudo pacman -Rs --noconfirm nano");
        assert_eq!(steps[1].command_line(), "sudo pacman -S --needed --noconfirm vim");
        assert_eq!(steps[2].command_line(), "paru -S --needed --noconfirm --skipreview spotify");
    }
}
