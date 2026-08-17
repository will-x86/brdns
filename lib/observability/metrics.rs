//! Prometheus metrics (aggregate only, never query names).

use axum::{Router, routing::get};
use once_cell::sync::Lazy;
use prometheus::{
    HistogramVec, IntCounterVec, IntGauge, TextEncoder, register_histogram_vec,
    register_int_counter_vec, register_int_gauge,
};
use std::time::Duration;

static QUERIES: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "brdns_queries_total",
        "DNS queries handled, by account/outcome/protocol",
        &["account", "action", "protocol"]
    )
    .unwrap()
});

static QUERY_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "brdns_query_duration_seconds",
        "DNS query latency",
        &["account", "protocol"]
    )
    .unwrap()
});

static BLOCKED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "brdns_blocked_total",
        "Queries blocked, by account and reason (deny/limit)",
        &["account", "reason"]
    )
    .unwrap()
});

static CATEGORY_DOMAINS: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!(
        "brdns_category_domains",
        "Domains currently in the category index"
    )
    .unwrap()
});

static POLICY_ACCOUNTS: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!(
        "brdns_policy_accounts",
        "Accounts currently in the policy cache"
    )
    .unwrap()
});

static UPSTREAM_TRANSPORTS: Lazy<IntGauge> = Lazy::new(|| {
    register_int_gauge!(
        "brdns_upstream_transports",
        "Distinct upstream transports currently pooled"
    )
    .unwrap()
});

/// Outcome of a single query, used for metric labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Allow,
    Deny,
    LimitOk,
    LimitExceeded,
    Error,
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::LimitOk => "limit_ok",
            Self::LimitExceeded => "limit_exceeded",
            Self::Error => "error",
        }
    }
}

pub fn record_query(account: &str, protocol: &str, outcome: Outcome, duration: Duration) {
    QUERIES
        .with_label_values(&[account, outcome.as_str(), protocol])
        .inc();
    QUERY_DURATION
        .with_label_values(&[account, protocol])
        .observe(duration.as_secs_f64());
}

pub fn record_blocked(account: &str, reason: &str) {
    BLOCKED.with_label_values(&[account, reason]).inc();
}

pub fn set_category_domains(n: i64) {
    CATEGORY_DOMAINS.set(n);
}

pub fn set_policy_accounts(n: i64) {
    POLICY_ACCOUNTS.set(n);
}

pub fn set_upstream_transports(n: i64) {
    UPSTREAM_TRANSPORTS.set(n);
}

fn ensure_registered() {
    let _ = &*QUERIES;
    let _ = &*QUERY_DURATION;
    let _ = &*BLOCKED;
    let _ = &*CATEGORY_DOMAINS;
    let _ = &*POLICY_ACCOUNTS;
    let _ = &*UPSTREAM_TRANSPORTS;
}

pub fn render() -> String {
    ensure_registered();
    let encoder = TextEncoder::new();
    encoder
        .encode_to_string(&prometheus::gather())
        .unwrap_or_default()
}

pub fn metrics_router() -> Router {
    Router::new().route("/metrics", get(metrics_handler))
}

async fn metrics_handler() -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::OK, render())
}

pub async fn serve_metrics(addr: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, metrics_router()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_render_includes_registered_metrics() {
        record_query("acct", "dot", Outcome::Allow, Duration::from_millis(12));
        record_blocked("acct", "deny");
        set_category_domains(42);
        set_policy_accounts(3);
        set_upstream_transports(2);

        let text = render();
        assert!(text.contains("brdns_queries_total"));
        assert!(text.contains("brdns_blocked_total"));
        assert!(text.contains("brdns_query_duration_seconds"));
        assert!(text.contains("brdns_category_domains 42"));
        assert!(text.contains("brdns_policy_accounts 3"));
        assert!(text.contains("brdns_upstream_transports 2"));
        assert!(!text.contains("example.com"));
    }
}
