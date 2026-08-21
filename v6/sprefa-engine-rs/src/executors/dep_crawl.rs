// @comment-ok: module contract, the frontier-closure family's one doc site.
// The four `dep_crawl_*` names over one traversal per (checkout_root, seed,
// frontier). A crawl produces four relations of different arity and a host
// answers with one row shape, so the crawl is four hosts over the same three
// inputs and the memo is what settles the four demands in one pass.
//
// LOCAL LOCATE ONLY: `dep_resolve.rs` never reaches the network. A target with
// no checkout under the root becomes a `dep_crawl_unresolved` row carrying its
// reason, so the boundary of the corpus is a relation and not a silence.
//
// `frontier` names which reader answers `targets_at`: `go_mod` for a Go corpus,
// `specifier` for the import/require plane.

use std::collections::BTreeMap;

use crate::dep_resolve::{
    DepResolveOutcome, DepResolver, GoModFrontier, IDepFrontierSource, LocalRepoRoster,
    SpecifierFrontier,
};
use crate::hosts::{host_row, required_input, HostError, IHostExecutor};
use crate::types::HostRow;

use super::{checkout_root, stop, FamilyMemo};

pub const DEP_CRAWL_REPO_COLUMNS: &[&str] = &["coordinate", "root"];
pub const DEP_CRAWL_VISITED_COLUMNS: &[&str] = &["repo", "rev", "hop"];
pub const DEP_CRAWL_EDGE_COLUMNS: &[&str] = &["from_repo", "from_rev", "target", "to_repo"];
pub const DEP_CRAWL_UNRESOLVED_COLUMNS: &[&str] = &["from_repo", "target", "reason"];

const CRAWL_HOSTS: [&str; 4] = [
    "dep_crawl_repo",
    "dep_crawl_visited",
    "dep_crawl_edge",
    "dep_crawl_unresolved",
];

#[derive(Default)]
pub struct DepCrawlExecutor {
    crawls: FamilyMemo<Vec<HostRow>>,
}

impl DepCrawlExecutor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IHostExecutor for DepCrawlExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        if !CRAWL_HOSTS.contains(&host) {
            return Err(stop(
                host,
                format!("not a crawl host; the roster is {}", CRAWL_HOSTS.join(", ")),
            ));
        }
        let root = checkout_root(host, &required_input(host, env, "checkout_root")?)?;
        let seed = required_input(host, env, "seed")?;
        let frontier = required_input(host, env, "frontier")?;
        let key = format!("{}|{seed}|{frontier}", root.display());
        let rows = self
            .crawls
            .answer(key, || crawl_rows(host, &root, &seed, &frontier))?;
        Ok(rows.as_ref().clone())
    }
}

fn frontier_reader(host: &str, name: &str) -> Result<Box<dyn IDepFrontierSource>, HostError> {
    match name {
        "go_mod" => Ok(Box::new(GoModFrontier::new())),
        "specifier" => Ok(Box::new(SpecifierFrontier::default())),
        other => Err(stop(
            host,
            format!("frontier `{other}` has no reader; the roster is go_mod and specifier"),
        )),
    }
}

fn crawl_rows(
    host: &str,
    root: &std::path::Path,
    seed: &str,
    frontier: &str,
) -> Result<Vec<HostRow>, HostError> {
    let span = tracing::info_span!("dep_crawl", root = %root.display(), seed, frontier);
    let _entered = span.enter();
    let mut reader = frontier_reader(host, frontier)?;
    let roster = LocalRepoRoster::scan_checkout_root(root).map_err(|error| {
        stop(
            host,
            format!("scan the checkout root {}: {error:#}", root.display()),
        )
    })?;
    let outcome = DepResolver::new(&roster)
        .run(&[seed.to_string()], reader.as_mut())
        .map_err(|error| stop(host, format!("crawl from `{seed}`: {error:#}")))?;
    Ok(outcome_rows(&outcome))
}

fn outcome_rows(outcome: &DepResolveOutcome) -> Vec<HostRow> {
    let mut rows: Vec<HostRow> = Vec::with_capacity(
        outcome.repos.len() + outcome.visited.len() + outcome.edges.len() + outcome.unresolved.len(),
    );
    for repo in &outcome.repos {
        rows.push(host_row([
            ("coordinate", serde_json::json!(repo.coordinate)),
            ("root", serde_json::json!(repo.root.to_string_lossy())),
        ]));
    }
    for at in &outcome.visited {
        rows.push(host_row([
            ("repo", serde_json::json!(at.coordinate)),
            ("rev", serde_json::json!(at.revision)),
            ("hop", serde_json::json!(at.hop)),
        ]));
    }
    for edge in &outcome.edges {
        rows.push(host_row([
            ("from_repo", serde_json::json!(edge.from_repo)),
            ("from_rev", serde_json::json!(edge.from_revision)),
            ("target", serde_json::json!(edge.target)),
            ("to_repo", serde_json::json!(edge.to_repo)),
        ]));
    }
    for gap in &outcome.unresolved {
        rows.push(host_row([
            ("from_repo", serde_json::json!(gap.from_repo)),
            ("target", serde_json::json!(gap.target)),
            ("reason", serde_json::json!(gap.reason.as_str())),
        ]));
    }
    rows
}
