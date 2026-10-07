//! `(http.serve ?Port)` on hyper: one accept task per served port. Every
//! inbound request becomes an `http.request` row; the program answers with
//! `http.response` rows keyed by the same id, which the next handout hands
//! back to the waiting connection task. A response that has not landed ten
//! seconds after the request is a 504 written by the connection task itself.
//! A port that cannot bind answers a `serve_error` row and leaves the ports.

use super::{
    blob_bytes, body_cell, body_of_term, classify_body, handle, media_type, message, text_of,
    HttpBody,
};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::ir_eval::SignedRow;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IService};
use http_body_util::{BodyExt, Full};
use hyper::header::HeaderValue;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub const RELATION: &str = "http.serve";
pub const REQUEST: &str = "http.request";
pub const RESPONSE: &str = "http.response";
pub const ERROR: &str = "http.serve_error";

/// The 10-second law: an unanswered request is a 504, never a hang.
const RESPONSE_BUDGET: Duration = Duration::from_secs(10);

/// A request body larger than this is a 413, never a memory problem.
const BODY_CAP: usize = 16 * 1024 * 1024;

/// Connection ids are process-wide, so rows from different ports never
/// collide.
static NEXT_ID: AtomicI64 = AtomicI64::new(0);

pub struct HttpServe {
    relation: TermId,
    request: TermId,
    response: TermId,
    error: TermId,
    /// Inbound events, plain data until `poll` mints terms.
    done: Receiver<Inbound>,
    sender: Sender<Inbound>,
    /// Ports with one accept task each.
    ports: HashSet<i64>,
    /// Requests waiting for their program answer, by id.
    waiting: HashMap<i64, tokio::sync::oneshot::Sender<Answer>>,
    /// The request row each waiting id minted; retracted with its answer.
    request_rows: HashMap<i64, Row>,
    /// Request rows to retract at the next signed poll.
    retract: Vec<Row>,
}

/// One event from the accept and connection tasks. Plain data; terms are
/// minted in `poll` on the tick thread.
enum Inbound {
    Arrived {
        id: i64,
        method: String,
        path: String,
        query: String,
        request_body: HttpBody,
        answerer: tokio::sync::oneshot::Sender<Answer>,
    },
    /// The connection wrote its own 504; the request row is retracted.
    Abandoned { id: i64 },
    /// The port could not bind.
    Failed { port: i64, message: String },
}

/// One program answer: status, content type, and a body that streams from
/// the blob map when it is a handle.
struct Answer {
    status: i64,
    content_type: String,
    body: HttpBody,
}

impl Answer {
    fn into_response(self) -> Response<Full<bytes::Bytes>> {
        let status = StatusCode::from_u16(self.status.clamp(100, 599) as u16)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = match self.body {
            HttpBody::Text(text) => bytes::Bytes::from(text),
            HttpBody::Blob(handle) => blob_bytes(&handle).unwrap_or_default(),
        };
        let mut response = Response::new(Full::new(bytes));
        *response.status_mut() = status;
        if !self.content_type.is_empty() {
            response.headers_mut().insert(
                CONTENT_TYPE,
                self.content_type
                    .parse()
                    .unwrap_or_else(|_| HeaderValue::from_static("text/plain")),
            );
        }
        response
    }
}

impl HttpServe {
    pub fn new(relation: TermId, request: TermId, response: TermId, error: TermId) -> HttpServe {
        let (sender, done) = channel();
        HttpServe {
            relation,
            request,
            response,
            error,
            done,
            sender,
            ports: HashSet::new(),
            waiting: HashMap::new(),
            request_rows: HashMap::new(),
            retract: Vec::new(),
        }
    }

    /// One accept task per port: binds inside the task, then serves
    /// connections until the listener fails. The loop ends when the listener
    /// errors or the process shuts the runtime down.
    fn spawn_accept(&mut self, port: i64) {
        self.ports.insert(port);
        let sender = self.sender.clone();
        handle().spawn(async move {
            let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port as u16)).await {
                Ok(listener) => listener,
                Err(failure) => {
                    let _ = sender.send(Inbound::Failed {
                        port,
                        message: message(failure),
                    });
                    super::wake();
                    return;
                }
            };
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        let inbound = sender.clone();
                        // One task per connection; the loop ends with the
                        // connection.
                        tokio::spawn(async move {
                            let _ = http1::Builder::new()
                                .serve_connection(
                                    io,
                                    service_fn(move |request| {
                                        let inbound = inbound.clone();
                                        async move { respond(request, inbound).await }
                                    }),
                                )
                                .await;
                        });
                    }
                    Err(_) => return,
                }
            }
        });
    }

    fn error_row(&self, u: &mut Universe, port: i64, failure: &str) -> Row {
        let port = u.int(port);
        let failure = u.string(failure);
        Row {
            rel: self.error,
            args: vec![
                u.compound("const", vec![port]),
                u.compound("const", vec![failure]),
            ],
        }
    }
}

/// One connection's request: register it, wake the answering loop, and wait
/// for the program's row or the 10-second law.
async fn respond(
    request: Request<hyper::body::Incoming>,
    inbound: Sender<Inbound>,
) -> Result<Response<Full<bytes::Bytes>>, std::convert::Infallible> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().unwrap_or_default().to_string();
    let media = media_type(
        request
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
    );
    let bytes = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => bytes::Bytes::new(),
    };
    if bytes.len() > BODY_CAP {
        return Ok(plain(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body too large",
        ));
    }
    let request_body = classify_body(&media, &bytes);
    let (answerer, answer) = tokio::sync::oneshot::channel();
    let _ = inbound.send(Inbound::Arrived {
        id,
        method,
        path,
        query,
        request_body,
        answerer,
    });
    super::wake();
    match tokio::time::timeout(RESPONSE_BUDGET, answer).await {
        Ok(Ok(answer)) => Ok(answer.into_response()),
        _ => {
            let _ = inbound.send(Inbound::Abandoned { id });
            super::wake();
            Ok(plain(
                StatusCode::GATEWAY_TIMEOUT,
                "no http.response row within 10 seconds",
            ))
        }
    }
}

fn plain(status: StatusCode, text: &str) -> Response<Full<bytes::Bytes>> {
    let mut response = Response::new(Full::new(bytes::Bytes::from(text.to_string())));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    response
}

impl IService for HttpServe {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Continuing
    }

    fn call(&mut self, u: &mut Universe, rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let Some(values) = application_values(u, application) else {
                continue;
            };
            let Some(port) = values
                .first()
                .copied()
                .flatten()
                .and_then(|term| u.as_int(term))
            else {
                continue;
            };
            if self.ports.contains(&port) {
                continue;
            }
            self.spawn_accept(port);
        }
        // Every handout: hand each `http.response` row to its waiting
        // connection task and mark the request row for retraction. The scan
        // is bounded by the rows the program wrote.
        if let Some(table) = rows.table(self.response) {
            for index in 0..table.len() {
                let row = table.row(index as u32);
                // Every cell of a stored row is a `const` wrapper; the
                // unwrap is the same one Timer's clock reads through.
                let cell = |term: TermId| u.unary(term, "const");
                let Some(id) = cell(row[0]).and_then(|value| u.as_int(value)) else {
                    continue;
                };
                let Some(answerer) = self.waiting.remove(&id) else {
                    continue;
                };
                let status = cell(row[1])
                    .and_then(|value| u.as_int(value))
                    .unwrap_or(200);
                let content_type = cell(row[2])
                    .and_then(|value| text_of(u, value))
                    .unwrap_or_default();
                let body = body_of_term(u, cell(row[3]).unwrap_or(row[3]));
                let _ = answerer.send(Answer {
                    status,
                    content_type,
                    body,
                });
                if let Some(request_row) = self.request_rows.remove(&id) {
                    self.retract.push(request_row);
                }
            }
        }
    }

    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        while let Ok(event) = self.done.try_recv() {
            match event {
                Inbound::Arrived {
                    id,
                    method,
                    path,
                    query,
                    request_body,
                    answerer,
                } => {
                    let id_term = u.int(id);
                    let method_term = u.string(&method);
                    let path_term = u.string(&path);
                    let query_term = u.string(&query);
                    let body_term = body_cell(u, &request_body);
                    let rel = self.request;
                    let args = vec![
                        u.compound("const", vec![id_term]),
                        u.compound("const", vec![method_term]),
                        u.compound("const", vec![path_term]),
                        u.compound("const", vec![query_term]),
                        u.compound("const", vec![body_term]),
                    ];
                    self.request_rows.insert(
                        id,
                        Row {
                            rel,
                            args: args.clone(),
                        },
                    );
                    self.waiting.insert(id, answerer);
                    rows.push(Row { rel, args });
                }
                Inbound::Abandoned { id } => {
                    self.waiting.remove(&id);
                    if let Some(request_row) = self.request_rows.remove(&id) {
                        self.retract.push(request_row);
                    }
                }
                Inbound::Failed { port, message } => {
                    self.ports.remove(&port);
                    rows.push(self.error_row(u, port, &message));
                }
            }
        }
        rows
    }

    /// Signed rows from the served ports: request inserts from `poll` at
    /// weight 1, retracted request rows at weight -1. The batch `run` store
    /// cannot retract; only the live IR host applies the negative rows.
    fn poll_signed(&mut self, u: &mut Universe, _timeout: Duration) -> Vec<SignedRow> {
        let mut signed: Vec<SignedRow> = IService::poll(self, u)
            .into_iter()
            .map(|row| SignedRow { row, weight: 1 })
            .collect();
        for row in std::mem::take(&mut self.retract) {
            signed.push(SignedRow { row, weight: -1 });
        }
        signed
    }

    fn in_flight(&self) -> usize {
        self.waiting.len()
    }

    fn armed(&self) -> bool {
        !self.ports.is_empty()
    }
}
