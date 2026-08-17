use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{get, post},
};

use super::ApiState;
use super::accounts::{create_account, get_account};
use super::auth::auth;
use super::healthz::healthz;
use super::rules::{list_rules, replace_rules};
use super::upstreams::{list_presets, list_upstreams, replace_upstreams};

pub fn router(state: Arc<ApiState>) -> Router {
    let authed = Router::new()
        .route("/api/accounts", post(create_account))
        .route("/api/accounts/{acct}", get(get_account))
        .route(
            "/api/accounts/{acct}/rules",
            get(list_rules).put(replace_rules),
        )
        .route(
            "/api/accounts/{acct}/upstreams",
            get(list_upstreams).put(replace_upstreams),
        )
        .route("/api/upstreams/presets", get(list_presets))
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new()
        .route("/healthz", get(healthz))
        .merge(authed)
        .with_state(state)
}

pub async fn serve(state: Arc<ApiState>, addr: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(state)).await
}
