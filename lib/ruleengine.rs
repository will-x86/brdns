//! Ordered rule evaluation (Pi-hole style): first match wins.
//!
//! Each account has an ordered list of rules. The engine walks them in order
//! and returns the decision of the first rule whose target matches the query
//! name. Category targets consult the [`CategoryIndex`]; domain/wildcard
//! targets are matched here.

use crate::model::{Action, Rule, TargetType, Window};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allow the query (also the implicit default when no rule matches).
    Allow,
    /// Block the query.
    Deny,
    /// Allow only if under the per-window quota.
    Limit {
        rule_id: i64,
        limit_count: i64,
        window: Window,
    },
}

fn normalize(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

/// Match a query name against a single rule target (domain/wildcard only;
/// category targets always return false here).
pub fn matches_target(qname: &str, target_type: TargetType, target_value: &str) -> bool {
    match target_type {
        TargetType::Domain => normalize(qname) == normalize(target_value),
        TargetType::Wildcard => {
            let suffix = normalize(target_value.trim_start_matches("*."));
            if suffix.is_empty() {
                return false;
            }
            let q = normalize(qname);
            q == suffix || q.ends_with(&format!(".{suffix}"))
        }
        TargetType::Category => false,
    }
}

/// Evaluate an ordered rule list for `qname`.
///
/// `in_category` reports whether `qname` belongs to the given category name.
/// Returns `None` when no rule matches (the caller's default applies).
pub fn evaluate(
    rules: &[Rule],
    qname: &str,
    in_category: &dyn Fn(&str) -> bool,
) -> Option<Decision> {
    for rule in rules.iter().filter(|r| r.enabled) {
        let matched = match rule.target_type {
            TargetType::Category => in_category(&rule.target_value),
            _ => matches_target(qname, rule.target_type, &rule.target_value),
        };
        if matched {
            return Some(match rule.action {
                Action::Allow => Decision::Allow,
                Action::Deny => Decision::Deny,
                Action::Limit => Decision::Limit {
                    rule_id: rule.id,
                    // A CHECK constraint guarantees these for 'limit' rules in
                    // Postgres; be lenient here rather than panic.
                    limit_count: rule.limit_count.unwrap_or(0),
                    window: rule.limit_window.unwrap_or(Window::Month),
                },
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, TargetType};

    fn rule(action: Action, target_type: TargetType, value: &str) -> Rule {
        Rule {
            id: 1,
            account_id: 1,
            position: 0,
            action,
            target_type,
            target_value: value.into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        }
    }

    #[test]
    fn domain_exact() {
        assert!(matches_target(
            "example.com",
            TargetType::Domain,
            "example.com"
        ));
        assert!(matches_target(
            "Example.COM.",
            TargetType::Domain,
            "example.com"
        ));
        assert!(!matches_target(
            "www.example.com",
            TargetType::Domain,
            "example.com"
        ));
    }

    #[test]
    fn wildcard_suffix() {
        assert!(matches_target(
            "example.com",
            TargetType::Wildcard,
            "*.example.com"
        ));
        assert!(matches_target(
            "www.example.com",
            TargetType::Wildcard,
            "*.example.com"
        ));
        assert!(matches_target(
            "a.b.example.com",
            TargetType::Wildcard,
            "*.example.com"
        ));
        assert!(!matches_target(
            "example.org",
            TargetType::Wildcard,
            "*.example.com"
        ));
    }

    #[test]
    fn first_match_wins() {
        let rules = vec![
            rule(Action::Allow, TargetType::Domain, "allowed.com"),
            rule(Action::Deny, TargetType::Wildcard, "*.example.com"),
        ];
        assert_eq!(
            evaluate(&rules, "allowed.com", &|_| false),
            Some(Decision::Allow)
        );
        assert_eq!(
            evaluate(&rules, "sub.example.com", &|_| false),
            Some(Decision::Deny)
        );
        assert_eq!(evaluate(&rules, "other.org", &|_| false), None);
    }

    #[test]
    fn category_rule_uses_index() {
        let rules = vec![rule(Action::Limit, TargetType::Category, "youtube")];
        // Simulate an index: "www.youtube.com" belongs to "youtube", nothing else does.
        let eval = |qname: &str| {
            evaluate(&rules, qname, &|cat| {
                cat == "youtube" && qname == "www.youtube.com"
            })
        };
        assert_eq!(
            eval("www.youtube.com"),
            Some(Decision::Limit {
                rule_id: 1,
                limit_count: 0,
                window: Window::Month
            })
        );
        assert_eq!(eval("example.com"), None);
    }

    #[test]
    fn disabled_rules_are_skipped() {
        let mut disabled = rule(Action::Deny, TargetType::Domain, "example.com");
        disabled.enabled = false;
        assert_eq!(evaluate(&[disabled], "example.com", &|_| false), None);
    }
}
