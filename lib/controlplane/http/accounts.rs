use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use super::{ApiError, ApiState, err};
use crate::model::{NewAccount, generate_account_number, is_valid_account_number};

pub(crate) async fn create_account(
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

pub(crate) async fn get_account(
    State(state): State<Arc<ApiState>>,
    Path(acct): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state.cp.get_account(&acct).await {
        Ok(Some(account)) => Ok(Json(json!(account))),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "account not found")),
        Err(e) => Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}
