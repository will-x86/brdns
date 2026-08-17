//! Blocklist/category ingestion.
//!
//! Community feeds (OISD, Hagezi, StevenBlack, AdGuard) are fetched, parsed
//! into a `domain -> categories` map, persisted via the control plane, and
//! loaded into the in-memory [`CategoryIndex`] the rule engine consults.
//!
//! License notes for the built-in feeds (reviewed before inclusion):
//! - OISD          https://oisd.nl — free to use (personal + commercial).
//! - StevenBlack   hosts — MIT-licensed repo; aggregates lists under their own
//!   (per-list) licenses.
//! - Hagezi        GPL-3.0 (https://github.com/hagezi/dns-blocklists).
//! - AdGuard DNS   GPL-3.0 (https://github.com/AdguardTeam/AdGuardSDNSFilter).

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;

use crate::categories::CategoryIndex;
use crate::controlplane::ControlPlane;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedFormat {
    /// One domain per line, `#` comments, optional `*.` wildcard prefix.
    Plain,
    /// `/etc/hosts` style: `<ip> <host> [host...]`.
    Hosts,
    /// AdBlock/AdGuard syntax (`||domain^`, `!` comments, `@@` exceptions).
    Adblock,
}

impl FeedFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Hosts => "hosts",
            Self::Adblock => "adblock",
        }
    }
}

impl FromStr for FeedFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plain" => Ok(Self::Plain),
            "hosts" => Ok(Self::Hosts),
            "adblock" => Ok(Self::Adblock),
            _ => Err(format!("unknown feed format {s:?} (plain|hosts|adblock)")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Feed {
    pub name: String,
    pub url: String,
    pub format: FeedFormat,
    /// Categories every domain from this feed is tagged with.
    pub categories: Vec<String>,
}

impl From<&crate::config::FeedConfig> for Feed {
    fn from(c: &crate::config::FeedConfig) -> Self {
        Self {
            name: c.name.clone(),
            url: c.url.clone(),
            format: c.format.parse().unwrap_or(FeedFormat::Plain),
            categories: c.categories.clone(),
        }
    }
}

/// Built-in, license-reviewed feeds.
///
/// Kept modest (~250k unique domains after dedup) so a small VPS can hold the
/// in-memory index comfortably. Add category feeds (gambling, social, ...)
/// via `[blocklist.feeds]`.
pub fn default_feeds() -> Vec<Feed> {
    vec![
        Feed {
            name: "oisd".into(),
            url: "https://small.oisd.nl/domainswild".into(),
            format: FeedFormat::Plain,
            categories: vec!["ads".into()],
        },
        Feed {
            name: "stevenblack".into(),
            url: "https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts".into(),
            format: FeedFormat::Hosts,
            categories: vec!["ads".into()],
        },
        Feed {
            name: "hagezi-multi".into(),
            url: "https://raw.githubusercontent.com/hagezi/dns-blocklists/main/adblock/multi.txt"
                .into(),
            format: FeedFormat::Adblock,
            categories: vec!["ads".into()],
        },
        Feed {
            name: "adguard-dns".into(),
            url: "https://adguardteam.github.io/AdGuardSDNSFilter/Filters/filter.txt".into(),
            format: FeedFormat::Adblock,
            categories: vec!["ads".into()],
        },
    ]
}

/// Load the persisted `domain -> categories` map from the control plane into
/// the in-memory index. Runs at startup so a restart keeps existing data.
pub async fn load(
    cp: &dyn ControlPlane,
    index: &CategoryIndex,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let map = cp.categories().await?;
    let n = map.len();
    index.replace(map);
    Ok(n)
}

/// Fetch all feeds, parse, merge, persist, and reload the in-memory index.
/// Returns the total number of domains indexed.
pub async fn ingest(
    cp: &dyn ControlPlane,
    index: &CategoryIndex,
    feeds: &[Feed],
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .user_agent(format!("brdns/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    let mut tasks = tokio::task::JoinSet::new();
    for feed in feeds {
        let client = client.clone();
        let feed = feed.clone();
        tasks.spawn(async move {
            let domains = fetch_and_parse(&client, &feed).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>((feed.name, feed.categories, domains))
        });
    }

    let mut merged: HashMap<String, HashSet<String>> = HashMap::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok((name, categories, domains))) => {
                log::info!("blocklist feed {name}: {} domains", domains.len());
                for domain in domains {
                    merged
                        .entry(domain)
                        .or_default()
                        .extend(categories.iter().cloned());
                }
            }
            Ok(Err(e)) => log::warn!("blocklist feed failed: {e}"),
            Err(e) => log::warn!("blocklist task failed: {e}"),
        }
    }

    let n = merged.len();
    cp.replace_categories(&merged).await?;
    index.replace(merged);
    Ok(n)
}

async fn fetch_and_parse(
    client: &reqwest::Client,
    feed: &Feed,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    let body = client
        .get(&feed.url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_domains(feed.format, &body))
}

/// Parse a feed body into a set of normalized domains.
pub fn parse_domains(format: FeedFormat, text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in text.lines() {
        match format {
            FeedFormat::Plain => parse_plain_line(line, &mut out),
            FeedFormat::Hosts => parse_hosts_line(line, &mut out),
            FeedFormat::Adblock => parse_adblock_line(line, &mut out),
        }
    }
    out
}

fn parse_plain_line(line: &str, out: &mut HashSet<String>) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    // Allow an inline `#` comment.
    let token = line.split('#').next().unwrap_or("").trim();
    if let Some(domain) = sanitize(token) {
        out.insert(domain);
    }
}

fn parse_hosts_line(line: &str, out: &mut HashSet<String>) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    let mut tokens = line.split_whitespace();
    let _ip = tokens.next(); // 0.0.0.0 / 127.0.0.1 / ::1 ...
    for host in tokens {
        if let Some(domain) = sanitize(host) {
            out.insert(domain);
        }
    }
}

fn parse_adblock_line(line: &str, out: &mut HashSet<String>) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
        return;
    }
    // Allowlist exceptions are not block entries.
    if line.starts_with("@@") {
        return;
    }
    // Only `||domain^`-style entries (the common case for domain blocklists).
    if let Some(rest) = line.strip_prefix("||") {
        let cut = rest
            .find(['^', '$', '/'])
            .map(|i| &rest[..i])
            .unwrap_or(rest);
        if let Some(domain) = sanitize(cut) {
            out.insert(domain);
        }
    }
}

/// Normalize a domain token; returns `None` when it isn't a usable domain.
fn sanitize(token: &str) -> Option<String> {
    let mut domain = token.trim().to_ascii_lowercase();
    while let Some(stripped) = domain.strip_prefix("*.") {
        domain = stripped.to_string();
    }
    domain = domain.trim_end_matches('.').to_string();
    if domain.is_empty() || domain.len() > 253 {
        return None;
    }
    // Domain labels only: letters, digits, hyphen, underscore, dot.
    if !domain
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
    {
        return None;
    }
    // Needs a dot (drops "localhost") and at least one alphabetic char
    // (drops bare IPs / numeric tokens).
    if !domain.contains('.') || !domain.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_format() {
        let text = "# comment\n*.ads.example.com\nplain.example.org\nbadsite.com # inline\n";
        let got = parse_domains(FeedFormat::Plain, text);
        assert!(got.contains("ads.example.com"));
        assert!(got.contains("plain.example.org"));
        assert!(got.contains("badsite.com"));
        assert!(!got.contains("*.ads.example.com"));
    }

    #[test]
    fn hosts_format() {
        let text = "0.0.0.0 ads.example.com\n127.0.0.1 localhost\n0.0.0.0 tracker.io junk.io\n";
        let got = parse_domains(FeedFormat::Hosts, text);
        assert!(got.contains("ads.example.com"));
        assert!(got.contains("tracker.io"));
        assert!(got.contains("junk.io"));
        assert!(!got.contains("localhost"));
    }

    #[test]
    fn adblock_format() {
        let text = "! comment\n[Adblock Plus]\n||ads.example.com^\n||tracker.io^$third-party\n@@||allowed.example.com^\n||path.example.com/foo^\n";
        let got = parse_domains(FeedFormat::Adblock, text);
        assert!(got.contains("ads.example.com"));
        assert!(got.contains("tracker.io"));
        assert!(got.contains("path.example.com"));
        assert!(!got.contains("allowed.example.com"));
    }

    #[test]
    fn sanitize_rejects_bad_tokens() {
        assert_eq!(sanitize("127.0.0.1"), None);
        assert_eq!(sanitize("localhost"), None);
        assert_eq!(
            sanitize("*.good.example.com"),
            Some("good.example.com".into())
        );
        assert_eq!(sanitize("bad site.com"), None);
    }
}
