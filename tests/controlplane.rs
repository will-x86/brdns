//! Integration tests for the control-plane management API (in-memory backend).

use std::sync::Arc;

use brdns::controlplane::{self, http};
use brdns::model::{Action, NewRule, TargetType};
use serde_json::{Value, json};

const TOKEN: &str = "test-token";

async fn spawn_api() -> String {
    spawn_api_with_token(Some(TOKEN.into())).await
}

async fn spawn_api_with_token(token: Option<String>) -> String {
    let cp = controlplane::init(None).await.expect("build control plane");
    let state = Arc::new(http::ApiState { cp, token });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, http::router(state))
            .await
            .expect("serve");
    });
    format!("http://{addr}")
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn healthz() {
    let base = spawn_api().await;
    let resp: Value = client()
        .get(format!("{base}/healthz"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp["status"], "ok");
}

#[tokio::test]
async fn api_requires_bearer_token() {
    let base = spawn_api().await;

    // No token -> 401.
    let resp = client()
        .get(format!("{base}/api/upstreams/presets"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Wrong token -> 401.
    let resp = client()
        .get(format!("{base}/api/upstreams/presets"))
        .header("Authorization", "Bearer wrong")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Correct token -> 200.
    let resp = client()
        .get(format!("{base}/api/upstreams/presets"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn api_disabled_without_token() {
    let base = spawn_api_with_token(None).await;
    let resp = client()
        .get(format!("{base}/api/upstreams/presets"))
        .header("Authorization", "Bearer anything")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn create_and_manage_account() {
    let base = spawn_api().await;
    let client = client();

    // Invalid account number is rejected.
    let resp = client
        .post(format!("{base}/api/accounts"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .json(&json!({ "account_number": "bad account!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Explicit account number.
    let resp = client
        .post(format!("{base}/api/accounts"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .json(&json!({ "account_number": "1234567890123456" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let acct: Value = resp.json().await.unwrap();
    assert_eq!(acct["account_number"], "1234567890123456");

    // Auto-generated account number (16 digits).
    let resp = client
        .post(format!("{base}/api/accounts"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let acct: Value = resp.json().await.unwrap();
    let number = acct["account_number"].as_str().unwrap();
    assert_eq!(number.len(), 16);
    assert!(number.chars().all(|c| c.is_ascii_digit()));

    // Replace rules.
    let rules = vec![NewRule {
        action: Action::Deny,
        target_type: TargetType::Category,
        target_value: "ads".into(),
        limit_count: None,
        limit_window: None,
        enabled: true,
    }];
    let resp = client
        .put(format!("{base}/api/accounts/1234567890123456/rules"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .json(&rules)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let stored: Value = resp.json().await.unwrap();
    assert_eq!(stored.as_array().unwrap().len(), 1);
    assert_eq!(stored[0]["action"], "deny");

    // Read them back.
    let resp = client
        .get(format!("{base}/api/accounts/1234567890123456/rules"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    let stored: Value = resp.json().await.unwrap();
    assert_eq!(stored.as_array().unwrap().len(), 1);

    // Presets exist.
    let resp = client
        .get(format!("{base}/api/upstreams/presets"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    let presets: Value = resp.json().await.unwrap();
    assert_eq!(presets.as_array().unwrap().len(), 4);

    // Unknown account 404.
    let resp = client
        .get(format!("{base}/api/accounts/0000000000000000"))
        .header("Authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
