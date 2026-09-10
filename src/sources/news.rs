//! The Arch Linux news feed, shown before updating: this is where "manual
//! intervention required" gets announced.

use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::cache::Cache;

const FEED: &str = "https://archlinux.org/feeds/news/";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub date: String,
    pub link: String,
}

impl NewsItem {
    /// The wording Arch uses when an update needs a human.
    pub fn needs_attention(&self) -> bool {
        let t = self.title.to_lowercase();
        t.contains("manual intervention") || t.contains("requires") || t.contains("action required")
    }
}

/// The latest news items, cached for an hour.
pub async fn fetch(http: &reqwest::Client, cache: &Cache) -> Result<Vec<NewsItem>> {
    let key = "news";
    let text = match cache.get(key, 3600) {
        Some(t) => t,
        None => {
            let t = http.get(FEED).send().await.context("reaching archlinux.org")?.error_for_status()?.text().await?;
            cache.put(key, &t);
            t
        }
    };
    Ok(parse(&text))
}

pub fn parse(rss: &str) -> Vec<NewsItem> {
    let mut reader = Reader::from_str(rss);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut cur: Option<NewsItem> = None;
    let mut field = "";
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"item" => cur = Some(NewsItem { title: String::new(), date: String::new(), link: String::new() }),
                b"title" => field = "title",
                b"pubDate" => field = "date",
                b"link" => field = "link",
                _ => field = "",
            },
            Ok(Event::Text(t)) => {
                if let (Some(item), Ok(text)) = (cur.as_mut(), t.unescape()) {
                    let text = text.trim();
                    match field {
                        "title" => item.title = text.to_string(),
                        "date" => item.date = text.chars().take(16).collect(),
                        "link" => item.link = text.to_string(),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"item"
                    && let Some(item) = cur.take()
                {
                    items.push(item);
                }
                field = "";
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    items
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_items() {
        let rss = r#"<rss><channel><title>Arch Linux: Recent news updates</title>
<item><title>linux-firmware &gt;= 20250613 upgrade requires manual intervention</title><link>https://archlinux.org/news/x/</link><pubDate>Sat, 21 Jun 2025 10:00:00 +0000</pubDate></item>
<item><title>Plasma 6.4</title><link>https://archlinux.org/news/y/</link><pubDate>Mon, 23 Jun 2025 10:00:00 +0000</pubDate></item>
</channel></rss>"#;
        let items = super::parse(rss);
        assert_eq!(items.len(), 2);
        assert!(items[0].needs_attention());
        assert!(!items[1].needs_attention());
        assert_eq!(items[0].date, "Sat, 21 Jun 2025");
        assert!(items[0].title.contains(">="));
    }
}
