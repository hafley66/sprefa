//! Conditional HTTP GET. `ureq` is blocking sockets with its own pool: the
//! host seam is sync, and a reqwest-blocking call inside `block_on` panics.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::{first_input, host_error, required_input};

/// One request may not outlive this; nothing seizes the machine.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_LIMIT: u64 = 8 * 1024 * 1024;
const USER_AGENT: &str = "sprefa-ghcacher";

pub static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .max_idle_connections(8)
        .max_idle_connections_per_host(4)
        .user_agent(USER_AGENT)
        .build()
        .into()
});

/// The ETag store the pagination walk reads: a 304 carries no body, so the
/// previous body is what the caller still needs.
static CONDITIONAL: LazyLock<Mutex<std::collections::HashMap<String, (String, String)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

pub fn cached_entry(url: &str) -> Option<(String, String)> {
    CONDITIONAL.lock().expect("etag cache").get(url).cloned()
}

pub fn remember(url: &str, tag: &str, body: &str) {
    if tag.is_empty() {
        return;
    }
    CONDITIONAL
        .lock()
        .expect("etag cache")
        .insert(url.to_string(), (tag.to_string(), body.to_string()));
}

pub fn forget_all() {
    CONDITIONAL.lock().expect("etag cache").clear();
}

/// api.github.com unless the program spelled a whole URL. `DL_GITHUB_API_BASE`
/// is the test door onto a local listener.
pub fn absolute_url(endpoint: &str) -> String {
    if endpoint.contains("://") {
        return endpoint.to_string();
    }
    let base = std::env::var("DL_GITHUB_API_BASE")
        .unwrap_or_else(|_| "https://api.github.com".to_string());
    format!("{}/{}", base.trim_end_matches('/'), endpoint.trim_start_matches('/'))
}

pub fn bearer_token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .filter(|token| !token.is_empty())
}

pub struct Fetched {
    pub status: u16,
    pub tag: String,
    pub body: String,
    pub bytes: usize,
}

/// One conditional GET. A 304 answers an empty body and the caller decides
/// whether the cached one stands in.
pub fn conditional_get(host: &str, url: &str, prev_tag: &str) -> Result<Fetched, HostError> {
    let span = tracing::info_span!("http_fetch", url, status = tracing::field::Empty,
        cached = tracing::field::Empty, bytes = tracing::field::Empty);
    let _entered = span.enter();
    let mut request = AGENT.get(url).header("Accept", "application/vnd.github+json");
    if !prev_tag.is_empty() {
        request = request.header("If-None-Match", prev_tag);
    }
    if let Some(token) = bearer_token() {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let mut response = request
        .call()
        .map_err(|failure| host_error(host, format!("GET {url}: {failure}")))?;
    let status = response.status().as_u16();
    let tag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = if status == 304 {
        String::new()
    } else {
        response
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_string()
            .map_err(|failure| host_error(host, format!("read body of {url}: {failure}")))?
    };
    let bytes = body.len();
    span.record("status", status);
    span.record("cached", status == 304);
    span.record("bytes", bytes);
    Ok(Fetched {
        status,
        tag,
        body,
        bytes,
    })
}

/// A conditional GET as a host: `status`, `tag`, `body`, and every top-level
/// field of a JSON object body, so a plan naming `full_name` gets it.
pub struct HttpFetchExecutor;

impl IHostExecutor for HttpFetchExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        // The batching seam: an `eps` input carrying a JSON array of endpoint
        // texts (json_group_array in the language) is ONE demand, one run.
        if let Some(batch) = first_input(env, &["eps"]).filter(|eps| !eps.is_empty()) {
            let endpoints: Vec<String> = serde_json::from_str(batch).map_err(|error| {
                host_error(host, format!("`eps` is not a JSON array of texts: {error}"))
            })?;
            let mut rows = Vec::with_capacity(endpoints.len());
            for endpoint in &endpoints {
                let mut row = fetch_one(host, endpoint, first_input(env, &["prev", "prev_etag", "tag"]))?;
                row.insert("ep".to_string(), serde_json::json!(endpoint));
                rows.push(row);
            }
            return Ok(rows);
        }
        let endpoint = required_input(host, env, &["ep", "endpoint_path", "url"])?;
        fetch_one(host, &endpoint, first_input(env, &["prev", "prev_etag", "tag"])).map(|row| vec![row])
    }
}

fn fetch_one(host: &str, endpoint: &str, prev_input: Option<&str>) -> Result<HostRow, HostError> {
    let url = absolute_url(endpoint);
    // A program that carries the tag relationally wins; one that does not
    // still re-asks conditionally, from this process's own store.
    let prev = match prev_input {
        Some(tag) if !tag.is_empty() => tag.to_string(),
        _ => cached_entry(&url).map(|(tag, _)| tag).unwrap_or_default(),
    };
    let fetched = conditional_get(host, &url, &prev)?;
    if fetched.status != 304 {
        remember(&url, &fetched.tag, &fetched.body);
    }
    Ok(answer_row(&fetched))
}

/// The declared columns decide which keys survive: `carries_every_column`
/// drops a row a plan cannot fill, which is a 304 answering nothing new.
pub fn answer_row(fetched: &Fetched) -> HostRow {
    let mut answer = super::row([
        ("status", serde_json::json!(fetched.status)),
        ("tag", serde_json::json!(fetched.tag)),
        ("body", serde_json::json!(fetched.body)),
    ]);
    if let Ok(serde_json::Value::Object(fields)) =
        serde_json::from_str::<serde_json::Value>(&fetched.body)
    {
        for (key, value) in fields {
            answer.entry(key).or_insert(value);
        }
    }
    answer
}
