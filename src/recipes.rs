//! Recipes: install packages and leave the thing configured. Small TOML files,
//! idempotent, applicable to the running system or to a mounted installation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::queue::Step;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Recipe {
    /// File stem; set on load.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub summary: String,
    pub category: String,
    /// Packages from the repositories.
    pub packages: Vec<String>,
    /// Packages from the AUR (need paru or yay).
    pub aur: Vec<String>,
    /// systemd units to enable.
    pub services: Vec<String>,
    /// Groups the user is added to.
    pub groups: Vec<String>,
    pub files: Vec<RecipeFile>,
    pub commands: Vec<RecipeCommand>,
    /// Lines for /etc/environment.
    pub env: Vec<String>,
    /// Other recipes this one needs first.
    pub needs: Vec<String>,
    /// A shell command that exits 0 when the recipe is already applied.
    pub check: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeFile {
    /// `~` means the user's home.
    pub path: String,
    /// Replace the file with this.
    pub content: Option<String>,
    /// Add this line if it is not there.
    pub append: Option<String>,
    /// Octal, e.g. "0644".
    pub mode: Option<String>,
    /// Owned by the user rather than root.
    pub user: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecipeCommand {
    pub run: String,
    /// "root" (default) or "user".
    #[serde(rename = "as")]
    pub as_who: String,
}

/// How and where to apply.
#[derive(Clone, Debug)]
pub struct ApplyOptions {
    /// A mounted installation (Reimu's /mnt); commands run through arch-chroot.
    pub chroot: Option<PathBuf>,
    /// The user that gets groups, home files and AUR builds.
    pub user: String,
    /// sudo or doas (ignored with chroot: we are root there).
    pub privilege: String,
    pub aur_helper: Option<String>,
}

/// Recipes shipped inside the binary.
const BUILTIN: &[(&str, &str)] = &[
    ("fonts", include_str!("../recipes/fonts.toml")),
    ("japanese", include_str!("../recipes/japanese.toml")),
    ("qemu-kvm", include_str!("../recipes/qemu-kvm.toml")),
    ("docker", include_str!("../recipes/docker.toml")),
    ("gaming", include_str!("../recipes/gaming.toml")),
    ("development", include_str!("../recipes/development.toml")),
    ("office", include_str!("../recipes/office.toml")),
    ("multimedia", include_str!("../recipes/multimedia.toml")),
    ("graphics", include_str!("../recipes/graphics.toml")),
    ("internet", include_str!("../recipes/internet.toml")),
    ("utilities", include_str!("../recipes/utilities.toml")),
    ("virtualbox", include_str!("../recipes/virtualbox.toml")),
    ("printing", include_str!("../recipes/printing.toml")),
    ("bluetooth", include_str!("../recipes/bluetooth.toml")),
    ("theme-xfce-arc", include_str!("../recipes/theme-xfce-arc.toml")),
    ("theme-xfce-greybird", include_str!("../recipes/theme-xfce-greybird.toml")),
    ("theme-xfce-materia", include_str!("../recipes/theme-xfce-materia.toml")),
    ("theme-xfce-catppuccin", include_str!("../recipes/theme-xfce-catppuccin.toml")),
];

/// Directories with extra or overriding recipes, in priority order (last wins).
fn user_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/share/sanae/recipes"), PathBuf::from("/etc/sanae/recipes")];
    if let Some(d) = directories::ProjectDirs::from("", "", "sanae") {
        dirs.push(d.config_dir().join("recipes"));
    }
    dirs
}

pub fn parse(id: &str, text: &str) -> Result<Recipe> {
    let mut r: Recipe = toml::from_str(text).with_context(|| format!("recipe {id}"))?;
    r.id = id.to_string();
    if r.name.is_empty() {
        r.name = id.to_string();
    }
    Ok(r)
}

/// Built-in recipes plus any found on disk, by id.
pub fn load_all() -> Vec<Recipe> {
    let mut map: BTreeMap<String, Recipe> = BTreeMap::new();
    for (id, text) in BUILTIN {
        if let Ok(r) = parse(id, text) {
            map.insert(r.id.clone(), r);
        }
    }
    for dir in user_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "toml")
                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                && let Ok(text) = std::fs::read_to_string(&p)
            {
                match parse(stem, &text) {
                    Ok(r) => {
                        map.insert(r.id.clone(), r);
                    }
                    Err(e) => eprintln!("warning: {e:#}"),
                }
            }
        }
    }
    map.into_values().collect()
}

pub fn find<'a>(all: &'a [Recipe], id: &str) -> Option<&'a Recipe> {
    all.iter().find(|r| r.id == id)
}

/// Single-quote for `bash -c`.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl Recipe {
    /// `~` expanded for the target user.
    fn home(&self, opts: &ApplyOptions) -> String {
        if opts.user == "root" { "/root".to_string() } else { format!("/home/{}", opts.user) }
    }

    /// The commands that apply this recipe, in order. Every step is safe to repeat.
    pub fn plan(&self, opts: &ApplyOptions) -> Vec<Step> {
        let mut steps = Vec::new();
        let root = |title: String, script: String| -> Step {
            match &opts.chroot {
                Some(dir) => Step {
                    title,
                    program: "arch-chroot".into(),
                    args: vec![dir.display().to_string(), "bash".into(), "-c".into(), script],
                },
                None => Step { title, program: opts.privilege.clone(), args: vec!["bash".into(), "-c".into(), script] },
            }
        };
        let user = |title: String, script: String| -> Step {
            match &opts.chroot {
                Some(dir) => Step {
                    title,
                    program: "arch-chroot".into(),
                    args: vec![
                        dir.display().to_string(),
                        "sudo".into(),
                        "-u".into(),
                        opts.user.clone(),
                        "-H".into(),
                        "bash".into(),
                        "-c".into(),
                        script,
                    ],
                },
                None => Step { title, program: "bash".into(), args: vec!["-c".into(), script] },
            }
        };

        if !self.packages.is_empty() {
            steps.push(root(
                format!("Installing {}", self.packages.join(" ")),
                format!("pacman -S --needed --noconfirm {}", self.packages.join(" ")),
            ));
        }
        if !self.aur.is_empty() {
            match &opts.aur_helper {
                Some(h) => {
                    let skip = if h == "paru" { " --skipreview" } else { "" };
                    steps.push(user(
                        format!("Building from the AUR: {}", self.aur.join(" ")),
                        format!("{h} -S --needed --noconfirm{skip} {}", self.aur.join(" ")),
                    ));
                }
                None => steps.push(Step::new(format!("No AUR helper for: {}", self.aur.join(" ")), "false", &[])),
            }
        }
        for g in &self.groups {
            steps.push(root(
                format!("Adding {} to group {g}", opts.user),
                format!("usermod -aG {g} {}", sq(&opts.user)),
            ));
        }
        let home = self.home(opts);
        for f in &self.files {
            let path = f.path.replacen('~', &home, 1);
            let mut script = format!("mkdir -p {}", sq(&parent(&path)));
            if let Some(c) = &f.content {
                script.push_str(&format!(" && printf '%s' {} > {}", sq(c), sq(&path)));
            }
            if let Some(a) = &f.append {
                script.push_str(&format!(
                    " && (grep -qxF {} {} 2>/dev/null || printf '%s\\n' {} >> {})",
                    sq(a),
                    sq(&path),
                    sq(a),
                    sq(&path)
                ));
            }
            if let Some(m) = &f.mode {
                script.push_str(&format!(" && chmod {} {}", sq(m), sq(&path)));
            }
            if f.user {
                script.push_str(&format!(" && chown -R {u}:{u} {p}", u = sq(&opts.user), p = sq(&parent(&path))));
            }
            steps.push(root(format!("Writing {path}"), script));
        }
        for e in &self.env {
            steps.push(root(
                format!("Environment: {e}"),
                format!("grep -qxF {} /etc/environment || printf '%s\\n' {} >> /etc/environment", sq(e), sq(e)),
            ));
        }
        for s in &self.services {
            steps.push(root(format!("Enabling {s}"), format!("systemctl enable {}", sq(s))));
        }
        for c in &self.commands {
            let title = format!("Running: {}", crate::truncate(&c.run, 60));
            if c.as_who == "user" {
                steps.push(user(title, c.run.clone()))
            } else {
                steps.push(root(title, c.run.clone()))
            }
        }
        steps
    }

    /// Is it already applied? Uses `check` when there is one, else "all its packages are installed".
    pub fn is_applied(&self, installed: &dyn Fn(&str) -> bool) -> bool {
        if let Some(check) = &self.check {
            return std::process::Command::new("bash")
                .args(["-c", check])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
        }
        let all: Vec<&String> = self.packages.iter().chain(self.aur.iter()).collect();
        !all.is_empty() && all.iter().all(|p| installed(p))
    }
}

fn parent(path: &str) -> String {
    Path::new(path).parent().map(|p| p.display().to_string()).filter(|p| !p.is_empty()).unwrap_or_else(|| "/".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_recipes_all_parse() {
        let all = load_all();
        assert!(all.len() >= BUILTIN.len());
        for r in &all {
            assert!(!r.name.is_empty(), "{} has no name", r.id);
            assert!(!r.summary.is_empty(), "{} has no summary", r.id);
        }
        assert!(find(&all, "qemu-kvm").is_some());
    }

    #[test]
    fn plan_uses_chroot_and_quotes_content() {
        let r = parse(
            "t",
            r#"
name = "Test"
summary = "s"
packages = ["a", "b"]
services = ["x.service"]
groups = ["wheel"]
[[files]]
path = "~/.config/it's.conf"
content = "hello 'world'"
user = true
[[commands]]
run = "echo done"
as = "user"
"#,
        )
        .unwrap();
        let opts = ApplyOptions {
            chroot: Some(PathBuf::from("/mnt")),
            user: "jp".into(),
            privilege: "sudo".into(),
            aur_helper: None,
        };
        let steps = r.plan(&opts);
        assert_eq!(steps[0].program, "arch-chroot");
        assert_eq!(steps[0].args[0], "/mnt");
        assert!(steps[0].args[3].contains("pacman -S --needed --noconfirm a b"));
        let write = steps.iter().find(|s| s.title.starts_with("Writing")).unwrap();
        assert!(write.args[3].contains("/home/jp/.config/"), "{}", write.args[3]);
        assert!(write.args[3].contains(r#"'hello '\''world'\'''"#), "{}", write.args[3]);
        assert!(write.args[3].contains("chown -R 'jp':'jp'"));
        let last = steps.last().unwrap();
        assert!(last.args.contains(&"-u".to_string()) && last.args.contains(&"jp".to_string()));
        let steps_local =
            r.plan(&ApplyOptions { chroot: None, user: "jp".into(), privilege: "doas".into(), aur_helper: None });
        assert_eq!(steps_local[0].program, "doas");
    }
}
