use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use sprefa_schema::{list_repos, count_files_for_repo, count_refs_for_repo, search_strings};

pub struct AppState {
    pub pool: SqlitePool,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/status", get(status_handler))
        .route("/repos", get(repos_handler))
        .route("/query", get(query_handler))
        .with_state(state)
}

#[derive(Serialize)]
struct StatusResponse {
    repos: Vec<RepoStatus>,
}

#[derive(Serialize)]
struct RepoStatus {
    name: String,
    root_path: String,
    files: i64,
    refs: i64,
    scanned_at: Option<String>,
}

async fn status_handler(State(state): State<Arc<AppState>>) -> Result<Json<StatusResponse>, String> {
    let repos = list_repos(&state.pool).await.map_err(|e| e.to_string())?;
    let mut statuses = Vec::new();

    for repo in repos {
        let files = count_files_for_repo(&state.pool, repo.id).await.unwrap_or(0);
        let refs = count_refs_for_repo(&state.pool, repo.id).await.unwrap_or(0);
        statuses.push(RepoStatus {
            name: repo.name,
            root_path: repo.root_path,
            files,
            refs,
            scanned_at: repo.scanned_at,
        });
    }

    Ok(Json(StatusResponse { repos: statuses }))
}

async fn repos_handler(State(state): State<Arc<AppState>>) -> Result<Json<Vec<sprefa_schema::Repo>>, String> {
    let repos = list_repos(&state.pool).await.map_err(|e| e.to_string())?;
    Ok(Json(repos))
}

#[derive(Deserialize)]
struct QueryParams {
    q: String,
}

async fn query_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<sprefa_schema::StringRow>>, String> {
    let results = search_strings(&state.pool, &params.q)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(results))
}

/// Start the server on the given bind address.
pub async fn serve(pool: SqlitePool, bind: &str) -> anyhow::Result<()> {
    let state = Arc::new(AppState { pool });
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("sprefa daemon listening on {}", bind);
    axum::serve(listener, app).await?;
    Ok(())
}
