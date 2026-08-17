//! Input (create/update) payloads and account-num helpers.

use serde::{Deserialize, Serialize};

use super::{Action, TargetType, UpstreamProtocol, Window};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAccount {
    /// Optional; generated when absent
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
