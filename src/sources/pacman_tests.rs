//! Parsing tests with recorded expac output. No pacman needed.

use super::pacman::{parse_local, parse_sync, parse_updates};
use crate::model::Source;

const LS: &str = "\x1f";

#[test]
fn sync_lines_become_packages() {
    let text = format!(
        "firefox\t143.0-1\textra\tFast, Private & Safe Web Browser\t\t262144000\t73400320\thttps://www.mozilla.org/firefox/\tMPL-2.0\n\
         xfce4-panel\t4.20.4-1\textra\tPanel for the Xfce desktop environment\txfce4\t8388608\t1048576\thttps://xfce.org\tGPL-2.0-or-later{LS}LGPL-2.1-or-later\n\
         broken line\n"
    );
    let pkgs = parse_sync(&text);
    assert_eq!(pkgs.len(), 2);
    let ff = &pkgs[0];
    assert_eq!(ff.name, "firefox");
    assert_eq!(ff.version, "143.0-1");
    assert_eq!(ff.source, Source::Repo("extra".into()));
    assert_eq!(ff.install_size, Some(262_144_000));
    assert_eq!(ff.download_size, Some(73_400_320));
    assert!(ff.groups.is_empty());
    assert_eq!(ff.licenses, vec!["MPL-2.0"]);
    let panel = &pkgs[1];
    assert_eq!(panel.groups, vec!["xfce4"]);
    assert_eq!(panel.licenses.len(), 2);
    assert_eq!(panel.url.as_deref(), Some("https://xfce.org"));
}

#[test]
fn local_lines_carry_install_reason_and_date() {
    let text = "paru\t2.1.0-2\tExplicitly installed\t1757500000\t12345678\tFeature packed AUR helper\thttps://github.com/Morganamilo/paru\tGPL-3.0-or-later\t\n\
                zlib\t1.3.1-2\tInstalled as a dependency for another package\t1757400000\t500000\tCompression library\tNone\tZlib\t\n";
    let pkgs = parse_local(text);
    assert_eq!(pkgs.len(), 2);
    assert!(pkgs[0].installed.explicit);
    assert_eq!(pkgs[0].installed.install_date, Some(1_757_500_000));
    assert!(!pkgs[1].installed.explicit);
    assert_eq!(pkgs[1].url, None, "'None' from expac must become no URL");
    assert_eq!(pkgs[1].licenses, vec!["Zlib"]);
}

#[test]
fn checkupdates_lines_become_updates() {
    let ups = parse_updates("linux 6.17.1.arch1-1 -> 6.17.2.arch1-1\nnonsense\n");
    assert_eq!(ups.len(), 1);
    assert_eq!(ups[0].name, "linux");
    assert_eq!(ups[0].current, "6.17.1.arch1-1");
    assert_eq!(ups[0].new, "6.17.2.arch1-1");
}
