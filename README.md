<div align="center">
  <br/>

# Sanae

**早苗 · A software store for Arch Linux that lives in the terminal: browse, install, configure.**

<br/>

![Arch Linux](https://img.shields.io/badge/arch%20linux-pacman%20%2B%20AUR-1793d1?style=for-the-badge&logo=archlinux&logoColor=white)
![Rust](https://img.shields.io/badge/rust-2024-b7410e?style=for-the-badge&logo=rust&logoColor=white)
![Dependencies at runtime](https://img.shields.io/badge/runtime%20deps-pacman%20expac-2b2140?style=for-the-badge)
![License MIT](https://img.shields.io/badge/license-MIT-1b150d?style=for-the-badge)

<br/>

*One static binary · never links libalpm · repos and AUR in one list · recipes that leave things configured*

</div>

---

> [!NOTE]
> Sanae is being built in milestones. This is **milestone 1: the data layer and the command line**. The store interface (milestone 2 and 3) and the recipes (milestone 4) come next. The design is in [`DESIGN.md`](DESIGN.md).

<br/>

## 🗺️ What it is

Sanae is what pamac or Octopi are for the desktop, but for the terminal: a place to browse software by what it is rather than by package name, see what a package brings before installing it, keep the system updated with the Arch news in front of you, and, the part no package manager does, **leave things configured**: fonts with fontconfig, QEMU with libvirt and your user in the right group, Docker with the service enabled. Those are *recipes*, small text files Sanae applies for you, also inside a fresh installation made by [Reimu](https://github.com/Chidaruma696/Reimu).

Three rules shape it:

- **KISS.** One binary, no daemon, no database of its own. It reads what pacman already keeps on disk and asks the AUR over HTTP.
- **YAGNI.** No Flatpak, no scanners, no tray icon until someone needs them.
- **Unix.** Sanae does not reimplement pacman or an AUR helper: it runs them. Every subcommand works from a script and speaks JSON with `--json`.

<br/>

## 🚀 Use it (milestone 1)

Needs `expac` and `pacman-contrib` (for `checkupdates`), both in the official repositories. An AUR helper (paru or yay) is only needed once installing lands.

```sh
sanae search firefox            # repositories and the AUR, fuzzy, installed ones marked
sanae search --aur --limit 10 tui
sanae info paru                 # everything about one package, from the repos or the AUR
sanae installed --explicit      # what you installed on purpose
sanae installed --orphans       # dependencies nothing needs any more
sanae installed --foreign       # from the AUR or built locally
sanae updates                   # repository updates via checkupdates, AUR ones via the RPC
sanae owner /usr/bin/vim        # which package brings a file
sanae clean                     # drop Sanae's cache (~/.cache/sanae)
```

Add `--json` to any of them for machine-readable output.

<br/>

## 🔧 How it works

```
src/
├── main.rs            command line (clap); the interface arrives in milestone 2
├── model.rs           Package, Installed, Details, Update: the one shape every source maps into
├── index.rs           in-memory index of every package, fuzzy search with nucleo
├── cache.rs           small file cache under ~/.cache/sanae, safe to delete
└── sources/
    ├── pacman.rs      expac -S / -Q dumps, pacman -Ql / -Fl, -Qdt, -Qm, checkupdates, vercmp
    └── aur.rs         AUR RPC v5: search (name, name-desc, keywords…), info in batches, PKGBUILD
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

1. ~~Data layer and CLI~~
2. Interface: search, details, installed, queue, execution with sudo inside the TUI
3. Store: AppStream categories and human names, popularity from pkgstats, updates with the Arch news
4. Recipes and `sanae apply --chroot`
5. Integration with Reimu
6. Polish, theme file, `sanae-bin` on the AUR

<br/>

## ⚖️ License

MIT. Sanae is not affiliated with Arch Linux. The name comes from Sanae Kochiya of Touhou Project, the shrine maiden who works miracles.

<div align="center">
  <br/>

早苗 · Miracles on demand.

</div>
