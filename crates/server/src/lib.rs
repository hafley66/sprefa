use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use sprefa_config::RepoConfig;
use sprefa_scan::Scanner;
use sprefa_schema::{
    BranchScope, QueryHit, list_repos, count_files_for_repo, count_refs_for_repo, search_refs,
};

/// How long a queued POST /scan will wait for the current scan to finish.
const SCAN_QUEUE_TIMEOUT_SECS: u64 = 300;

pub struct AppState {
    pub pool: SqlitePool,
    /// None when the daemon was started without a rules file (scan is disabled).
    pub scanner: Option<Arc<Scanner>>,
    pub repos: Vec<RepoConfig>,
    /// Mutex used to queue concurrent scan requests. Held for the duration of a scan.
    pub scan_lock: Mutex<()>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/status", get(status_handler))
        .route("/repos", get(repos_handler))
        .route("/query", get(query_handler))
        .route("/scan", post(scan_handler))
        .with_state(state)
}

// ── /status ──────────────────────────────────────────────────────────────────

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

async fn status_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, (StatusCode, String)> {
    let repos = list_repos(&state.pool).await.map_err(e500)?;
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

// ── /repos ────────────────────────────────────────────────────────────────────

async fn repos_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<sprefa_schema::Repo>>, (StatusCode, String)> {
    let repos = list_repos(&state.pool).await.map_err(e500)?;
    Ok(Json(repos))
}

// ── /query ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct QueryParams {
    q: String,
    scope: Option<BranchScope>,
}

async fn query_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<QueryHit>>, (StatusCode, String)> {
    let results = search_refs(&state.pool, &params.q, params.scope).await.map_err(e500)?;
    Ok(Json(results))
}

// ── /scan ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct ScanBody {
    /// If set, scan only this repo. If absent, scan all configured repos.
    repo: Option<String>,
}

#[derive(Serialize)]
struct ScanResultItem {
    repo: String,
    branch: String,
    files_scanned: usize,
    refs_inserted: usize,
    targets_resolved: usize,
    links_created: usize,
}

async fn scan_handler(
    State(state): State<Arc<AppState>>,
    body: Option<Json<ScanBody>>,
) -> Result<Json<Vec<ScanResultItem>>, (StatusCode, String)> {
    let scanner = state
        .scanner
        .as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "scan not configured".to_string()))?
        .clone();

    let body = body.map(|b| b.0).unwrap_or_default();

    let repos: Vec<&RepoConfig> = state
        .repos
        .iter()
        .filter(|r| body.repo.as_ref().map(|n| &r.name == n).unwrap_or(true))
        .collect();

    if repos.is_empty() {
        if let Some(name) = &body.repo {
            return Err((StatusCode::NOT_FOUND, format!("no repo named '{name}'")));
        }
    }

    // Block until the current scan finishes, up to SCAN_QUEUE_TIMEOUT_SECS.
    // Multiple concurrent POST /scan requests queue here naturally.
    tracing::info!(phase = "lock_acquire", lock = "scan_lock", "waiting for scan lock");
    let _guard = tokio::time::timeout(
        Duration::from_secs(SCAN_QUEUE_TIMEOUT_SECS),
        state.scan_lock.lock(),
    )
    .await
    .map_err(|_| {
        tracing::warn!(phase = "lock_timeout", lock = "scan_lock", timeout_secs = SCAN_QUEUE_TIMEOUT_SECS, "scan lock timeout");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("scan queue timeout after {SCAN_QUEUE_TIMEOUT_SECS}s"),
        )
    })?;
    tracing::info!(phase = "lock_acquired", lock = "scan_lock", "scan lock acquired");

    let mut results = Vec::new();
    for repo in repos {
        for branch in repo.rev_list() {
            match scanner.scan_repo(repo, &branch).await {
                Ok(r) => results.push(ScanResultItem {
                    repo: r.repo,
                    branch: r.branch,
                    files_scanned: r.files_scanned,
                    refs_inserted: r.refs_inserted,
                    targets_resolved: r.targets_resolved,
                    links_created: r.links_created,
                }),
                Err(e) => tracing::warn!("{}/{}: scan failed: {}", repo.name, branch, e),
            }
        }
    }

    Ok(Json(results))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn e500(e: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Start the HTTP daemon.
pub async fn serve(
    pool: SqlitePool,
    scanner: Option<Arc<Scanner>>,
    repos: Vec<RepoConfig>,
    bind: &str,
) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        pool,
        scanner,
        repos,
        scan_lock: Mutex::new(()),
    });
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("sprefa daemon listening on {}", bind);
    axum::serve(listener, app).await?;
    Ok(())
}
