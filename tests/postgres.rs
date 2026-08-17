//! Postgres control-plane tests. Skipped unless `BRDNS_TEST_DATABASE_URL` is set
//! (they need a reachable Postgres instance).

use brdns::controlplane::ControlPlane;
use brdns::controlplane::postgres::PostgresControlPlane;
use brdns::model::{Action, NewRule, NewUpstream, TargetType, UpstreamProtocol, Window};

fn test_url() -> Option<String> {
    match std::env::var("BRDNS_TEST_DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => {
            eprintln!("skipping Postgres tests (set BRDNS_TEST_DATABASE_URL to run)");
            None
        }
    }
}

#[tokio::test]
async fn postgres_roundtrip() {
    let Some(url) = test_url() else { return };
    let cp = PostgresControlPlane::connect(&url)
        .await
        .expect("connect + migrate");

    // Unique account per run.
    let account = format!(
        "{:016}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
            % 10_000_000_000_000_000
    );

    let acct = cp.create_account(&account).await.expect("create_account");
    assert_eq!(acct.account_number, account);

    // Replace rules (out of order: deny category, allow domain, limit youtube).
    let rules = vec![
        NewRule {
            action: Action::Allow,
            target_type: TargetType::Domain,
            target_value: "example.com".into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        },
        NewRule {
            action: Action::Deny,
            target_type: TargetType::Category,
            target_value: "ads".into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        },
        NewRule {
            action: Action::Limit,
            target_type: TargetType::Category,
            target_value: "youtube".into(),
            limit_count: Some(10_000),
            limit_window: Some(Window::Month),
            enabled: true,
        },
    ];
    let stored = cp
        .replace_rules(&account, &rules)
        .await
        .expect("replace_rules");
    assert_eq!(stored.len(), 3);

    // Order is preserved (first match wins).
    let read = cp.rules(&account).await.expect("rules");
    assert_eq!(read.len(), 3);
    assert_eq!(read[0].position, 0);
    assert_eq!(read[0].action, Action::Allow);
    assert_eq!(read[2].action, Action::Limit);
    assert_eq!(read[2].limit_count, Some(10_000));
    assert_eq!(read[2].limit_window, Some(Window::Month));

    // Replace upstreams.
    let ups = vec![NewUpstream {
        name: "custom-dot".into(),
        protocol: UpstreamProtocol::Dot,
        host: "1.2.3.4".into(),
        port: 853,
        addr: None,
    }];
    let stored = cp
        .replace_upstreams(&account, &ups)
        .await
        .expect("replace_upstreams");
    assert_eq!(stored.len(), 1);
    assert!(!stored[0].is_preset);

    let active = cp
        .active_upstream(&account)
        .await
        .expect("active_upstream")
        .expect("some upstream");
    assert_eq!(active.host, "1.2.3.4");
    assert_eq!(active.protocol, UpstreamProtocol::Dot);

    // Presets seeded by connect().
    let presets = cp.preset_upstreams().await.expect("presets");
    assert!(!presets.is_empty());
    assert!(presets.iter().all(|p| p.is_preset));

    // Quota: the limit rule has budget 2 per month.
    let limit_rule = &read[2];
    let w = Window::Month;
    assert!(
        cp.record_quota(&account, limit_rule.id, 2, w)
            .await
            .unwrap()
    );
    assert!(
        cp.record_quota(&account, limit_rule.id, 2, w)
            .await
            .unwrap()
    );
    assert!(
        !cp.record_quota(&account, limit_rule.id, 2, w)
            .await
            .unwrap()
    );

    // Poll snapshot includes the account's rules + upstream.
    let snap = cp.snapshot().await.unwrap();
    let policy = snap.get(&account).expect("account in snapshot");
    assert_eq!(policy.rules.len(), 3);
    assert_eq!(policy.upstream.as_ref().unwrap().host, "1.2.3.4");

    // Unknown account yields no rules/upstream.
    assert!(cp.rules("0000000000000000").await.unwrap().is_empty());
    assert!(
        cp.active_upstream("0000000000000000")
            .await
            .unwrap()
            .is_none()
    );

    // Global category map roundtrip.
    let mut cats = std::collections::HashMap::new();
    cats.insert(
        "ads.example.com".to_string(),
        std::collections::HashSet::from(["ads".to_string()]),
    );
    cp.replace_categories(&cats).await.unwrap();
    let read_cats = cp.categories().await.unwrap();
    assert!(read_cats["ads.example.com"].contains("ads"));
}
