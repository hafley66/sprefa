use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use std::sync::Arc;

use crate::_9_wire::models::body_rejected::BodyRejected;
use crate::_9_wire::models::change::Change;
use crate::_9_wire::models::delta::Delta;
use crate::_9_wire::models::plan::Plan;
use crate::_9_wire::models::query_request::QueryRequest;
use crate::_9_wire::models::recipe::Recipe;
use crate::_9_wire::models::server_error::ServerError;
use crate::_9_wire::models::table::Table;
use crate::_9_wire::ops_auto::CommitArgs;
use crate::_9_wire::ops_auto::OpError;
use crate::_9_wire::ops_auto::OpResult;
use crate::_9_wire::ops_auto::PlanArgs;
use crate::_9_wire::ops_auto::QueryArgs;
use crate::_9_wire::ops_auto::RecipeArgs;
use crate::_9_wire::ops_auto::Wire;

impl IntoResponse for OpError {
    fn into_response(self) -> Response {
        match self {
            OpError::Internal(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ServerError { error })).into_response(),
            OpError::CommitRefused(body) => (StatusCode::from_u16(400).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(body)).into_response(),
            OpError::BodyRejected(body) => (StatusCode::from_u16(422).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(body)).into_response(),
            OpError::ServerError(body) => (StatusCode::from_u16(500).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(body)).into_response(),
            OpError::SqlRejected(body) => (StatusCode::from_u16(400).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(body)).into_response(),
        }
    }
}

pub async fn commit<S: Wire>(State(ops): State<Arc<S>>, body: Result<Json<Vec<Change>>, JsonRejection>) -> OpResult<Json<Delta>> {
  let Json(body) = body.map_err(|rejection| OpError::BodyRejected(BodyRejected { error: rejection.body_text() }))?;
  let args = CommitArgs {
      changes: body,
  };
  let out = tokio::task::spawn_blocking(move || ops.commit(&args)).await??;
  Ok(Json(out))
}

pub async fn query<S: Wire>(State(ops): State<Arc<S>>, body: Result<Json<QueryRequest>, JsonRejection>) -> OpResult<Json<Table>> {
  let Json(body) = body.map_err(|rejection| OpError::BodyRejected(BodyRejected { error: rejection.body_text() }))?;
  let args = QueryArgs {
      request: body,
  };
  let out = tokio::task::spawn_blocking(move || ops.query(&args)).await??;
  Ok(Json(out))
}

pub async fn plan<S: Wire>(State(ops): State<Arc<S>>) -> OpResult<Json<Plan>> {
  let args = PlanArgs {

  };
  let out = tokio::task::spawn_blocking(move || ops.plan(&args)).await??;
  Ok(Json(out))
}

pub async fn recipe<S: Wire>(State(ops): State<Arc<S>>) -> OpResult<Json<Recipe>> {
  let args = RecipeArgs {

  };
  let out = tokio::task::spawn_blocking(move || ops.recipe(&args)).await??;
  Ok(Json(out))
}

pub fn router<S: Wire>(state: Arc<S>) -> axum::Router {
  axum::Router::new()
      .route("/commit", post(commit::<S>))
      .route("/query", post(query::<S>))
      .route("/plan", get(plan::<S>))
      .route("/recipe", get(recipe::<S>))
      .with_state(state)
}
