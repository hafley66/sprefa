//! @comment-ok: the transport contract, the one doc site for its columns.
//! ONE general transport. `http_get(url, headers, prev_etag, bucket)` and
//! `http_post(url, headers, request_body, bucket)` both answer
//! `(status, response_headers, body, bytes)`.
//!
//! The request IS the row. Every header on the wire comes from the `headers`
//! column, `Authorization` and `If-None-Match` included, so no token reaches
//! this file and no response is remembered between calls. A conditional GET,
//! a page walk, a GraphQL query and a rate budget are all rules in the program
//! that spelled them.
//!
//! `prev_etag` shapes no header. It is a demand-identity column: a program
//! whose stored tag changed asks a NEW question rather than re-asking the
//! answered one.
//!
//! `ureq` is blocking sockets with its own pool: the host seam is sync, and a
//! reqwest-blocking call inside `block_on` panics.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::{first_input, host_error, required_input};

/// One request may not outlive this; nothing seizes the machine.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_LIMIT: u64 = 8 * 1024 * 1024;
const USER_AGENT: &str = "sprefa-dl6";

/// The connection pool is the only state this module holds across calls.
pub static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .max_idle_connections(16)
        .max_idle_connections_per_host(8)
        .user_agent(USER_AGENT)
        .build()
        .into()
});

/// api.github.com unless the program spelled a whole URL. `DL_GITHUB_API_BASE`
/// is the test door onto a local listener.
pub fn absolute_url(endpoint: &str) -> String {
    if endpoint.contains("://") {
        return endpoint.to_string();
    }
    let base =
        std::env::var("DL_GITHUB_API_BASE").unwrap_or_else(|_| "https://api.github.com".to_string());
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    )
}

/// The pool ceiling, `daemon_thread_count`'s reading: a quarter of the cores,
/// floor 2, `DL_HTTP_CONCURRENCY` overriding. Nothing seizes the machine.
pub fn transport_concurrency() -> usize {
    if let Some(spelled) = std::env::var("DL_HTTP_CONCURRENCY")
        .ok()
        .and_then(|text| text.parse::<usize>().ok())
        .filter(|count| *count > 0)
    {
        return spelled;
    }
    let cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(2);
    std::cmp::max(2, cores / 4)
}

/// A json column crosses the seam as TEXT under `json_valid`: a non-JSON body
/// travels as a JSON string of itself, an empty one as the null document.
fn json_column(text: &str) -> serde_json::Value {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return serde_json::Value::String("null".to_string());
    }
    if serde_json::from_str::<serde::de::IgnoredAny>(trimmed).is_ok() {
        return serde_json::Value::String(trimmed.to_string());
    }
    serde_json::Value::String(
        serde_json::Value::String(text.to_string()).to_string(),
    )
}

/// The `headers` input column: a JSON object of header name to header value.
/// Anything else is a named stop rather than a silently header-less request.
fn request_headers(host: &str, spelled: &str) -> Result<Vec<(String, String)>, HostError> {
    let trimmed = spelled.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    let document: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|failure| host_error(host, format!("`headers` is not JSON: {failure}")))?;
    let serde_json::Value::Object(fields) = document else {
        return Err(host_error(
            host,
            format!("`headers` is not a JSON object: {trimmed}"),
        ));
    };
    Ok(fields
        .into_iter()
        .filter_map(|(name, value)| match value {
            serde_json::Value::String(text) => Some((name, text)),
            serde_json::Value::Null => None,
            other => Some((name, other.to_string())),
        })
        .filter(|(_, value)| !value.is_empty())
        .collect())
}

/// Response header names are lowercased: HTTP header names are
/// case-insensitive and a decode rule may not depend on the origin's case.
fn answer_row(status: u16, headers: serde_json::Map<String, serde_json::Value>, body: &str) -> HostRow {
    super::row([
        ("status", serde_json::json!(status)),
        (
            "response_headers",
            serde_json::Value::String(serde_json::Value::Object(headers).to_string()),
        ),
        ("body", json_column(body)),
        ("bytes", serde_json::json!(body.len())),
    ])
}

/// A request the transport can send, built entirely out of one demand row.
pub struct Request {
    pub host: String,
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

pub fn send(request: &Request) -> Result<HostRow, HostError> {
    let span = tracing::info_span!(
        "http",
        url = %request.url,
        status = tracing::field::Empty,
        bytes = tracing::field::Empty
    );
    let _entered = span.enter();
    // GET and POST build DIFFERENT `ureq` builder types (WithoutBody /
    // WithBody), so the header loop cannot be lifted out of the match.
    let sent = match request.method {
        Method::Get => {
            let mut builder = AGENT.get(&request.url);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            builder.call()
        }
        Method::Post => {
            let mut builder = AGENT.post(&request.url);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            builder.send(request.body.as_deref().unwrap_or(""))
        }
    };
    let mut response = sent.map_err(|failure| {
        host_error(&request.host, format!("{} {}: {failure}", request.method.spelling(), request.url))
    })?;
    let status = response.status().as_u16();
    let mut headers = serde_json::Map::new();
    for (name, value) in response.headers().iter() {
        let Ok(text) = value.to_str() else {
            continue;
        };
        headers.insert(
            name.as_str().to_ascii_lowercase(),
            serde_json::Value::String(text.to_string()),
        );
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()
        .map_err(|failure| {
            host_error(&request.host, format!("read body of {}: {failure}", request.url))
        })?;
    span.record("status", status);
    span.record("bytes", body.len());
    // The tick-cost counters: a 304 carries no body and moves no wire bytes.
    crate::executors::cost::count_request(body.len() as u64, status == 304);
    Ok(answer_row(status, headers, &body))
}

impl Method {
    fn spelling(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

/// One tick's transport demands on a bounded pool, answered in demand order:
/// the answers line up positionally with the requests handed in.
pub fn send_all(requests: &[Request]) -> Vec<Result<HostRow, HostError>> {
    if requests.len() < 2 {
        return requests.iter().map(send).collect();
    }
    let width = std::cmp::min(transport_concurrency(), requests.len());
    let span = tracing::info_span!("http_pool", requests = requests.len(), width);
    let _entered = span.enter();
    let mut answers: Vec<Option<Result<HostRow, HostError>>> =
        (0..requests.len()).map(|_| None).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<&mut Option<Result<HostRow, HostError>>>> =
        answers.iter_mut().map(std::sync::Mutex::new).collect();
    std::thread::scope(|scope| {
        for _ in 0..width {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(request) = requests.get(index) else {
                    return;
                };
                let answered = send(request);
                **slots[index].lock().expect("transport slot") = Some(answered);
            });
        }
    });
    drop(slots);
    answers
        .into_iter()
        .map(|slot| slot.expect("every request answered"))
        .collect()
}

fn request_of(
    host: &str,
    method: Method,
    env: &BTreeMap<String, String>,
) -> Result<Request, HostError> {
    let url = absolute_url(required_input(host, env, &["url"])?);
    let headers = request_headers(host, first_input(env, &["headers"]).unwrap_or(""))?;
    let body = match method {
        Method::Get => None,
        Method::Post => Some(
            first_input(env, &["request_body"])
                .unwrap_or("")
                .to_string(),
        ),
    };
    Ok(Request {
        host: host.to_string(),
        method,
        url,
        headers,
        body,
    })
}

pub struct HttpGetExecutor;

impl IHostExecutor for HttpGetExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        send(&request_of(host, Method::Get, env)?).map(|row| vec![row])
    }
}

pub struct HttpPostExecutor;

impl IHostExecutor for HttpPostExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        send(&request_of(host, Method::Post, env)?).map(|row| vec![row])
    }
}

/// The `collect` seam: a transport demand becomes a `Request` there so the
/// whole tick's requests ride one pool rather than one executor call each.
pub fn transport_request(
    execution: &str,
    host: &str,
    env: &BTreeMap<String, String>,
) -> Option<Result<Request, HostError>> {
    let method = match execution {
        "/http/get" => Method::Get,
        "/http/post" => Method::Post,
        _ => return None,
    };
    Some(request_of(host, method, env))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_that_is_not_json_travels_as_a_json_string() {
        assert_eq!(json_column("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(json_column(""), "null");
        assert_eq!(json_column("not json"), "\"not json\"");
        for spelling in ["{\"a\":1}", "", "not json", "[1,2]"] {
            let column = json_column(spelling);
            let text = column.as_str().expect("json column is text");
            assert!(
                serde_json::from_str::<serde_json::Value>(text).is_ok(),
                "a json column has to be json_valid on every body: {spelling}"
            );
        }
    }

    #[test]
    fn every_header_on_the_wire_comes_from_the_row() {
        let headers = request_headers(
            "http_get",
            r#"{"Accept":"application/vnd.github+json","If-None-Match":"\"tag\"","Empty":""}"#,
        )
        .expect("a JSON object of headers");
        assert_eq!(
            headers,
            vec![
                ("Accept".to_string(), "application/vnd.github+json".to_string()),
                ("If-None-Match".to_string(), "\"tag\"".to_string()),
            ],
            "an empty value is no header, never a blank one"
        );
        assert!(request_headers("http_get", "").expect("absent").is_empty());
        assert!(request_headers("http_get", "null").expect("null").is_empty());
        assert!(request_headers("http_get", "[1]").is_err(), "an array is a named stop");
    }

    #[test]
    fn the_transport_pool_is_bounded() {
        std::env::set_var("DL_HTTP_CONCURRENCY", "3");
        assert_eq!(transport_concurrency(), 3);
        std::env::remove_var("DL_HTTP_CONCURRENCY");
        assert!(transport_concurrency() >= 2, "floor 2");
    }
}
