//! @comment-ok: the executor's contract, the one doc site for its columns.
//! `gh_pr_batch(batch_key, slug_list, bucket)` over the GitHub GraphQL API.
//! `slug_list` is a space-separated `owner/name` list the PROGRAM built with a
//! SQL aggregate, and each slug becomes one alias in ONE query, which is
//! sync/prs.rs:71-84's batching without its string code.
//!
//! Every query carries `rateLimit { cost remaining resetAt }` (gh.rs:378
//! inject_rate_limit), so the budget rows come back on the same call they
//! account for rather than from a second request.
//!
//! Columns: status, rate_remaining, rate_reset, gql_cost, bytes, data. `data`
//! is ONE json document whose keys are the flat element arrays the program's
//! spreads read: pulls, reviews, comments, labels, requested_reviewers,
//! status_checks. Flattening here rather than in dl6 is deliberate: the
//! GraphQL answer nests per-repository aliases, and a rel column cannot spell
//! "the alias whose name I computed".

use std::collections::BTreeMap;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::fetch::{absolute_url, bearer_token, AGENT};
use super::{first_input, host_error, required_input, row};

/// sync/prs.rs:58 BATCH_SIZE. A longer `slug_list` is truncated rather than
/// sent: an over-large query is a rejected query and the points go either way.
const BATCH_CAP: usize = 20;

/// sync/prs.rs:74 `pullRequests(first: 100, states: OPEN)`.
const PULLS_PER_REPO: usize = 100;

/// sync/prs.rs:7-41 PR_FIELDS, verbatim in field selection and page sizes.
const PR_FIELDS: &str = "number title state isDraft body \
     headRefName headRefOid baseRefName mergeable \
     additions deletions changedFiles \
     createdAt updatedAt mergedAt closedAt \
     databaseId id \
     author { login } \
     reviews(last: 20) { nodes { databaseId author { login } state body submittedAt } } \
     comments(last: 50) { nodes { databaseId author { login } body createdAt updatedAt } } \
     reviewRequests(first: 20) { nodes { requestedReviewer { \
       ... on User { login } ... on Team { name: slug } } } } \
     labels(first: 10) { nodes { name color } } \
     commits(last: 1) { nodes { commit { statusCheckRollup { \
       contexts(first: 50) { nodes { __typename \
         ... on StatusContext { context state targetUrl description } \
         ... on CheckRun { name conclusion detailsUrl } } } } } } }";

pub struct GhPrBatchExecutor;

impl IHostExecutor for GhPrBatchExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let batch_key = required_input(host, env, &["batch_key", "key"])?;
        let slug_list = first_input(env, &["slug_list", "slugs"]).unwrap_or_default();
        let slugs: Vec<&str> = slug_list
            .split_whitespace()
            .filter(|slug| slug.contains('/'))
            .take(BATCH_CAP)
            .collect();
        if slugs.is_empty() {
            return Ok(Vec::new());
        }
        let span = tracing::info_span!(
            "gh_pr_batch",
            host,
            batch_key,
            repos = slugs.len(),
            status = tracing::field::Empty,
            pulls = tracing::field::Empty
        );
        let _entered = span.enter();

        let (status, body, bytes) = post(host, &query_for(&slugs))?;
        span.record("status", status);
        if status != 200 {
            return Err(host_error(
                host,
                format!("graphql answered {status} for {batch_key}"),
            ));
        }
        if let Some(errors) = body.get("errors") {
            return Err(host_error(host, format!("graphql errors: {errors}")));
        }

        let data = &body["data"];
        let rate = &data["rateLimit"];
        let flat = flatten(&slugs, data);
        span.record("pulls", flat["pulls"].as_array().map_or(0, Vec::len));

        Ok(vec![row([
            ("status", serde_json::json!(status)),
            (
                "rate_remaining",
                serde_json::json!(rate["remaining"].as_i64().unwrap_or(-1)),
            ),
            ("rate_reset", serde_json::json!(reset_epoch(rate))),
            (
                "gql_cost",
                serde_json::json!(rate["cost"].as_i64().unwrap_or(0)),
            ),
            ("bytes", serde_json::json!(bytes)),
            ("data", serde_json::json!(flat.to_string())),
        ])])
    }
}

/// One aliased selection per repository, plus the rate-limit selection every
/// query carries.
fn query_for(slugs: &[&str]) -> String {
    let mut out = String::from("{\n  rateLimit { cost remaining resetAt }\n");
    for (index, slug) in slugs.iter().enumerate() {
        let (owner, name) = slug.split_once('/').unwrap_or((*slug, ""));
        out.push_str(&format!(
            "  repo_{index}: repository(owner: {}, name: {}) {{\n\
             \x20   pullRequests(first: {PULLS_PER_REPO}, states: OPEN, \
             orderBy: {{field: UPDATED_AT, direction: DESC}}) {{\n\
             \x20     nodes {{ {PR_FIELDS} }}\n\
             \x20   }}\n  }}\n",
            serde_json::json!(owner),
            serde_json::json!(name)
        ));
    }
    out.push('}');
    out
}

fn post(host: &str, query: &str) -> Result<(u16, serde_json::Value, usize), HostError> {
    let url = absolute_url("graphql");
    let mut request = AGENT
        .post(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Content-Type", "application/json");
    if let Some(token) = bearer_token() {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    // ureq is linked without its `json` feature, so the envelope is serialized
    // here rather than by `send_json`.
    let envelope = serde_json::json!({ "query": query }).to_string();
    let mut response = request
        .send(&envelope)
        .map_err(|failure| host_error(host, format!("POST {url}: {failure}")))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|failure| host_error(host, format!("read body of {url}: {failure}")))?;
    let bytes = text.len();
    let body = serde_json::from_str(&text)
        .map_err(|failure| host_error(host, format!("parse graphql answer: {failure}")))?;
    Ok((status, body, bytes))
}

/// `resetAt` is RFC 3339; the program compares it against a clock bucket, so it
/// arrives as epoch seconds like the REST `X-RateLimit-Reset` header does.
fn reset_epoch(rate: &serde_json::Value) -> i64 {
    rate["resetAt"].as_str().map(rfc3339_epoch).unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SSZ` to epoch seconds. GitHub always answers UTC in this
/// one shape, and the crate is linked without a date library.
fn rfc3339_epoch(stamp: &str) -> i64 {
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        stamp.get(range)?.parse().ok()
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return 0;
    };
    let (Some(hour), Some(minute), Some(second)) =
        (digits(11..13), digits(14..16), digits(17..19))
    else {
        return 0;
    };
    days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second
}

/// Howard Hinnant's civil-to-days, the standard branch-free form.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Flattened into the six element arrays the dl6 spreads read. Every element
/// carries `owner`/`name`/`number` so a spread rebuilds the repository ref.
fn flatten(slugs: &[&str], data: &serde_json::Value) -> serde_json::Value {
    let mut pulls = Vec::new();
    let mut reviews = Vec::new();
    let mut comments = Vec::new();
    let mut labels = Vec::new();
    let mut requested = Vec::new();
    let mut checks = Vec::new();

    for (index, slug) in slugs.iter().enumerate() {
        let (owner, name) = slug.split_once('/').unwrap_or((*slug, ""));
        let Some(nodes) = data[format!("repo_{index}")]["pullRequests"]["nodes"].as_array() else {
            continue;
        };
        for node in nodes {
            let number = node["number"].as_i64().unwrap_or(0);
            pulls.push(pull_row(owner, name, number, node));
            for review in array(&node["reviews"]["nodes"]) {
                reviews.push(serde_json::json!({
                    "owner": owner, "name": name, "number": number,
                    "gh_id": review["databaseId"].as_i64().unwrap_or(0),
                    "author": text(&review["author"]["login"]),
                    "state": text(&review["state"]),
                    "body": text(&review["body"]),
                    "submitted_at": text(&review["submittedAt"]),
                }));
            }
            for comment in array(&node["comments"]["nodes"]) {
                comments.push(serde_json::json!({
                    "owner": owner, "name": name, "number": number,
                    "gh_id": comment["databaseId"].as_i64().unwrap_or(0),
                    "author": text(&comment["author"]["login"]),
                    "body": text(&comment["body"]),
                    "path": "",
                    "line": 0,
                    "in_reply_to_id": 0,
                    "created_at": text(&comment["createdAt"]),
                    "updated_at": text(&comment["updatedAt"]),
                }));
            }
            for label in array(&node["labels"]["nodes"]) {
                labels.push(serde_json::json!({
                    "owner": owner, "name": name, "number": number,
                    "label": text(&label["name"]),
                    "color": text(&label["color"]),
                }));
            }
            for request in array(&node["reviewRequests"]["nodes"]) {
                let reviewer = match request["requestedReviewer"]["login"].as_str() {
                    Some(login) => login.to_string(),
                    None => text(&request["requestedReviewer"]["name"]),
                };
                if reviewer.is_empty() {
                    continue;
                }
                requested.push(serde_json::json!({
                    "owner": owner, "name": name, "number": number,
                    "reviewer": reviewer,
                }));
            }
            for context in status_contexts(node) {
                if let Some(check) = status_check(owner, name, number, &context) {
                    checks.push(check);
                }
            }
        }
    }

    serde_json::json!({
        "pulls": pulls,
        "reviews": reviews,
        "comments": comments,
        "labels": labels,
        "requested_reviewers": requested,
        "status_checks": checks,
    })
}

fn pull_row(
    owner: &str,
    name: &str,
    number: i64,
    node: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "owner": owner,
        "name": name,
        "number": number,
        "node_id": text(&node["id"]),
        "state": pr_state(node),
        "title": text(&node["title"]),
        "author": text(&node["author"]["login"]),
        "head_ref": text(&node["headRefName"]),
        "head_sha": text(&node["headRefOid"]),
        "base_ref": text(&node["baseRefName"]),
        "mergeable": text(&node["mergeable"]),
        "draft": i64::from(node["isDraft"].as_bool().unwrap_or(false)),
        "additions": node["additions"].as_i64().unwrap_or(0),
        "deletions": node["deletions"].as_i64().unwrap_or(0),
        "changed_files": node["changedFiles"].as_i64().unwrap_or(0),
        "created_at": text(&node["createdAt"]),
        "updated_at": text(&node["updatedAt"]),
        "merged_at": text(&node["mergedAt"]),
        "closed_at": text(&node["closedAt"]),
        "body": text(&node["body"]),
    })
}

/// sync/prs.rs:210-214, the three-word vocabulary the stored rows use.
fn pr_state(node: &serde_json::Value) -> &'static str {
    match node["state"].as_str().unwrap_or("OPEN") {
        "MERGED" => "merged",
        "CLOSED" => "closed",
        _ => "open",
    }
}

fn status_contexts(node: &serde_json::Value) -> Vec<serde_json::Value> {
    array(&node["commits"]["nodes"])
        .first()
        .map(|commit| {
            array(&commit["commit"]["statusCheckRollup"]["contexts"]["nodes"])
                .into_iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// sync/prs.rs:348-362: a StatusContext and a CheckRun are one stored shape
/// under different field names, and anything else contributes no row.
fn status_check(
    owner: &str,
    name: &str,
    number: i64,
    context: &serde_json::Value,
) -> Option<serde_json::Value> {
    let (label, state, url, description) = match context["__typename"].as_str()? {
        "StatusContext" => (
            text(&context["context"]),
            text(&context["state"]),
            text(&context["targetUrl"]),
            text(&context["description"]),
        ),
        "CheckRun" => {
            let conclusion = text(&context["conclusion"]);
            (
                text(&context["name"]),
                if conclusion.is_empty() {
                    "pending".to_string()
                } else {
                    conclusion
                },
                text(&context["detailsUrl"]),
                String::new(),
            )
        }
        _ => return None,
    };
    Some(serde_json::json!({
        "owner": owner, "name": name, "number": number,
        "context": label, "state": state,
        "target_url": url, "description": description,
        "updated_at": "",
    }))
}

fn array(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    value.as_array().map(|items| items.iter().collect()).unwrap_or_default()
}

/// A null field reads as the empty word: the column plane has no null, and
/// absence stays row absence rather than becoming a null cell.
fn text(value: &serde_json::Value) -> String {
    value.as_str().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_carries_one_alias_per_repo_and_the_rate_limit() {
        let query = query_for(&["o/a", "o/b"]);
        assert!(query.contains("rateLimit { cost remaining resetAt }"));
        assert!(query.contains("repo_0: repository(owner: \"o\", name: \"a\")"));
        assert!(query.contains("repo_1: repository(owner: \"o\", name: \"b\")"));
        assert_eq!(query.matches("pullRequests(first:").count(), 2);
    }

    #[test]
    fn query_escapes_a_slug_rather_than_interpolating_it() {
        let query = query_for(&["o\"evil/n"]);
        assert!(query.contains(r#"owner: "o\"evil""#), "{query}");
    }

    #[test]
    fn flatten_fans_every_child_set_out_with_its_repo_and_number() {
        let data = serde_json::json!({
            "repo_0": { "pullRequests": { "nodes": [{
                "number": 7, "id": "PR_7", "state": "OPEN", "title": "t",
                "isDraft": false, "author": {"login": "alice"},
                "reviews": {"nodes": [
                    {"databaseId": 11, "author": {"login": "bob"},
                     "state": "APPROVED", "body": "", "submittedAt": "2026-01-01"}]},
                "comments": {"nodes": [
                    {"databaseId": 12, "author": {"login": "carol"}, "body": "hi",
                     "createdAt": "2026-01-02", "updatedAt": "2026-01-02"}]},
                "labels": {"nodes": [{"name": "bug", "color": "red"}]},
                "reviewRequests": {"nodes": [
                    {"requestedReviewer": {"login": "dave"}},
                    {"requestedReviewer": {"name": "platform-team"}}]},
                "commits": {"nodes": [{"commit": {"statusCheckRollup": {
                    "contexts": {"nodes": [
                        {"__typename": "StatusContext", "context": "ci/build",
                         "state": "success", "targetUrl": "u", "description": "ok"},
                        {"__typename": "CheckRun", "name": "test",
                         "conclusion": "success", "detailsUrl": "u2"}]}}}}]}
            }]}}
        });
        let flat = flatten(&["o/n"], &data);

        assert_eq!(flat["pulls"].as_array().unwrap().len(), 1);
        assert_eq!(flat["pulls"][0]["state"], "open");
        assert_eq!(flat["pulls"][0]["owner"], "o");
        assert_eq!(flat["pulls"][0]["number"], 7);

        assert_eq!(flat["reviews"][0]["gh_id"], 11);
        assert_eq!(flat["reviews"][0]["number"], 7);
        assert_eq!(flat["comments"][0]["author"], "carol");
        assert_eq!(flat["labels"][0]["label"], "bug");

        let reviewers: Vec<&str> = flat["requested_reviewers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["reviewer"].as_str().unwrap())
            .collect();
        assert_eq!(reviewers, vec!["dave", "platform-team"]);

        let checks = flat["status_checks"].as_array().unwrap();
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0]["context"], "ci/build");
        assert_eq!(checks[1]["context"], "test");
        assert_eq!(checks[1]["state"], "success");
    }

    #[test]
    fn merged_and_closed_map_to_the_stored_vocabulary() {
        for (graphql, stored) in [("MERGED", "merged"), ("CLOSED", "closed"), ("OPEN", "open")] {
            assert_eq!(pr_state(&serde_json::json!({"state": graphql})), stored);
        }
    }

    #[test]
    fn an_unknown_check_typename_contributes_no_row() {
        let unknown = serde_json::json!({"__typename": "Something", "context": "x"});
        assert!(status_check("o", "n", 1, &unknown).is_none());
    }

    #[test]
    fn reset_at_becomes_epoch_seconds() {
        let rate = serde_json::json!({"resetAt": "2026-01-01T00:00:00Z"});
        assert_eq!(reset_epoch(&rate), 1767225600);
        assert_eq!(reset_epoch(&serde_json::json!({})), 0);
    }
}
