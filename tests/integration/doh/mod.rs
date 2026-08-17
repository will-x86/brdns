use std::sync::Arc;

use crate::DohServer;
use brdns::controlplane::{ControlPlane, InMemControlPlane};
use brdns::model::{Action, NewRule, TargetType};
use brdns::protocol::header::ResultCode;
use brdns::protocol::record::QueryType;

#[tokio::test]
async fn resolves_google() {
    let srv = DohServer::start(None).await;
    let resp = srv.query("google.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NOERROR);
    assert!(resp.header.response);
    assert!(!resp.answers.is_empty());
}

#[tokio::test]
async fn echoes_question() {
    let srv = DohServer::start(None).await;
    let resp = srv.query("github.com", QueryType::A).await.unwrap();
    assert_eq!(resp.questions.len(), 1);
    assert_eq!(resp.questions[0].name, "github.com");
}

#[tokio::test]
async fn deny_rule_synthesizes_nxdomain() {
    let cp = InMemControlPlane::default();
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

    let srv = DohServer::start_with(None, Arc::new(cp)).await;
    let resp = srv.query("example.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NXDOMAIN);
    assert!(resp.header.response);
    assert!(resp.answers.is_empty());
}
