<div align="center">
  <br/>

# Sanae

**早苗 · A software store for Arch Linux that lives in the terminal: browse, install, configure.**

<br/>

![Arch Linux](https://img.shields.io/badge/arch%20linux-pacman%20%2B%20AUR-1793d1?style=for-the-badge&logo=archlinux&logoColor=white)
![Rust](https://img.shields.io/badge/rust-2024-b7410e?style=for-the-badge&logo=rust&logoColor=white)
![License MIT](https://img.shields.io/badge/license-MIT-1b150d?style=for-the-badge)

<br/>

*One static binary · never links libalpm · repos and AUR in one list · recipes that leave things configured · eight interface languages*

</div>

---

> [!IMPORTANT]
> Sanae runs pacman and your AUR helper for you and writes the files its recipes say. Read what the queue and the recipes will do before applying them. The design is in [`DESIGN.md`](DESIGN.md).

<br/>

## 🗺️ What it is

Sanae is what pamac or Octopi are for the desktop, but for the terminal: a place to browse software by what it is rather than by package name, see what a package brings before installing it, keep the system updated with the Arch news in front of you, and, the part no package manager does, **leave things configured**: fonts with fontconfig, QEMU with libvirt and your user in the right group, Docker with the service enabled. Those are *recipes*, small text files Sanae applies for you, also inside a fresh installation made by [Reimu](https://github.com/Chidaruma696/Reimu).

Three rules shape it:

- **KISS.** One binary, no daemon, no database of its own. It reads what pacman already keeps on disk and asks the AUR over HTTP.
- **YAGNI.** No Flatpak, no scanners, no tray icon until someone needs them.
- **Unix.** Sanae does not reimplement pacman or an AUR helper: it runs them. Every subcommand works from a script and speaks JSON with `--json`.

<br/>

## 🚀 Use it

Needs `expac` and `pacman-contrib` (for `checkupdates`); `archlinux-appstream-data` fills the store shelves; paru or yay handle the AUR. All in the official repositories:

```sh
sudo pacman -S --needed expac pacman-contrib archlinux-appstream-data
curl -fsSL https://github.com/Chidaruma696/Sanae/releases/latest/download/sanae-x86_64-linux -o sanae
chmod +x sanae && sudo mv sanae /usr/local/bin/
sanae
```

[Reimu](https://github.com/Chidaruma696/Reimu) offers to do exactly this at the end of an installation.

```
 早苗 Sanae   1 Store · 2 Search · 3 Installed · 4 Updates (7) · 5 Queue (2) · 6 Recipes · 7 Settings   ? help  q quit
╭ Shelves ───────────╮╭ Internet · 148 apps · by popularity ─────────────────────────────────────────────╮
│ ★ Featured         ││ ✔ extra     Firefox  (firefox)                69.2%  Web Browser                  │
│▸🌐 Internet        ││   extra     Chromium  (chromium)              31.0%  Web browser                  │
│ 🎵 Multimedia      ││ + extra     qBittorrent  (qbittorrent)        18.3%  BitTorrent client            │
│ 🎨 Graphics        ││   extra     Telegram Desktop  (telegram-de…)  15.1%  Messaging                    │
╰────────────────────╯╰──────────────────────────────────────────────────────────────────────────────────╯
╭ firefox 143.0-1  Info  Dependencies  Files  PKGBUILD  Tab switches ───────────────────────────────────╮
│ Firefox  ·  Web Browser                                                                                │
│ source      extra        installed   143.0-1 · explicitly · 2026-08-30                                 │
│ size        262.1 MiB installed, 70.0 MiB download      popularity  69.2% of Arch systems have it      │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 Made by Chidaruma · like it? star it at github.com/Chidaruma696
 ←→ shelves/apps  ↑↓ move  space mark  i install now  Enter open  Tab details  a apply queue  ? help  q quit
```

**Tabs.** *Store*: shelves from AppStream (Internet, Multimedia, Graphics, Office, Development, Games, Education, System, Utilities) with human names, summaries and how many Arch systems have each app, plus *Featured*: the most installed apps you do not have. *Search*: repositories and the AUR in one list as you type. *Installed*: all, explicit, dependencies, orphans, or AUR/local, with one key to queue every orphan. *Updates*: repositories and AUR, with the Arch news above them. *Queue*: what you marked, what pacman would download and remove, the exact commands. *Recipes*: install and configure in one go. *Settings*: interface language (English, Spanish, German, French, Italian, Portuguese, Japanese, Russian, or whatever `LANG` says), check for a newer Sanae at start and update it in place, AUR helper, sudo/doas, Nerd Font marks, and the extra sources: Flatpak (Flathub), Snap, Chaotic-AUR, the Liquorix kernel repository, BlackArch and ALHP (x86-64-v3/v4 builds), each with what it does and a warning, because nothing outside the official repositories is reviewed by Arch. With BlackArch enabled, the Store grows one *☠ Darkside* shelf: its 2 800+ tools by BlackArch group (scanner, webapp, exploitation, forensic, wireless, cracker…), Enter opens a group, A queues it whole, and a *Purple team* group gathers detection, forensics and hardening tools from any repository.

**Keys.** `1-7` tabs · `/` search · `space` mark · `d` mark for removal · `i` install now · `a` apply the queue · `u` update everything · `Tab` Info / Dependencies / Files / PKGBUILD · `?` everything else. Commands run inside Sanae in a pseudo-terminal: sudo asks for your password right there, the output streams live, Ctrl+C cancels.

### Command line

```sh
sanae search firefox            # repositories and the AUR, fuzzy, installed ones marked
sanae search --aur --limit 10 tui
sanae info paru                 # everything about one package, from the repos or the AUR
sanae installed --explicit      # what you installed on purpose
sanae installed --orphans       # dependencies nothing needs any more
sanae installed --foreign       # from the AUR or built locally
sanae updates                   # repository updates via checkupdates, AUR ones via the RPC
sanae owner /usr/bin/vim        # which package brings a file
sanae install firefox spotify   # repos and AUR sorted out, with what pacman will download shown first
sanae remove nano
sanae update                    # pacman -Syu, then the AUR helper
sanae recipes                   # the recipes and whether each is applied
sanae apply docker fonts        # install and configure
sanae apply --chroot /mnt --user jp qemu-kvm --dry-run   # inside a fresh installation, printing the commands
sanae apply source-flatpak      # the sources from Settings are recipes too
sanae self-update               # replace this binary with the latest release
sanae clean                     # drop Sanae's cache (~/.cache/sanae)
```

Add `--json` to `search`, `info`, `installed`, `updates` and `recipes` for machine-readable output.

### Recipes

A recipe is a small TOML file: packages, AUR packages, services to enable, groups to join, files to write, lines for `/etc/environment`, commands, and a `check` that says whether it is already applied. Every step is safe to repeat. Sanae ships with: fonts, japanese, qemu-kvm, docker, virtualbox, gaming, development, office, multimedia, graphics, internet, utilities, printing, bluetooth, XFCE themes (Arc, Greybird, Materia, Catppuccin), and the sources shown in Settings (source-flatpak, source-snap, source-chaotic-aur, source-liquorix, source-blackarch, source-alhp). Drop your own in `~/.config/sanae/recipes/` or `/etc/sanae/recipes/`.

```toml
name = "Docker"
summary = "Docker engine with Compose and Buildx, the service enabled and your user in the docker group"
packages = ["docker", "docker-compose", "docker-buildx"]
services = ["docker.service"]
groups = ["docker"]
check = "systemctl is-enabled docker.service >/dev/null 2>&1 && id -nG | grep -qw docker"
notes = "Log out and back in so the docker group applies."
```

### Configuration

`~/.config/sanae/config.toml`, every key optional:

```toml
[general]
aur_helper = "auto"   # paru · yay · auto
privilege = "auto"    # sudo · doas · auto
check_updates = true  # ask GitHub for a newer Sanae at start
language = "auto"     # en · es · de · fr · it · pt · ja · ru · auto (from LANG)

[theme]
accent = "#5fd7a7"    # Moriya green
accent2 = "#87afff"   # lake blue
nerd_font = false     # Nerd Font glyphs for the marks
```

<br/>

## 🔧 How it works

```
src/
├── main.rs            command line (clap) and the entry into the interface
├── model.rs           Package, Installed, Details, Update: the one shape every source maps into
├── index.rs           in-memory index of every package, fuzzy search with nucleo
├── cache.rs           small file cache under ~/.cache/sanae, safe to delete
├── config.rs          ~/.config/sanae/config.toml
├── queue.rs           the queue, its preflight (pacman --print) and the commands that apply it
├── exec.rs            runs commands in a pty, streams lines, forwards keystrokes (sudo)
├── recipes.rs         TOML recipes, built-in ones embedded, --chroot aware plans
├── selfupdate.rs      newer release? and the steps that replace the binary
├── i18n.rs            t() and tf(): English in the code, i18n/<code>.txt for the rest
├── recipes/           the recipes shipped in the binary
├── sources/
│   ├── pacman.rs      expac -S / -Q dumps, pacman -Ql / -Fl, -Qdt, -Qm, checkupdates, vercmp
│   ├── aur.rs         AUR RPC v5: search (name, name-desc, keywords…), info in batches, PKGBUILD
│   ├── appstream.rs   /usr/share/swcatalog: names, summaries, shelves, screenshots
│   ├── pkgstats.rs    popularity from pkgstats.archlinux.de
│   └── news.rs        the Arch news feed
└── ui/
    ├── mod.rs         state, keys, background work
    ├── draw.rs        rendering with ratatui
    └── theme.rs       colors and marks
```

Why no libalpm: pacman 7.1 ships `libalpm.so=16` while the Rust bindings target 15. Linking would mean rebuilding Sanae at every pacman release; `expac` and `pacman` have had the same interface for a decade. Reading the sync databases through `expac` takes well under a second for the ~15 000 official packages.

<br/>

## 🧪 Testing

```sh
cargo test              # parsing with recorded expac output, index ranking, AUR merging
cargo clippy --all-targets -- -D warnings
```

CI runs format, clippy, tests and builds a static `x86_64-unknown-linux-musl` binary on every push; tags starting with `v` publish it as a release.

<br/>

## 🗺️ Roadmap

- `sanae-bin` on the AUR.
- More languages: add `i18n/<code>.txt` (one `English<TAB>Translation` per line) and register it in `i18n.rs`. The Japanese and Russian tables were written with care but not by native speakers: corrections are welcome.
- Screenshots in the terminal for terminals that can show images.
- More recipes.
- Flatpak, if anyone asks.

<br/>

## ⚖️ License

MIT. Sanae is not affiliated with Arch Linux. The name comes from Sanae Kochiya of Touhou Project, the shrine maiden who works miracles. Touhou Project and its characters belong to Team Shanghai Alice (ZUN); this is unofficial fan work made under their guidelines for derivative works, with no affiliation or endorsement.

<div align="center">
  <br/>

早苗 · Miracles on demand.

</div>
