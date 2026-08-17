//! Control plane - storage layer behind the DNS policy engine and
//! the management API.
//!
//! The [`ControlPlane`] trait is the only thing the rest of the service depends
//! on. There are two implementations rn:
//!
//! - [`storage::InMemControlPlane`]: in-memory,
//! - [`storage::PostgresControlPlane`]: Postgres via sqlx.

pub mod http;
pub mod storage;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;

use crate::model::{
    Account, AccountPolicy, NewRule, NewUpstream, Rule, Upstream, UpstreamProtocol, Window,
};

pub use storage::{InMemControlPlane, PostgresControlPlane};

pub type CplResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[async_trait]
pub trait ControlPlane: Send + Sync {
    // Read path used by the DNS pipeline.

    /// Ordered enabled rules for an account
    /// First match wins
    async fn rules(&self, account_number: &str) -> CplResult<Vec<Rule>>;

    /// Account active upstream if there is one.
    async fn active_upstream(&self, account_number: &str) -> CplResult<Option<Upstream>>;

    /// Record one query against a limit rule's current window and
    /// report whether the account remains within budget (`false` = over quota).
    async fn record_quota(
        &self,
        account_number: &str,
        rule_id: i64,
        limit_count: i64,
        window: Window,
    ) -> CplResult<bool>;

    // Management.

    async fn create_account(&self, account_number: &str) -> CplResult<Account>;
    async fn get_account(&self, account_number: &str) -> CplResult<Option<Account>>;

    /// Replace the account's rule list.
    async fn replace_rules(&self, account_number: &str, rules: &[NewRule]) -> CplResult<Vec<Rule>>;

    async fn list_upstreams(&self, account_number: &str) -> CplResult<Vec<Upstream>>;

    /// Replace the account's custom upstreams.
    async fn replace_upstreams(
        &self,
        account_number: &str,
        upstreams: &[NewUpstream],
    ) -> CplResult<Vec<Upstream>>;

    async fn preset_upstreams(&self) -> CplResult<Vec<Upstream>>;

    // Global category index (blocklists, step 7).

    /// Domain to categories map.
    async fn categories(&self) -> CplResult<HashMap<String, HashSet<String>>>;

    /// Replace the category map (after a blocklist refresh).
    async fn replace_categories(
        &self,
        categories: &HashMap<String, HashSet<String>>,
    ) -> CplResult<()>;

    /// Unix epoch seconds of the last blocklist ingestion, if any.
    async fn last_ingestion(&self) -> CplResult<Option<i64>>;

    /// account to policy snapshot (rules + active upstream)
    async fn snapshot(&self) -> CplResult<HashMap<String, AccountPolicy>>;
}

/// Preset upstreams
pub fn preset_upstreams_default() -> Vec<NewUpstream> {
    vec![
        NewUpstream {
            name: "cloudflare-dot".into(),
            protocol: UpstreamProtocol::Dot,
            host: "1.1.1.1".into(),
            port: 853,
            addr: None,
        },
        NewUpstream {
            name: "cloudflare-doh".into(),
            protocol: UpstreamProtocol::Doh,
            host: "cloudflare-dns.com".into(),
            port: 443,
            addr: Some("1.1.1.1:443".into()),
        },
        NewUpstream {
            name: "quad9-dot".into(),
            protocol: UpstreamProtocol::Dot,
            host: "9.9.9.9".into(),
            port: 853,
            addr: None,
        },
        NewUpstream {
            name: "quad9-doh".into(),
            protocol: UpstreamProtocol::Doh,
            host: "dns.quad9.net".into(),
            port: 443,
            addr: Some("9.9.9.9:443".into()),
        },
    ]
}

/// Init control plane
pub async fn init(database_url: Option<&str>) -> CplResult<Arc<dyn ControlPlane>> {
    match database_url {
        Some(url) => Ok(Arc::new(PostgresControlPlane::connect(url).await?)),
        None => Ok(Arc::new(InMemControlPlane::default())),
    }
}

/// Unix time in seconds.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs() as i64
}
