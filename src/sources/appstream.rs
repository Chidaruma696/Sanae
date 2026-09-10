//! AppStream catalog data (`archlinux-appstream-data`): human names, summaries,
//! categories and screenshots for graphical applications. This is what turns a
//! package list into a store.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppInfo {
    pub id: String,
    pub pkgname: String,
    pub name: String,
    pub summary: String,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub screenshots: Vec<String>,
}

/// The store's shelves: label, and the freedesktop categories that land on it.
pub const SHELVES: &[(&str, &[&str])] = &[
    (
        "Internet",
        &[
            "Network",
            "WebBrowser",
            "Email",
            "Chat",
            "InstantMessaging",
            "FileTransfer",
            "P2P",
            "News",
            "Feed",
            "IRCClient",
            "RemoteAccess",
            "VideoConference",
        ],
    ),
    (
        "Multimedia",
        &[
            "AudioVideo",
            "Audio",
            "Video",
            "Player",
            "Recorder",
            "Music",
            "TV",
            "Midi",
            "Mixer",
            "Sequencer",
            "Tuner",
            "AudioVideoEditing",
            "DiscBurning",
        ],
    ),
    (
        "Graphics",
        &["Graphics", "2DGraphics", "3DGraphics", "Photography", "RasterGraphics", "VectorGraphics", "Scanning", "OCR"],
    ),
    (
        "Office",
        &[
            "Office",
            "WordProcessor",
            "Spreadsheet",
            "Presentation",
            "Calendar",
            "ContactManagement",
            "Finance",
            "Publishing",
            "ProjectManagement",
            "Dictionary",
            "Chart",
        ],
    ),
    (
        "Development",
        &[
            "Development",
            "IDE",
            "Debugger",
            "RevisionControl",
            "Building",
            "GUIDesigner",
            "WebDevelopment",
            "Profiling",
            "Translation",
            "Database",
        ],
    ),
    (
        "Games",
        &[
            "Game",
            "ActionGame",
            "AdventureGame",
            "ArcadeGame",
            "BoardGame",
            "BlocksGame",
            "CardGame",
            "Emulator",
            "KidsGame",
            "LogicGame",
            "RolePlaying",
            "Shooter",
            "Simulation",
            "SportsGame",
            "StrategyGame",
        ],
    ),
    (
        "Education",
        &[
            "Education",
            "Science",
            "Math",
            "Astronomy",
            "Chemistry",
            "Physics",
            "Biology",
            "Geography",
            "Geology",
            "Languages",
            "Electronics",
            "Engineering",
            "Medical",
            "Robotics",
        ],
    ),
    (
        "System",
        &[
            "System",
            "Settings",
            "DesktopSettings",
            "HardwareSettings",
            "PackageManager",
            "Security",
            "Monitor",
            "Filesystem",
            "TerminalEmulator",
            "Printing",
        ],
    ),
    (
        "Utilities",
        &[
            "Utility",
            "Accessibility",
            "Archiving",
            "Compression",
            "FileManager",
            "FileTools",
            "TextEditor",
            "TextTools",
            "Calculator",
            "Clock",
            "Viewer",
        ],
    ),
];

impl AppInfo {
    /// The first shelf whose categories this app belongs to, in shelf order.
    pub fn shelf(&self) -> &'static str {
        for (label, cats) in SHELVES {
            if self.categories.iter().any(|c| cats.contains(&c.as_str())) {
                return label;
            }
        }
        "Other"
    }
}

/// Where the catalog files live, newest layout first.
const DIRS: &[&str] = &["/usr/share/swcatalog/xml", "/usr/share/app-info/xmls", "/var/cache/swcatalog/xml"];

/// Load every catalog file found. Empty when the data package is not installed.
pub fn load() -> Result<HashMap<String, AppInfo>> {
    let mut apps: HashMap<String, AppInfo> = HashMap::new();
    for dir in DIRS {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !(name.ends_with(".xml.gz") || name.ends_with(".xml")) {
                continue;
            }
            let text = read_maybe_gz(&path)?;
            for app in parse(&text) {
                apps.entry(app.pkgname.clone()).or_insert(app);
            }
        }
        if !apps.is_empty() {
            break;
        }
    }
    Ok(apps)
}

fn read_maybe_gz(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    if path.extension().is_some_and(|e| e == "gz") {
        let mut s = String::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_string(&mut s)?;
        Ok(s)
    } else {
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Which element's text we are collecting.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    None,
    Id,
    Pkgname,
    Name,
    Summary,
    Category,
    Keyword,
    License,
    Homepage,
    Image,
}

/// Parse one catalog document. Only desktop applications are kept; only untranslated
/// (default language) texts are used.
#[allow(clippy::while_let_loop)]
pub fn parse(xml: &str) -> Vec<AppInfo> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut cur: Option<AppInfo> = None;
    let mut is_app = false;
    let mut field = Field::None;
    let mut depth_in_screenshots = 0usize;
    loop {
        let ev = match reader.read_event() {
            Ok(ev) => ev,
            Err(_) => break,
        };
        match ev {
            Event::Start(e) => {
                let tag = e.name();
                let tag = tag.as_ref();
                let has_lang = e.attributes().flatten().any(|a| a.key.as_ref() == b"xml:lang");
                let attr = |k: &[u8]| -> Option<String> {
                    e.attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == k)
                        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
                };
                match tag {
                    b"component" => {
                        cur = Some(AppInfo::default());
                        let t = attr(b"type").unwrap_or_default();
                        is_app = t == "desktop-application" || t == "desktop";
                    }
                    b"screenshots" => depth_in_screenshots = 1,
                    _ if cur.is_none() => {}
                    b"id" => field = Field::Id,
                    b"pkgname" => field = Field::Pkgname,
                    b"name" if !has_lang => field = Field::Name,
                    b"summary" if !has_lang => field = Field::Summary,
                    b"category" => field = Field::Category,
                    b"keyword" if !has_lang => field = Field::Keyword,
                    b"project_license" => field = Field::License,
                    b"url" if attr(b"type").as_deref() == Some("homepage") => field = Field::Homepage,
                    b"image" if depth_in_screenshots > 0 && attr(b"type").as_deref() != Some("thumbnail") => {
                        field = Field::Image
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if let (Some(app), Ok(text)) = (cur.as_mut(), t.unescape()) {
                    let text = text.trim().to_string();
                    match field {
                        Field::Id if app.id.is_empty() => app.id = text,
                        Field::Pkgname if app.pkgname.is_empty() => app.pkgname = text,
                        Field::Name if app.name.is_empty() => app.name = text,
                        Field::Summary if app.summary.is_empty() => app.summary = text,
                        Field::Category => app.categories.push(text),
                        Field::Keyword => app.keywords.push(text),
                        Field::License if app.license.is_none() => app.license = Some(text),
                        Field::Homepage if app.homepage.is_none() => app.homepage = Some(text),
                        Field::Image => app.screenshots.push(text),
                        _ => {}
                    }
                }
            }
            Event::End(e) => {
                let tag = e.name();
                match tag.as_ref() {
                    b"component" => {
                        if let Some(app) = cur.take()
                            && is_app
                            && !app.pkgname.is_empty()
                            && !app.name.is_empty()
                        {
                            out.push(app);
                        }
                        is_app = false;
                    }
                    b"screenshots" => depth_in_screenshots = 0,
                    _ => {}
                }
                field = Field::None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_desktop_apps_and_skips_translations_and_addons() {
        let xml = r#"<?xml version="1.0"?>
<components version="0.14" origin="archlinux-arch-extra">
  <component type="desktop-application">
    <id>org.mozilla.firefox</id>
    <pkgname>firefox</pkgname>
    <name>Firefox</name>
    <name xml:lang="es">Firefox en español</name>
    <summary>Web Browser</summary>
    <summary xml:lang="es">Navegador web</summary>
    <project_license>MPL-2.0</project_license>
    <url type="homepage">https://www.mozilla.org/firefox/</url>
    <url type="bugtracker">https://bugzilla.mozilla.org/</url>
    <categories><category>Network</category><category>WebBrowser</category></categories>
    <keywords><keyword>browser</keyword><keyword xml:lang="es">navegador</keyword></keywords>
    <screenshots><screenshot type="default"><image type="source">https://x/1.png</image><image type="thumbnail">https://x/t.png</image></screenshot></screenshots>
  </component>
  <component type="addon">
    <id>some.addon</id><pkgname>addon</pkgname><name>Addon</name>
  </component>
</components>"#;
        let apps = parse(xml);
        assert_eq!(apps.len(), 1);
        let ff = &apps[0];
        assert_eq!(ff.pkgname, "firefox");
        assert_eq!(ff.name, "Firefox");
        assert_eq!(ff.summary, "Web Browser");
        assert_eq!(ff.categories, vec!["Network", "WebBrowser"]);
        assert_eq!(ff.keywords, vec!["browser"]);
        assert_eq!(ff.license.as_deref(), Some("MPL-2.0"));
        assert_eq!(ff.homepage.as_deref(), Some("https://www.mozilla.org/firefox/"));
        assert_eq!(ff.screenshots, vec!["https://x/1.png"]);
        assert_eq!(ff.shelf(), "Internet");
    }
}
