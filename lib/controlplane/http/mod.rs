//! Control-plane API (axum).
//!
//! Thin JSON routes over the [`ControlPlane`] trait
//!
//! Every `/api/*` route requires `Authorization: Bearer <token>`. With no
//! token configured, the API refuses all management requests.

mod accounts;
mod auth;
mod healthz;
mod router;
mod rules;
mod upstreams;

use std::sync::Arc;

use axum::Json;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::controlplane::ControlPlane;

pub use router::{router, serve};

pub struct ApiState {
    pub cp: Arc<dyn ControlPlane>,
    pub token: Option<String>,
}

pub(crate) type ApiError = (StatusCode, Json<Value>);

pub(crate) fn err(status: StatusCode, msg: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": msg.into() })))
}
