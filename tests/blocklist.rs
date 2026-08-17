//! Network-dependent blocklist ingestion test. Skipped unless
//! `BRDNS_TEST_NETWORK=1` is set (fetches real community feeds).

use brdns::blocklist;
use brdns::categories::CategoryIndex;
use brdns::controlplane::{ControlPlane, InMemControlPlane};

#[tokio::test]
async fn ingest_real_feeds() {
    if std::env::var("BRDNS_TEST_NETWORK").is_err() {
        eprintln!("skipping network test (set BRDNS_TEST_NETWORK=1 to run)");
        return;
    }

    let cp = InMemControlPlane::default();
    let index = CategoryIndex::new();

    let n = blocklist::ingest(&cp, &index, &blocklist::default_feeds())
        .await
        .expect("ingest");

    eprintln!("ingested {n} domains");
    assert!(n > 1_000, "expected a substantial index, got {n}");

    // The feeds all tag ads; a well-known ad domain should be present.
    assert!(
        index.contains("doubleclick.net", "ads") || index.contains("adservice.google.com", "ads"),
        "expected a known ad domain in the ads category"
    );

    // Persistence round-trip through the control plane.
    assert_eq!(cp.categories().await.unwrap().len(), n);
}
