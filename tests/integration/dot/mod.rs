use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::Arc;

use crate::DotServer;
use brdns::categories::CategoryIndex;
use brdns::config::{BlockResponse, Settings};
use brdns::controlplane::{ControlPlane, NoopControlPlane};
use brdns::model::{Action, NewRule, TargetType};
use brdns::protocol::header::ResultCode;
use brdns::protocol::record::{DnsRecord, QueryType};

#[tokio::test]
async fn resolves_google() {
    let srv = DotServer::start(None).await;
    let resp = srv.query("google.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NOERROR);
    assert!(resp.header.response);
    assert!(!resp.answers.is_empty());
}

#[tokio::test]
async fn nxdomain_for_bogus_name() {
    let srv = DotServer::start(None).await;
    let resp = srv
        .query("definitely-not-real-92417.example", QueryType::A)
        .await
        .unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NXDOMAIN);
}

#[tokio::test]
async fn echoes_question() {
    let srv = DotServer::start(None).await;
    let resp = srv.query("github.com", QueryType::A).await.unwrap();
    assert_eq!(resp.questions.len(), 1);
    assert_eq!(resp.questions[0].name, "github.com");
    assert_eq!(resp.questions[0].qtype, QueryType::A);
}

#[tokio::test]
async fn aaaa_query() {
    let srv = DotServer::start(None).await;
    let resp = srv.query("google.com", QueryType::AAAA).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NOERROR);
    assert!(resp.header.response);
}

#[tokio::test]
async fn deny_rule_synthesizes_nxdomain() {
    // SNI is "localhost", which does not match the base domain, so the server
    // uses the fallback account "default".
    let cp = NoopControlPlane::default();
    cp.create_account("default").await.unwrap();
    cp.replace_rules(
        "default",
        &[NewRule {
            action: Action::Deny,
            target_type: TargetType::Domain,
            target_value: "example.com".into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        }],
    )
    .await
    .unwrap();

    let srv = DotServer::start_with(None, Arc::new(cp)).await;
    let resp = srv.query("example.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NXDOMAIN);
    assert!(resp.header.response);
    assert!(resp.answers.is_empty());
}

#[tokio::test]
async fn deny_category_blocks_domains_in_category() {
    let cp = NoopControlPlane::default();
    cp.create_account("default").await.unwrap();
    cp.replace_rules(
        "default",
        &[NewRule {
            action: Action::Deny,
            target_type: TargetType::Category,
            target_value: "ads".into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        }],
    )
    .await
    .unwrap();

    let categories = Arc::new(CategoryIndex::new());
    categories.replace(HashMap::from([(
        "ads.example.com".to_string(),
        HashSet::from(["ads".to_string()]),
    )]));

    let srv = DotServer::start_full(None, Arc::new(cp), categories).await;
    let resp = srv.query("ads.example.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NXDOMAIN);
    assert!(resp.answers.is_empty());
}

#[tokio::test]
async fn null_block_response_returns_zero_addr() {
    let mut settings = Settings::default();
    settings.policy.block_response = BlockResponse::Null;

    let cp = NoopControlPlane::default();
    cp.create_account("default").await.unwrap();
    cp.replace_rules(
        "default",
        &[NewRule {
            action: Action::Deny,
            target_type: TargetType::Domain,
            target_value: "example.com".into(),
            limit_count: None,
            limit_window: None,
            enabled: true,
        }],
    )
    .await
    .unwrap();

    let srv = DotServer::start_with(Some(&settings), Arc::new(cp)).await;
    let resp = srv.query("example.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NOERROR);
    assert_eq!(resp.answers.len(), 1);
    assert_eq!(
        resp.answers[0],
        DnsRecord::A {
            domain: "example.com".into(),
            addr: Ipv4Addr::UNSPECIFIED,
            ttl: 60,
        }
    );
}
