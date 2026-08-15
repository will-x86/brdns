use crate::DohServer;
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
