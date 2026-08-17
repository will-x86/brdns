//! Identity extraction for SNI-based accounts.
//!
//! Both DoT and DoH identify the caller by the TLS SNI: the account is the
//! leftmost label of the hostname the client dialed, e.g.
//! `1234567890.dns.example.com` -> account `1234567890`.

/// Extract an account id from an SNI hostname by stripping the base domain.
///
/// Returns `None` when the SNI is empty, equals the base domain itself, has
/// extra labels, or carries a trailing dot.
pub fn account_from_sni(sni: &str, domain: &str) -> Option<String> {
    let sni = sni.trim_end_matches('.');
    let domain = domain.trim_matches('.');

    if domain.is_empty() || sni.is_empty() {
        return None;
    }
    // The base domain alone is not an account.
    if sni == domain {
        return None;
    }

    let suffix = format!(".{domain}");
    let account = sni.strip_suffix(&suffix)?;

    // The account must be exactly one DNS label.
    if account.is_empty() || account.contains('.') {
        return None;
    }
    Some(account.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_base_domain() {
        assert_eq!(
            account_from_sni("1234567890.dns.example.com", "dns.example.com"),
            Some("1234567890".into())
        );
    }

    #[test]
    fn rejects_base_domain() {
        assert_eq!(account_from_sni("dns.example.com", "dns.example.com"), None);
    }

    #[test]
    fn rejects_multi_label_account() {
        assert_eq!(
            account_from_sni("a.b.dns.example.com", "dns.example.com"),
            None
        );
    }

    #[test]
    fn handles_trailing_dot() {
        assert_eq!(
            account_from_sni("acct.dns.example.com.", "dns.example.com"),
            Some("acct".into())
        );
    }

    #[test]
    fn rejects_unrelated_sni() {
        assert_eq!(account_from_sni("other.org", "dns.example.com"), None);
    }
}
