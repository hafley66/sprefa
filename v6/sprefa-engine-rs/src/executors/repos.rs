//! `repos(org, bucket)` / `gh_repos(org, bucket)` over the GitHub REST API,
//! paginated through the same conditional client the fetch host uses.

use std::collections::BTreeMap;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::fetch::{absolute_url, cached_entry, conditional_get, remember};
use super::{host_error, required_input, row};

/// 100 per page is the API maximum; ten pages caps one org at 1000 rows so a
/// pathological org cannot hold a tick open.
const PER_PAGE: usize = 100;
const PAGE_CAP: usize = 10;

pub struct GhReposExecutor;

impl IHostExecutor for GhReposExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let org = required_input(host, env, &["org", "owner"])?;
        let span = tracing::info_span!("gh_repos", host, org, pages = tracing::field::Empty,
            rows = tracing::field::Empty);
        let _entered = span.enter();
        let rows = if org.contains('/') {
            one_repository(host, org)?
        } else {
            whole_org(host, org, &span)?
        };
        span.record("rows", rows.len());
        Ok(rows)
    }
}

/// A slug with a slash is one repository, and its absence is `exists_flag` 0
/// rather than an error: absence is data the rules join against.
fn one_repository(host: &str, slug: &str) -> Result<Vec<HostRow>, HostError> {
    let url = absolute_url(&format!("repos/{slug}"));
    match conditional_body(host, &url)? {
        Some(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(fields)) => Ok(vec![repository_row(slug, &fields, 1)]),
            _ => Ok(vec![absent_row(slug)]),
        },
        None => Ok(vec![absent_row(slug)]),
    }
}

fn whole_org(host: &str, org: &str, span: &tracing::Span) -> Result<Vec<HostRow>, HostError> {
    let mut rows = Vec::new();
    let mut pages = 0usize;
    for page in 1..=PAGE_CAP {
        let url = absolute_url(&format!("orgs/{org}/repos?per_page={PER_PAGE}&page={page}"));
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
                let slug = fields
                    .get("full_name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                rows.push(repository_row(&slug, &fields, 1));
            }
        }
        if count < PER_PAGE {
            break;
        }
    }
    span.record("pages", pages);
    Ok(rows)
}

/// The 304 arm: the previous body is the answer, and no bytes crossed the wire.
fn conditional_body(host: &str, url: &str) -> Result<Option<String>, HostError> {
    let cached = cached_entry(url);
    let prev_tag = cached
        .as_ref()
        .map(|(tag, _)| tag.clone())
        .unwrap_or_default();
    let fetched = conditional_get(host, url, &prev_tag)?;
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

fn repository_row(
    slug: &str,
    fields: &serde_json::Map<String, serde_json::Value>,
    exists_flag: i64,
) -> HostRow {
    let text = |key: &str| {
        fields
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (owner, name) = match slug.split_once('/') {
        Some((owner, name)) => (owner.to_string(), name.to_string()),
        None => (String::new(), slug.to_string()),
    };
    let full_name = if text("full_name").is_empty() {
        slug.to_string()
    } else {
        text("full_name")
    };
    let name = if name.is_empty() { text("name") } else { name };
    row([
        ("exists_flag", serde_json::json!(exists_flag)),
        ("repo_slug", serde_json::json!(slug)),
        ("full_name", serde_json::json!(full_name)),
        ("org", serde_json::json!(owner)),
        ("owner", serde_json::json!(owner)),
        ("name", serde_json::json!(name)),
        (
            "stars",
            serde_json::json!(fields
                .get("stargazers_count")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)),
        ),
        ("default_branch", serde_json::json!(text("default_branch"))),
        ("pushed_at", serde_json::json!(text("pushed_at"))),
    ])
}

fn absent_row(slug: &str) -> HostRow {
    row([
        ("exists_flag", serde_json::json!(0)),
        ("repo_slug", serde_json::json!(slug)),
        ("org", serde_json::json!(slug)),
    ])
}
