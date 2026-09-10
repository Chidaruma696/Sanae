//! Package popularity from pkgstats.archlinux.de: the percentage of reporting
//! systems that have a package installed. Used to rank the store.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cache::Cache;

const API: &str = "https://pkgstats.archlinux.de/api/packages";
/// The list is sorted by popularity; below the first 10 000 nobody cares.
const TOP: usize = 10_000;

#[derive(Deserialize)]
struct Page {
    #[serde(rename = "packagePopularities")]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    name: String,
    popularity: f64,
}

/// name -> popularity in percent, for the most installed packages. Cached for a day.
pub async fn top(http: &reqwest::Client, cache: &Cache) -> Result<HashMap<String, f64>> {
    let key = "pkgstats-top";
    let text = match cache.get(key, 86_400) {
        Some(t) => t,
        None => {
            let url = format!("{API}?limit={TOP}&offset=0");
            let t = http.get(&url).send().await.context("reaching pkgstats")?.error_for_status()?.text().await?;
            cache.put(key, &t);
            t
        }
    };
    parse(&text)
}

pub fn parse(text: &str) -> Result<HashMap<String, f64>> {
    let page: Page = serde_json::from_str(text).context("reading pkgstats")?;
    Ok(page.items.into_iter().map(|i| (i.name, i.popularity)).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_a_page() {
        let m = super::parse(r#"{"total":2,"count":2,"packagePopularities":[{"name":"firefox","popularity":69.22},{"name":"vim","popularity":40.5}],"limit":2,"offset":0}"#).unwrap();
        assert_eq!(m["firefox"], 69.22);
        assert_eq!(m.len(), 2);
    }
}
