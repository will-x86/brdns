use crate::DotServer;
use brdns::protocol::header::ResultCode;
use brdns::protocol::record::QueryType;

#[tokio::test]
async fn resolves_google() {
    let srv = DotServer::start().await;
    let resp = srv.query("google.com", QueryType::A).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NOERROR);
    assert!(resp.header.response);
    assert!(!resp.answers.is_empty());
}

#[tokio::test]
async fn nxdomain_for_bogus_name() {
    let srv = DotServer::start().await;
    let resp = srv
        .query("definitely-not-real-92417.example", QueryType::A)
        .await
        .unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NXDOMAIN);
}

#[tokio::test]
async fn echoes_question() {
    let srv = DotServer::start().await;
    let resp = srv.query("github.com", QueryType::A).await.unwrap();
    assert_eq!(resp.questions.len(), 1);
    assert_eq!(resp.questions[0].name, "github.com");
    assert_eq!(resp.questions[0].qtype, QueryType::A);
}

#[tokio::test]
async fn aaaa_query() {
    let srv = DotServer::start().await;
    let resp = srv.query("google.com", QueryType::AAAA).await.unwrap();
    assert_eq!(resp.header.rescode, ResultCode::NOERROR);
    assert!(resp.header.response);
}