//! Observability: Prom metrics, logs, and otel
//!
//! Metrics are recorded per account and per outcome.
//! No domain/qname ever appears in a metric label, log line, or span
//! attribute.

mod metrics;
mod tracing;

pub use metrics::{
    Outcome, metrics_router, record_blocked, record_query, render, serve_metrics,
    set_category_domains, set_policy_accounts, set_upstream_transports,
};
pub use tracing::init;
