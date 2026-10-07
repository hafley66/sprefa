//! `(http.fetch ?Method ?Url ?Headers ?RequestBody ?Status ?ContentType
//! ?ResponseBody)`: every pending request in flight at once on reqwest. A
//! text body lands as a string; any other body lands as a blob handle, the
//! blake3 hex of bytes kept in the blob map. Status 0 marks a transport
//! failure, where no response arrived; the message stands in `?ResponseBody`
//! with an empty `?ContentType`.

use super::{body_bytes, body_cell, classify_body, handle, media_type, message, HttpBody};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IService};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub const RELATION: &str = "http.fetch";

/// The 10-second law: a slower server is an error row, never a hang.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Status 0 marks a transport failure, where no response arrived.
const NO_RESPONSE: i64 = 0;

pub struct HttpFetch {
    relation: TermId,
    client: reqwest::Client,
    /// Finished fetches, plain data until `poll` mints terms.
    done: Receiver<Finished>,
    sender: Sender<Finished>,
    /// Fetches started and not yet drained.
    tasks: usize,
}

/// One finished fetch. Plain data; terms are minted in `poll` on the tick
/// thread.
#[derive(Clone)]
struct Finished {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    request_body: HttpBody,
    status: i64,
    content_type: String,
    response_body: HttpBody,
}

impl HttpFetch {
    pub fn new(relation: TermId) -> HttpFetch {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client");
        let (sender, done) = channel();
        HttpFetch {
            relation,
            client,
            done,
            sender,
            tasks: 0,
        }
    }
}

/// The bound text at one application position; `None` when unbound or not a
/// string.
fn text(u: &Universe, term: Option<TermId>) -> Option<String> {
    let Term::Str(sym) = u.get(term?) else {
        return None;
    };
    Some(u.sym_str(*sym).to_string())
}

/// `(name, value)` per `"name: value"` list item; unbound headers send none.
fn header_list(u: &Universe, term: Option<TermId>) -> Vec<(String, String)> {
    let Some(items) = term.and_then(|term| u.as_list(term)) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| {
            let Term::Str(sym) = u.get(item) else {
                return None;
            };
            let (name, value) = u.sym_str(*sym).split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// A string column value that names a stored blob is the blob.
fn body_term(u: &Universe, term: Option<TermId>) -> HttpBody {
    match text(u, term) {
        Some(value) => match super::blob_bytes(&value) {
            Some(_) => HttpBody::Blob(value),
            None => HttpBody::Text(value),
        },
        None => HttpBody::Text(String::new()),
    }
}

impl IService for HttpFetch {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let Some(values) = application_values(u, application) else {
                continue;
            };
            let at = |index: usize| values.get(index).copied().flatten();
            let Some(url) = text(u, at(1)) else {
                continue;
            };
            let method = text(u, at(0)).unwrap_or_else(|| "GET".to_string());
            let headers = header_list(u, at(2));
            let request_body = body_term(u, at(3));
            let request_bytes = body_bytes(&request_body);
            let echo = Finished {
                method: method.clone(),
                url: url.clone(),
                headers: headers.clone(),
                request_body,
                status: NO_RESPONSE,
                content_type: String::new(),
                response_body: HttpBody::Text(String::new()),
            };
            let method =
                reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
            let request = self.client.request(method, &url);
            let sender = self.sender.clone();
            self.tasks += 1;
            handle().spawn(async move {
                let finished = run_fetch(echo, request, headers, request_bytes).await;
                let _ = sender.send(finished);
                super::wake();
            });
        }
    }

    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        while let Ok(finished) = self.done.try_recv() {
            self.tasks -= 1;
            let method = u.string(&finished.method);
            let url = u.string(&finished.url);
            let header_items: Vec<TermId> = finished
                .headers
                .iter()
                .map(|(name, value)| u.string(&format!("{name}: {value}")))
                .collect();
            let headers = u.list(&header_items);
            let request_body = body_cell(u, &finished.request_body);
            let status = u.int(finished.status);
            let content_type = u.string(&finished.content_type);
            let response_body = body_cell(u, &finished.response_body);
            rows.push(Row {
                rel: self.relation,
                args: vec![
                    u.compound("const", vec![method]),
                    u.compound("const", vec![url]),
                    u.compound("const", vec![headers]),
                    u.compound("const", vec![request_body]),
                    u.compound("const", vec![status]),
                    u.compound("const", vec![content_type]),
                    u.compound("const", vec![response_body]),
                ],
            });
        }
        rows
    }

    fn in_flight(&self) -> usize {
        self.tasks
    }

    fn armed(&self) -> bool {
        false
    }
}

async fn run_fetch(
    mut echo: Finished,
    request: reqwest::RequestBuilder,
    headers: Vec<(String, String)>,
    body: bytes::Bytes,
) -> Finished {
    let mut request = request;
    for (name, value) in &headers {
        request = request.header(name, value);
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            echo.response_body = HttpBody::Text(message(error));
            return echo;
        }
    };
    let status = i64::from(response.status().as_u16());
    let content_type = media_type(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
    );
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            echo.status = status;
            echo.response_body = HttpBody::Text(message(error));
            return echo;
        }
    };
    echo.status = status;
    echo.content_type = content_type.clone();
    echo.response_body = classify_body(&content_type, &bytes);
    echo
}
