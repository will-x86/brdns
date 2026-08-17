//! Control-plane management API (axum).
//!
//! Thin JSON routes over the [`ControlPlane`] trait; the friendly CLI/UX
//! (step 14) is built on top of this.
//!
//! Every `/api/*` route requires `Authorization: Bearer <token>`. With no
//! token configured, the API refuses all management requests (secure default).

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};

use super::ControlPlane;
use crate::model::{
    NewAccount, NewRule, NewUpstream, generate_account_number, is_valid_account_number,
};

pub struct ApiState {
    pub cp: Arc<dyn ControlPlane>,
    pub token: Option<String>,
}

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

/// Run the management API until the process shuts down.
pub async fn serve(state: Arc<ApiState>, addr: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(state)).await
}

type ApiError = (StatusCode, Json<Value>);

fn err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": msg.into() })))
}

/// Constant-time comparison for bearer tokens.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn auth(State(state): State<Arc<ApiState>>, req: Request<Body>, next: Next) -> Response {
    let Some(expected) = state.token.as_deref() else {
        return err(
            StatusCode::UNAUTHORIZED,
            "management API disabled: no api_token configured",
        )
        .into_response();
    };

    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()))
        .unwrap_or(false);

    if authorized {
        next.run(req).await
    } else {
        err(StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn create_account(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<NewAccount>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let number = if body.account_number.is_empty() {
        generate_account_number()
    } else {
        body.account_number
    };
    if !is_valid_account_number(&number) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "account_number must be 1-64 chars of [a-zA-Z0-9-]",
        ));
    }
    let account = state
        .cp
        .create_account(&number)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(json!(account))))
}

async fn get_account(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state.cp.get_account(&acct).await {
        Ok(Some(account)) => Ok(Json(json!(account))),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "account not found")),
        Err(e) => Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn list_rules(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let rules = state
        .cp
        .rules(&acct)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(rules)))
}

async fn replace_rules(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
    Json(body): Json<Vec<NewRule>>,
) -> Result<Json<Value>, ApiError> {
    if body.iter().any(|r| {
        r.target_value.is_empty()
            || r.target_value.len() > 253
            || r.limit_count.is_some_and(|c| c < 0)
    }) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid rule: target_value must be 1-253 chars; limit_count >= 0",
        ));
    }
    let rules = state
        .cp
        .replace_rules(&acct, &body)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(rules)))
}

async fn list_upstreams(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ups = state
        .cp
        .list_upstreams(&acct)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(ups)))
}

async fn replace_upstreams(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
    Json(body): Json<Vec<NewUpstream>>,
) -> Result<Json<Value>, ApiError> {
    let ups = state
        .cp
        .replace_upstreams(&acct, &body)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(ups)))
}

async fn list_presets(State(state): State<Arc<ApiState>>) -> Result<Json<Value>, ApiError> {
    let presets = state
        .cp
        .preset_upstreams()
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!(presets)))
}
