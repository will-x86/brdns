//! Observability smoke tests. OTel export is skipped unless
//! `BRDNS_TEST_OTEL_ENDPOINT` points at a reachable collector.

use std::time::Duration;

use brdns::config::ObservabilityConfig;

#[tokio::test]
async fn metrics_endpoint_serves() {
    // Record activity so label-vec metrics have a child to render.
    brdns::observability::record_query(
        "acct",
        "dot",
        brdns::observability::Outcome::Allow,
        Duration::from_millis(1),
    );
    brdns::observability::record_blocked("acct", "deny");

    let router = brdns::observability::metrics_router();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("brdns_queries_total"));
    assert!(body.contains("brdns_blocked_total"));
}

#[tokio::test]
async fn otel_export_smoke() {
    let Some(endpoint) = std::env::var("BRDNS_TEST_OTEL_ENDPOINT").ok() else {
        eprintln!("skipping OTel smoke test (set BRDNS_TEST_OTEL_ENDPOINT)");
        return;
    };
    let cfg = ObservabilityConfig {
        metrics_addr: String::new(),
        otel_endpoint: Some(endpoint),
    };
    brdns::observability::init(&cfg).expect("init observability");

    for _ in 0..3 {
        let span = tracing::info_span!("test_query", account = "acct");
        let _e = span.enter();
        tracing::info!("query handled");
    }
    // Give the batch exporter time to flush.
    tokio::time::sleep(Duration::from_secs(3)).await;
}
