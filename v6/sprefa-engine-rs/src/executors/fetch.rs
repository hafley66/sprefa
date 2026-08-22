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
    pub last_modified: String,
    /// `X-Poll-Interval`, GitHub's own floor. 0 when the header is absent.
    pub poll_interval: i64,
    /// `X-RateLimit-Remaining`. -1 when absent, which the program reads as
    /// "no budget row yet" rather than as an exhausted pool.
    pub rate_remaining: i64,
    pub rate_reset: i64,
    /// `Link: <...>; rel="next"`, empty when this is the last page.
    pub next_link: String,
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
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let tag = header("etag");
    let last_modified = header("last-modified");
    let poll_interval = header("x-poll-interval").parse().unwrap_or(0);
    let rate_remaining = header("x-ratelimit-remaining").parse().unwrap_or(-1);
    let rate_reset = header("x-ratelimit-reset").parse().unwrap_or(0);
    let next_link = link_next(&header("link"));
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
        last_modified,
        poll_interval,
        rate_remaining,
        rate_reset,
        next_link,
    })
}

/// The `rel="next"` url out of an RFC 5988 `Link` header, empty when absent.
fn link_next(header: &str) -> String {
    for part in header.split(',') {
        let Some((target, params)) = part.split_once(';') else {
            continue;
        };
        if !params.contains("rel=\"next\"") && !params.contains("rel=next") {
            continue;
        }
        return target
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string();
    }
    String::new()
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

/// Ten pages caps one endpoint at 1000 rows, matching `repos.rs`, so a
/// pathological collection cannot hold a tick open.
const PAGE_CAP: usize = 10;

/// Walks `Link: rel="next"` and concatenates the arrays, as `gh api --paginate`
/// does (gh.rs:392-404). Only an ARRAY body pages, and a 304 pages not at all.
fn follow_link_next(host: &str, fetched: &mut Fetched) -> Result<i64, HostError> {
    if fetched.status != 200 {
        return Ok(0);
    }
    let Ok(serde_json::Value::Array(mut items)) =
        serde_json::from_str::<serde_json::Value>(&fetched.body)
    else {
        return Ok(1);
    };
    let mut pages = 1usize;
    let mut next = fetched.next_link.clone();
    while !next.is_empty() && pages < PAGE_CAP {
        let cached = cached_entry(&next);
        let prev_tag = cached.as_ref().map(|(tag, _)| tag.clone()).unwrap_or_default();
        let page = conditional_get(host, &next, &prev_tag)?;
        pages += 1;
        let text = match page.status {
            304 => match cached {
                Some((_, body)) => body,
                None => break,
            },
            200 => {
                remember(&next, &page.tag, &page.body);
                fetched.bytes += page.bytes;
                page.body.clone()
            }
            _ => break,
        };
        let Ok(serde_json::Value::Array(more)) = serde_json::from_str(&text) else {
            break;
        };
        items.extend(more);
        next = page.next_link;
    }
    fetched.body = serde_json::Value::Array(items).to_string();
    Ok(pages as i64)
}

/// A `json` column's DDL is `CHECK (json_valid(...))`, so a 304's empty body
/// arrives as the null document, which is gh.rs:220-227's reading of it too.
fn json_text(body: &str) -> &str {
    if body.trim().is_empty() {
        "null"
    } else {
        body
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
    let mut fetched = conditional_get(host, &url, &prev)?;
    if fetched.status == 304 {
        // The 304 arm `repos.rs` and `pulls.rs` take: the previous body IS
        // the answer, and `bytes` stays 0 because none crossed the wire.
        if let Some((_, body)) = cached_entry(&url) {
            fetched.body = body;
        }
    } else {
        remember(&url, &fetched.tag, &fetched.body);
    }
    let pages = follow_link_next(host, &mut fetched)?;
    Ok(answer_row(&fetched, pages))
}

/// The declared columns decide which keys survive: `carries_every_column`
/// drops a row a plan cannot fill, which is a 304 answering nothing new.
/// `tag` and `etag` are one header under the two registered spellings.
pub fn answer_row(fetched: &Fetched, pages: i64) -> HostRow {
    let mut answer = super::row([
        ("status", serde_json::json!(fetched.status)),
        ("tag", serde_json::json!(fetched.tag)),
        ("etag", serde_json::json!(fetched.tag)),
        ("last_modified", serde_json::json!(fetched.last_modified)),
        ("poll_interval", serde_json::json!(fetched.poll_interval)),
        ("rate_remaining", serde_json::json!(fetched.rate_remaining)),
        ("rate_reset", serde_json::json!(fetched.rate_reset)),
        ("bytes", serde_json::json!(fetched.bytes)),
        ("pages", serde_json::json!(pages)),
        ("body", serde_json::json!(json_text(&fetched.body))),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fetched(status: u16, body: &str) -> Fetched {
        Fetched {
            status,
            tag: "\"abc\"".into(),
            body: body.into(),
            bytes: body.len(),
            last_modified: "Thu, 01 Jan 2026 00:00:00 GMT".into(),
            poll_interval: 60,
            rate_remaining: 4998,
            rate_reset: 1_700_000_000,
            next_link: String::new(),
        }
    }

    #[test]
    fn link_next_reads_the_next_target_and_ignores_the_others() {
        let header = "<https://api.github.com/x?page=2>; rel=\"next\", \
                      <https://api.github.com/x?page=9>; rel=\"last\"";
        assert_eq!(link_next(header), "https://api.github.com/x?page=2");
        assert_eq!(link_next("<https://api.github.com/x?page=9>; rel=\"last\""), "");
        assert_eq!(link_next(""), "");
    }

    #[test]
    fn every_response_header_reaches_the_row_as_its_own_column() {
        let row = answer_row(&fetched(200, "{\"full_name\":\"o/n\"}"), 1);
        assert_eq!(row["status"], 200);
        assert_eq!(row["etag"], "\"abc\"");
        assert_eq!(row["tag"], row["etag"], "both registered spellings answer");
        assert_eq!(row["last_modified"], "Thu, 01 Jan 2026 00:00:00 GMT");
        assert_eq!(row["poll_interval"], 60);
        assert_eq!(row["rate_remaining"], 4998);
        assert_eq!(row["rate_reset"], 1_700_000_000i64);
        assert_eq!(row["bytes"], 19);
        assert_eq!(row["pages"], 1);
        assert_eq!(row["full_name"], "o/n", "body fields still splat");
    }

    /// A `json` column's DDL carries `CHECK (json_valid(...))`, so a 304's
    /// empty body must not reach it as the empty string.
    #[test]
    fn a_304_answers_the_null_document_rather_than_an_empty_body() {
        let row = answer_row(&fetched(304, ""), 0);
        assert_eq!(row["body"], "null");
        assert_eq!(row["bytes"], 0);
        assert_eq!(row["pages"], 0);
        assert!(
            serde_json::from_str::<serde_json::Value>(row["body"].as_str().unwrap()).is_ok(),
            "the body column has to be json_valid on every status"
        );
    }

    #[test]
    fn a_body_key_never_clobbers_a_header_column() {
        let row = answer_row(&fetched(200, "{\"bytes\":999,\"status\":7}"), 1);
        assert_eq!(row["bytes"], 24, "the header column wins");
        assert_eq!(row["status"], 200);
    }

    #[test]
    fn a_non_200_pages_not_at_all() {
        let mut answer = fetched(304, "");
        assert_eq!(follow_link_next("h", &mut answer).unwrap(), 0);
        let mut object = fetched(200, "{\"a\":1}");
        assert_eq!(
            follow_link_next("h", &mut object).unwrap(),
            1,
            "an object body is one document, never a page walk"
        );
    }
}
