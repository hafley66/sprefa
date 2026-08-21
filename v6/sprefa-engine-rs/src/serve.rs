// @comment-ok: module contract, and the rx lowering every served construct owes.
// The compiled program as an HTTP API on a socket file: one Router, one
// listener, one shutdown token, the shape src/daemon/shell/http.rs:117-142
// carries in v5.
//
// rx lowering: arrivals$.pipe(concatMap(batch => driver.tick(batch))), reads a
// latest projection. The engine thread IS the concatMap: arrival batches and
// reads cross one queue in order, so a read answers the pre-tick or the
// post-tick state and never a half-folded one.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::driver::format_deltas;
use crate::program::GenProgram;
use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{
    bytes_to_base64, Arrival, ArrivalSign, RelDelta, Row, RowColumnType, SqlStatement, TickDeltas,
    Value,
};

// The whole per-tick history a follower may fall behind by. Further back than
// this is an error, never a silent gap: those ticks are gone.
const DELTA_RING_TICKS: usize = 256;

// A long poll parks at most this long, then answers empty at the current tick.
const DELTA_POLL_WAIT: Duration = Duration::from_secs(30);

// The arrival batch as it crosses the wire: the schedule.json line shape the
// harness already reads, so one decoder serves the file door and the socket.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ArrivalDto {
    pub rel: String,
    pub sign: String,
    pub row: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum ServeError {
    NoSuchRel(String),
    UnknownSign(String),
    Tick(String),
    Sql(String),
    DeltasDropped { since: usize, oldest: usize },
    EngineGone,
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeError::NoSuchRel(rel) => write!(f, "no rel named {rel}"),
            ServeError::UnknownSign(sign) => write!(f, "arrival sign {sign} is not add or del"),
            ServeError::Tick(detail) => write!(f, "tick fold stopped: {detail}"),
            ServeError::Sql(detail) => write!(f, "relation read stopped: {detail}"),
            ServeError::DeltasDropped { since, oldest } => write!(
                f,
                "deltas since tick {since} left the ring, oldest tick kept is {oldest}"
            ),
            ServeError::EngineGone => write!(f, "the engine thread is gone"),
        }
    }
}

impl std::error::Error for ServeError {}

impl ServeError {
    fn status(&self) -> StatusCode {
        match self {
            ServeError::NoSuchRel(_) => StatusCode::NOT_FOUND,
            ServeError::UnknownSign(_) => StatusCode::BAD_REQUEST,
            ServeError::Tick(_) | ServeError::Sql(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ServeError::DeltasDropped { .. } => StatusCode::GONE,
            ServeError::EngineGone => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl IntoResponse for ServeError {
    fn into_response(self) -> Response {
        (self.status(), Json(json!({ "error": self.to_string() }))).into_response()
    }
}

pub type ServeResult = Result<serde_json::Value, ServeError>;

enum EngineJob {
    Arrive(Vec<Arrival>, oneshot::Sender<ServeResult>),
    Read(String, oneshot::Sender<ServeResult>),
}

// What each tick wrote, kept DELTA_RING_TICKS deep. `dropped_through` is the
// last tick evicted, so a caller from before it is told, never given a hole.
struct DeltaRing {
    ticks: VecDeque<(usize, Vec<RelDelta>)>,
    latest: usize,
    dropped_through: usize,
}

impl DeltaRing {
    fn new(ticks_folded: usize) -> DeltaRing {
        DeltaRing {
            ticks: VecDeque::new(),
            latest: ticks_folded,
            dropped_through: ticks_folded,
        }
    }

    fn push(&mut self, tick: usize, deltas: &TickDeltas) {
        let written: Vec<RelDelta> = deltas
            .rels
            .iter()
            .filter(|delta| !delta.add.is_empty() || !delta.del.is_empty())
            .cloned()
            .collect();
        self.ticks.push_back((tick, written));
        self.latest = tick;
        while self.ticks.len() > DELTA_RING_TICKS {
            if let Some((evicted, _)) = self.ticks.pop_front() {
                self.dropped_through = evicted;
            }
        }
    }

    // Every tick after `since` folded into one add/del pair: a follower that
    // missed four ticks reads their union, in tick order.
    fn since(&self, rel: &str, since: usize) -> Result<(Vec<Row>, Vec<Row>), ServeError> {
        if since < self.dropped_through {
            return Err(ServeError::DeltasDropped {
                since,
                oldest: self.dropped_through + 1,
            });
        }
        let mut add = Vec::new();
        let mut del = Vec::new();
        for (tick, written) in self.ticks.iter().filter(|(tick, _)| *tick > since) {
            let _ = tick;
            for delta in written.iter().filter(|delta| delta.rel == rel) {
                add.extend(delta.add.iter().cloned());
                del.extend(delta.del.iter().cloned());
            }
        }
        Ok((add, del))
    }
}

#[derive(Clone)]
pub struct ServeState {
    name: Arc<str>,
    jobs: mpsc::UnboundedSender<EngineJob>,
    rels: Arc<HashSet<String>>,
    column_types: Arc<HashMap<String, Vec<RowColumnType>>>,
    ring: Arc<Mutex<DeltaRing>>,
    ticked: watch::Receiver<usize>,
}

impl ServeState {
    // The caller runs the DDL and the boot statements (driver::run_schedule
    // does both) before the program reaches the socket.
    pub fn spawn(program: GenProgram, seam: SqliteSeam) -> ServeState {
        ServeState::resume(program, seam, 0)
    }

    // A socket opened over an already-folded seam keeps counting that fold's
    // ticks, so one program has one tick sequence across both doors.
    pub fn resume(program: GenProgram, seam: SqliteSeam, ticks_folded: usize) -> ServeState {
        let name: Arc<str> = Arc::from(program.name.as_str());
        let rels: HashSet<String> = program.final_select.keys().cloned().collect();
        let column_types = program.rel_column_types.clone();
        let (jobs, inbox) = mpsc::unbounded_channel();
        let ring = Arc::new(Mutex::new(DeltaRing::new(ticks_folded)));
        let (ticked_tx, ticked) = watch::channel(ticks_folded);
        let engine_ring = Arc::clone(&ring);
        std::thread::spawn(move || {
            engine_loop(program, seam, inbox, ticks_folded, engine_ring, ticked_tx)
        });
        ServeState {
            name,
            jobs,
            rels: Arc::new(rels),
            column_types: Arc::new(column_types),
            ring,
            ticked,
        }
    }

    pub async fn arrive(&self, arrivals: Vec<Arrival>) -> ServeResult {
        self.ask(|reply| EngineJob::Arrive(arrivals, reply)).await
    }

    pub async fn read_rel(&self, rel: &str) -> ServeResult {
        let rel = rel.to_string();
        self.ask(|reply| EngineJob::Read(rel, reply)).await
    }

    // The long poll: deltas already past `since` answer now, otherwise the call
    // parks on the next tick and DELTA_POLL_WAIT closes it out with an empty.
    pub async fn deltas_since(&self, rel: &str, since: usize) -> ServeResult {
        if !self.rels.contains(rel) {
            return Err(ServeError::NoSuchRel(rel.to_string()));
        }
        let types = self.column_types.get(rel).cloned().unwrap_or_default();
        let mut ticked = self.ticked.clone();
        let deadline = tokio::time::Instant::now() + DELTA_POLL_WAIT;
        loop {
            let (tick, add, del) = {
                let ring = self.ring.lock().map_err(|_| ServeError::EngineGone)?;
                let (add, del) = ring.since(rel, since)?;
                (ring.latest, add, del)
            };
            if !add.is_empty() || !del.is_empty() || tokio::time::Instant::now() >= deadline {
                return Ok(delta_answer(tick, &types, add, del));
            }
            if tokio::time::timeout_at(deadline, ticked.changed())
                .await
                .is_err()
            {
                let ring = self.ring.lock().map_err(|_| ServeError::EngineGone)?;
                let (add, del) = ring.since(rel, since)?;
                return Ok(delta_answer(ring.latest, &types, add, del));
            }
        }
    }

    async fn ask<F>(&self, job: F) -> ServeResult
    where
        F: FnOnce(oneshot::Sender<ServeResult>) -> EngineJob,
    {
        let (reply, answer) = oneshot::channel();
        self.jobs
            .send(job(reply))
            .map_err(|_| ServeError::EngineGone)?;
        answer.await.map_err(|_| ServeError::EngineGone)?
    }
}

// Rows cross as arrays in the rel's column order, decoded by the same typing
// GET /rel/{name} gives each column.
fn delta_answer(
    tick: usize,
    types: &[RowColumnType],
    add: Vec<Row>,
    del: Vec<Row>,
) -> serde_json::Value {
    json!({ "tick": tick, "add": rows_json(types, add), "del": rows_json(types, del) })
}

fn rows_json(types: &[RowColumnType], rows: Vec<Row>) -> Vec<serde_json::Value> {
    let mut encoded: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::Value::Array(
                row.iter()
                    .enumerate()
                    .map(|(index, value)| value_json(value, types.get(index).copied()))
                    .collect(),
            )
        })
        .collect();
    encoded.sort_by_key(|row| row.to_string());
    encoded
}

fn engine_loop(
    program: GenProgram,
    seam: SqliteSeam,
    mut inbox: mpsc::UnboundedReceiver<EngineJob>,
    ticks_folded: usize,
    ring: Arc<Mutex<DeltaRing>>,
    ticked: watch::Sender<usize>,
) {
    let mut tick = ticks_folded;
    while let Some(job) = inbox.blocking_recv() {
        match job {
            EngineJob::Arrive(arrivals, reply) => {
                let answer = match program.run_tick(&seam, &arrivals) {
                    Ok(deltas) => {
                        tick += 1;
                        if let Ok(mut ring) = ring.lock() {
                            ring.push(tick, &deltas);
                        }
                        let line = format_deltas(&program, tick, &deltas);
                        let _ = ticked.send(tick);
                        serde_json::from_str(&line).map_err(|e| ServeError::Tick(e.to_string()))
                    }
                    Err(failure) => Err(ServeError::Tick(failure.to_string())),
                };
                let _ = reply.send(answer);
            }
            EngineJob::Read(rel, reply) => {
                let _ = reply.send(read_rel(&program, &seam, &rel));
            }
        }
    }
}

// final_select is the program's own decoded read. Column names ride along so a
// row body deserializes into the struct the program's .types.rs declares.
fn read_rel(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> ServeResult {
    let select = program
        .final_select
        .get(rel)
        .ok_or_else(|| ServeError::NoSuchRel(rel.to_string()))?;
    let columns = program.rel_columns.get(rel).cloned().unwrap_or_default();
    let types = program
        .rel_column_types
        .get(rel)
        .cloned()
        .unwrap_or_default();
    let result = seam
        .execute(&SqlStatement {
            sql: select.clone(),
            args: vec![],
        })
        .map_err(|failure| ServeError::Sql(failure.to_string()))?;
    let mut rows: Vec<serde_json::Value> = result
        .rows
        .iter()
        .map(|row| row_object(&columns, &types, row))
        .collect();
    rows.sort_by_key(|row| row.to_string());
    Ok(json!({ "rel": rel, "columns": columns, "rows": rows }))
}

fn row_object(columns: &[String], types: &[RowColumnType], row: &[Value]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (index, value) in row.iter().enumerate() {
        let column = match columns.get(index) {
            Some(name) => name.clone(),
            None => format!("col{}", index + 1),
        };
        object.insert(column, value_json(value, types.get(index).copied()));
    }
    serde_json::Value::Object(object)
}

fn value_json(value: &Value, column_type: Option<RowColumnType>) -> serde_json::Value {
    match value {
        Value::Integer(v) => json!(v),
        Value::Real(v) => json!(v),
        Value::Bool(b) => json!(b),
        Value::List(items) => serde_json::Value::Array(items.clone()),
        Value::Bytes(bytes) => json!(bytes_to_base64(bytes)),
        Value::Text(text) => {
            if column_type == Some(RowColumnType::Json) || column_type == Some(RowColumnType::Ref) {
                serde_json::from_str(text).unwrap_or_else(|_| json!(text))
            } else {
                json!(text)
            }
        }
    }
}

// Byte-for-byte the harness's file-schedule decoding: a null column crosses as
// empty text and a nested document as its own text.
pub fn value_from_json(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => Value::Integer(integer),
            None => Value::Real(number.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::String(text) => Value::Text(text.clone()),
        serde_json::Value::Null => Value::Text(String::new()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Value::Text(value.to_string())
        }
    }
}

pub fn arrival_batch(batch: Vec<ArrivalDto>) -> Result<Vec<Arrival>, ServeError> {
    batch.into_iter().map(arrival_from_dto).collect()
}

fn arrival_from_dto(dto: ArrivalDto) -> Result<Arrival, ServeError> {
    let sign = match dto.sign.as_str() {
        "add" => ArrivalSign::Add,
        "del" => ArrivalSign::Del,
        other => return Err(ServeError::UnknownSign(other.to_string())),
    };
    Ok(Arrival {
        rel: dto.rel,
        sign,
        row: dto.row.iter().map(value_from_json).collect(),
    })
}

pub fn router(state: ServeState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/rel/{name}", get(rel_route))
        .route("/rel/{name}/deltas", get(deltas_route))
        .route("/arrive", post(arrive_route))
        .with_state(state)
}

async fn health(State(state): State<ServeState>) -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "program": state.name.as_ref() }))
}

async fn rel_route(
    State(state): State<ServeState>,
    UrlPath(rel): UrlPath<String>,
) -> Result<Json<serde_json::Value>, ServeError> {
    state.read_rel(&rel).await.map(Json)
}

// `since` is the last tick the caller already folded; 0 asks from the start.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeltasQuery {
    #[serde(default)]
    pub since: usize,
}

async fn deltas_route(
    State(state): State<ServeState>,
    UrlPath(rel): UrlPath<String>,
    Query(query): Query<DeltasQuery>,
) -> Result<Json<serde_json::Value>, ServeError> {
    state.deltas_since(&rel, query.since).await.map(Json)
}

async fn arrive_route(
    State(state): State<ServeState>,
    Json(batch): Json<Vec<ArrivalDto>>,
) -> Result<Json<serde_json::Value>, ServeError> {
    let arrivals = arrival_batch(batch)?;
    state.arrive(arrivals).await.map(Json)
}

// A second bind on a live socket file is an error, never a silent takeover:
// the resident process stays authoritative.
pub fn bind(socket: &Path) -> anyhow::Result<tokio::net::UnixListener> {
    tokio::net::UnixListener::bind(socket)
        .with_context(|| format!("bind unix socket {}", socket.display()))
}

pub async fn serve_on(
    state: ServeState,
    listener: tokio::net::UnixListener,
    socket: &Path,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let served = axum::serve(listener, router(state))
        .with_graceful_shutdown(async move { cancel.cancelled().await })
        .await;
    let _ = std::fs::remove_file(socket);
    served.with_context(|| format!("uds server on {} exited", socket.display()))
}

pub async fn serve_unix(
    state: ServeState,
    socket: &Path,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let listener = bind(socket)?;
    serve_on(state, listener, socket, cancel).await
}
