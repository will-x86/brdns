//! Group-as-tenant data model.
//!
//! Everything hangs off an [`Account`], which is identified by an opaque
//! account number (16 digits or a UUID) — never email or any other PII. The
//! account owns an ordered list of [`Rule`]s and a set of DNS [`Upstream`]s.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Enums (serde lowercase, portable as TEXT columns in storage)
// ---------------------------------------------------------------------------

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $s),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok(Self::$variant),)+
                    _ => Err(format!(
                        "unknown {}: {s:?} (expected one of {})",
                        stringify!($name),
                        [$($s),+].join(", ")
                    )),
                }
            }
        }
    };
}

str_enum!(Action {
    Allow => "allow",
    Deny => "deny",
    Limit => "limit",
});

str_enum!(TargetType {
    Domain => "domain",
    Wildcard => "wildcard",
    Category => "category",
});

str_enum!(Window {
    Hour => "hour",
    Day => "day",
    Week => "week",
    Month => "month",
});

str_enum!(UpstreamProtocol {
    Dot => "dot",
    Doh => "doh",
    Udp => "udp",
});

// UpstreamProtocol: also derive Hash for UpstreamPool dedup.
impl std::hash::Hash for UpstreamProtocol {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

/// A tenant. The account number is the identity seen in the TLS SNI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub account_number: String,
}

/// A single entry in the account's ordered rule list (first match wins).
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

/// A device/API-key member of an account (used by the control plane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: i64,
    pub account_id: i64,
    /// Opaque device label, e.g. `laptop`. Not PII.
    pub name: String,
}

/// Snapshot of everything the pipeline needs for one account, refreshed on a
/// poll so the hot path never touches the control-plane storage.
#[derive(Debug, Clone, Default)]
pub struct AccountPolicy {
    pub rules: Vec<Rule>,
    pub upstream: Option<Upstream>,
}

// ---------------------------------------------------------------------------
// Input (create/update) types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAccount {
    /// Optional; a random 16-digit number is generated when absent.
    #[serde(default)]
    pub account_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRule {
    pub action: Action,
    pub target_type: TargetType,
    pub target_value: String,
    #[serde(default)]
    pub limit_count: Option<i64>,
    #[serde(default)]
    pub limit_window: Option<Window>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUpstream {
    pub name: String,
    pub protocol: UpstreamProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub addr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMember {
    pub name: String,
}

fn yes() -> bool {
    true
}

/// Generate a 16-digit account number (Like mullvad)
///
/// Digits come from the with rejection sampling so every
/// digit is uniform (no modulo bias). Not a secret — used only as an identifier.
pub fn generate_account_number() -> String {
    let mut out = String::with_capacity(16);
    let mut buf = [0u8; 1];
    while out.len() < 16 {
        openssl::rand::rand_bytes(&mut buf).expect("openssl rand_bytes failed");
        let b = buf[0];
        // Reject 250..=255 so `b % 10` is uniform over 0..=9.
        if b < 250 {
            out.push(char::from_digit((b % 10) as u32, 10).expect("digit"));
        }
    }
    out
}

/// Validate an account number: 1-64 chars, alphanumeric + hyphen.
pub fn is_valid_account_number(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_roundtrip_str() {
        assert_eq!(Action::Deny.as_str(), "deny");
        assert_eq!("allow".parse::<Action>().unwrap(), Action::Allow);
        assert_eq!(TargetType::Wildcard.as_str(), "wildcard");
        assert_eq!(
            "category".parse::<TargetType>().unwrap(),
            TargetType::Category
        );
        assert_eq!(Window::Month.as_str(), "month");
        assert_eq!("day".parse::<Window>().unwrap(), Window::Day);
        assert_eq!(UpstreamProtocol::Doh.as_str(), "doh");
        assert_eq!(
            "udp".parse::<UpstreamProtocol>().unwrap(),
            UpstreamProtocol::Udp
        );
        assert!("bogus".parse::<Action>().is_err());
    }

    #[test]
    fn account_number_is_16_digits() {
        let n = generate_account_number();
        assert_eq!(n.len(), 16, "{n}");
        assert!(n.chars().all(|c| c.is_ascii_digit()), "{n}");
    }
}
