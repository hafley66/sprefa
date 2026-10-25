use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum_extra::extract::Query;
use serde::Deserialize;

use crate::models::change::Change;
use crate::models::delta::Delta;
use crate::models::plan::Plan;
use crate::models::query_request::QueryRequest;
use crate::models::recipe::Recipe;
use crate::models::table::Table;
use crate::ops_auto::CommitArgs;
use crate::ops_auto::OpError;
use crate::ops_auto::OpResult;
use crate::ops_auto::PlanArgs;
use crate::ops_auto::QueryArgs;
use crate::ops_auto::RecipeArgs;

impl IntoResponse for OpError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": self.0}))).into_response()
    }
}

pub async fn commit(Json(body): Json<Vec<Change>>) -> OpResult<Json<Delta>> {
  let args = CommitArgs {
      changes: body,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::commit(&args)).await??;
  Ok(Json(out))
}

pub async fn query(Json(body): Json<QueryRequest>) -> OpResult<Json<Table>> {
  let args = QueryArgs {
      request: body,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::query(&args)).await??;
  Ok(Json(out))
}

pub async fn plan() -> OpResult<Json<Plan>> {
  let args = PlanArgs {

  };
  let out = tokio::task::spawn_blocking(move || crate::ops::plan(&args)).await??;
  Ok(Json(out))
}

pub async fn recipe() -> OpResult<Json<Recipe>> {
  let args = RecipeArgs {

  };
  let out = tokio::task::spawn_blocking(move || crate::ops::recipe(&args)).await??;
  Ok(Json(out))
}

pub fn router() -> axum::Router {
  axum::Router::new()
      .route("/commit", post(commit))
      .route("/query", post(query))
      .route("/plan", get(plan))
      .route("/recipe", get(recipe))
}
