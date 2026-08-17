use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;

use super::{ApiError, ApiState, err};
use crate::model::NewUpstream;

pub(crate) async fn list_upstreams(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ups = state
        .cp
        .list_upstreams(&acct)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!(ups)))
}

pub(crate) async fn replace_upstreams(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
    Json(body): Json<Vec<NewUpstream>>,
) -> Result<Json<Value>, ApiError> {
    let ups = state
        .cp
        .replace_upstreams(&acct, &body)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!(ups)))
}

pub(crate) async fn list_presets(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<Value>, ApiError> {
    let presets = state
        .cp
        .preset_upstreams()
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!(presets)))
}
