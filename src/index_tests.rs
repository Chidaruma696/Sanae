use crate::index::Index;
use crate::model::{Installed, Package, Source};
use crate::sources::pacman::LocalPackage;

fn pkg(name: &str, desc: &str) -> Package {
    Package {
        name: name.into(),
        version: "1.0-1".into(),
        source: Source::Repo("extra".into()),
        description: desc.into(),
        groups: vec![],
        licenses: vec![],
        url: None,
        install_size: None,
        download_size: None,
        votes: None,
        popularity: None,
        out_of_date: None,
        maintainer: None,
        last_modified: None,
        installed: None,
    }
}

fn local(name: &str, explicit: bool) -> LocalPackage {
    LocalPackage {
        name: name.into(),
        description: format!("{name} from the local db"),
        url: None,
        licenses: vec![],
        groups: vec![],
        installed: Installed { version: "1.0-1".into(), explicit, install_date: None, install_size: None },
    }
}

#[test]
fn build_merges_installed_and_keeps_foreign_packages() {
    let index = Index::build(
        vec![pkg("firefox", "Web browser"), pkg("vim", "Editor")],
        vec![local("vim", true), local("paru", true)],
    );
    assert_eq!(index.len(), 3);
    assert!(index.get("vim").unwrap().is_installed());
    assert!(!index.get("firefox").unwrap().is_installed());
    let paru = index.get("paru").unwrap();
    assert_eq!(paru.source, Source::Local);
    assert!(paru.is_installed());
}

#[test]
fn search_ranks_exact_then_prefix_then_description() {
    let index = Index::build(
        vec![
            pkg("vim", "Vi Improved, a highly configurable text editor"),
            pkg("neovim", "Fork of Vim aiming to improve user experience"),
            pkg("gvim", "Vi Improved with GTK"),
            pkg("emacs", "The extensible editor, nothing to do with the other one"),
        ],
        vec![],
    );
    let names: Vec<&str> = index.search("vim", 10).into_iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names[0], "vim");
    assert!(names.contains(&"neovim") && names.contains(&"gvim"));
    assert!(!names.contains(&"emacs"));
    assert!(index.search("", 10).is_empty());
}

#[test]
fn merge_aur_upgrades_local_packages_but_not_repo_ones() {
    let mut index = Index::build(vec![pkg("firefox", "Web browser")], vec![local("paru", true)]);
    let mut aur_paru = pkg("paru", "AUR helper");
    aur_paru.source = Source::Aur;
    aur_paru.votes = Some(1257);
    let mut aur_firefox = pkg("firefox", "should not replace the repo one");
    aur_firefox.source = Source::Aur;
    index.merge_aur(vec![aur_paru, aur_firefox]);
    let paru = index.get("paru").unwrap();
    assert_eq!(paru.source, Source::Aur);
    assert_eq!(paru.votes, Some(1257));
    assert!(paru.is_installed(), "installed state must survive the merge");
    assert_eq!(index.get("firefox").unwrap().source, Source::Repo("extra".into()));
}
