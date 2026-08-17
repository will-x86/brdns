//! Entity structs

use serde::{Deserialize, Serialize};

use super::{Action, TargetType, UpstreamProtocol, Window};

/// A tenant. The account number is identity (sni)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub account_number: String,
}

/// An entry in an account's ordered rule list (first match wins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub account_id: i64,
    /// 0-based order within the list.
    pub position: i32,
    pub action: Action,
    pub target_type: TargetType,
    /// Domain name, wildcard (`*.example.com`), or category name.
    pub target_value: String,
    /// For [`Action::Limit`]: the per-window budget.
    pub limit_count: Option<i64>,
    /// For [`Action::Limit`]: the counting window.
    pub limit_window: Option<Window>,
    pub enabled: bool,
}

/// A DNS upstream. `account_id = None` marks a global preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    pub id: i64,
    pub account_id: Option<i64>,
    pub name: String,
    pub protocol: UpstreamProtocol,
    pub host: String,
    pub port: u16,
    /// Optional pinned `IP:port` override (mainly for DoH SNI/pinning).
    pub addr: Option<String>,
    pub is_preset: bool,
}

/// A device/API-key member of an account
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: i64,
    pub account_id: i64,
    /// Device label
    pub name: String,
}

/// Everything the pipeline needs for one account, refreshed on a
/// poll
#[derive(Debug, Clone, Default)]
pub struct AccountPolicy {
    pub rules: Vec<Rule>,
    pub upstream: Option<Upstream>,
}
