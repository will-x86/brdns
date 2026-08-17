use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;

use super::{ApiError, ApiState, err};
use crate::model::NewRule;

pub(crate) async fn list_rules(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let rules = state
        .cp
        .rules(&acct)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!(rules)))
}

pub(crate) async fn replace_rules(
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
    Ok(Json(serde_json::json!(rules)))
}
