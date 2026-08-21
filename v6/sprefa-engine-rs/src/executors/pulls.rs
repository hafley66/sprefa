//! @comment-ok: the executor's contract, the one doc site for its columns.
//! `gh_pulls(repo_slug, state)` over the GitHub REST pull list, paginated
//! through the conditional client `repos.rs` and the fetch host share. One call
//! per (repo, state, page); a 304 moves zero body bytes and answers the same
//! rows from the remembered body. Columns: repo_slug, number, title, head_ref,
//! head_sha, state, merged_at, merged_sha, updated_at, mergeable,
//! review_decision, draft.

use std::collections::BTreeMap;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::fetch::{absolute_url, cached_entry, conditional_get, remember};
use super::{first_input, host_error, required_input, row};

/// 100 per page is the API maximum; five pages caps one repository at 500 pulls
/// so a pathological repository cannot hold a tick open.
const PER_PAGE: usize = 100;
const PAGE_CAP: usize = 5;

/// The list endpoint carries `mergeable` and leaves it null. A joining rule
/// reads a word rather than an empty cell.
const UNKNOWN: &str = "unknown";

pub struct GhPullsExecutor;

impl IHostExecutor for GhPullsExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let slug = required_input(host, env, &["repo_slug", "repo", "slug"])?;
        if !slug.contains('/') {
            return Err(host_error(
                host,
                format!("`{slug}` is not an <owner>/<name> repository slug"),
            ));
        }
        let state = match first_input(env, &["state", "bucket_state"]) {
            Some(named) if !named.is_empty() => named,
            _ => "all",
        };
        let span = tracing::info_span!(
            "gh_pulls",
            host,
            slug,
            state,
            pages = tracing::field::Empty,
            rows = tracing::field::Empty
        );
        let _entered = span.enter();
        let rows = walk(host, slug, state, &span)?;
        span.record("rows", rows.len());
        Ok(rows)
    }
}

fn walk(
    host: &str,
    slug: &str,
    state: &str,
    span: &tracing::Span,
) -> Result<Vec<HostRow>, HostError> {
    let mut rows = Vec::new();
    let mut pages = 0usize;
    for page in 1..=PAGE_CAP {
        let url = absolute_url(&format!(
            "repos/{slug}/pulls?state={state}&per_page={PER_PAGE}&page={page}\
             &sort=updated&direction=desc"
        ));
        pages += 1;
        let Some(text) = conditional_body(host, &url)? else {
            break;
        };
        let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&text) else {
            break;
        };
        let count = items.len();
        for item in items {
            if let serde_json::Value::Object(fields) = item {
                rows.push(pull_row(slug, &fields));
            }
        }
        if count < PER_PAGE {
            break;
        }
    }
    span.record("pages", pages);
    Ok(rows)
}

/// The 304 arm: the previous body is the answer and no bytes crossed the wire.
/// Every byte that did is counted, because `tick_cost` reads that counter.
fn conditional_body(host: &str, url: &str) -> Result<Option<String>, HostError> {
    let cached = cached_entry(url);
    let prev_tag = cached
        .as_ref()
        .map(|(tag, _)| tag.clone())
        .unwrap_or_default();
    let fetched = conditional_get(host, url, &prev_tag)?;
    super::cost::count_request(fetched.bytes as u64, fetched.status == 304);
    match fetched.status {
        304 => Ok(cached.map(|(_, body)| body)),
        200 => {
            remember(url, &fetched.tag, &fetched.body);
            Ok(Some(fetched.body))
        }
        404 => Ok(None),
        other => Err(host_error(host, format!("GET {url} answered {other}"))),
    }
}

/// `state` is the three-word vocabulary a rule joins on, not the endpoint's two:
/// a closed pull carrying `merged_at` is MERGED.
fn pull_row(slug: &str, fields: &serde_json::Map<String, serde_json::Value>) -> HostRow {
    let text = |key: &str| {
        fields
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let nested = |outer: &str, inner: &str| {
        fields
            .get(outer)
            .and_then(serde_json::Value::as_object)
            .and_then(|object| object.get(inner))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let merged_at = text("merged_at");
    let state = match (text("state").as_str(), merged_at.is_empty()) {
        (_, false) => "MERGED",
        ("open", _) => "OPEN",
        _ => "CLOSED",
    };
    let word_or_unknown = |key: &str| match fields.get(key) {
        Some(serde_json::Value::Bool(flag)) => flag.to_string(),
        Some(serde_json::Value::String(word)) if !word.is_empty() => word.clone(),
        _ => UNKNOWN.to_string(),
    };
    row([
        ("repo_slug", serde_json::json!(slug)),
        (
            "number",
            serde_json::json!(fields
                .get("number")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)),
        ),
        ("title", serde_json::json!(text("title"))),
        ("head_ref", serde_json::json!(nested("head", "ref"))),
        ("head_sha", serde_json::json!(nested("head", "sha"))),
        ("state", serde_json::json!(state)),
        ("merged_at", serde_json::json!(merged_at)),
        ("merged_sha", serde_json::json!(text("merge_commit_sha"))),
        ("updated_at", serde_json::json!(text("updated_at"))),
        ("mergeable", serde_json::json!(word_or_unknown("mergeable"))),
        ("review_decision", serde_json::json!(review_decision(fields))),
        (
            "draft",
            serde_json::json!(fields
                .get("draft")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)),
        ),
    ])
}

/// GraphQL owns `reviewDecision`; REST carries only the reviewer request set, so
/// an outstanding request reads REVIEW_REQUIRED and nothing else claims APPROVED.
fn review_decision(fields: &serde_json::Map<String, serde_json::Value>) -> String {
    if let Some(serde_json::Value::String(decision)) = fields.get("review_decision") {
        if !decision.is_empty() {
            return decision.clone();
        }
    }
    let requested = |key: &str| {
        fields
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|list| !list.is_empty())
    };
    if requested("requested_reviewers") || requested("requested_teams") {
        return "REVIEW_REQUIRED".to_string();
    }
    UNKNOWN.to_string()
}
