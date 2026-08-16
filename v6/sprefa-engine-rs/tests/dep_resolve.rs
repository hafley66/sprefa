//! FAIL-PRE-FIX: written before the module existed --
//! `error[E0432]: unresolved import sprefa_engine_rs::dep_resolve`, rc=101.
//!
//! SABOTAGE 1, drop the visited-set guard on the recursion branch: the cycle
//! and drift tests go RED with `left: VisitBudget, right: FrontierClosed`
//! (5 passed, 2 failed). The backstop is why that is a failure and not a hang.
//!
//! SABOTAGE 2, drop the per-coordinate revision cache in `admit`: only the
//! drift test goes RED, same assertion (6 passed, 1 failed). Two guards, two
//! discriminating tests.
//!
//! THE LOCAL CORPUS LEG is `#[ignore]`d and reads `DEP_RESOLVE_CORPUS`: a real
//! checkout root is not a hermetic fixture.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sprefa_engine_rs::dep_resolve::{
    DepResolver, GoModFrontier, IDepFrontierSource, LocalRepo, LocalRepoRoster, StopReason,
    UnresolvedReason,
};

/// The frontier under test with no filesystem: each repository's outgoing
/// targets come from a table, and every call is recorded so a re-entry shows
/// up as a duplicate rather than as a hang.
struct TableFrontier {
    targets: BTreeMap<String, Vec<String>>,
    visits: Vec<String>,
    /// When set, `revision_of` answers a fresh revision every call. A resolver
    /// that keyed its visited set on that answer would never close.
    revision_drifts: bool,
    revision_calls: u32,
}

impl TableFrontier {
    fn new(targets: &[(&str, &[&str])]) -> Self {
        Self {
            targets: targets
                .iter()
                .map(|(repo, out)| {
                    (
                        (*repo).to_string(),
                        out.iter().map(|t| (*t).to_string()).collect(),
                    )
                })
                .collect(),
            visits: Vec::new(),
            revision_drifts: false,
            revision_calls: 0,
        }
    }

    fn drifting(mut self) -> Self {
        self.revision_drifts = true;
        self
    }
}

impl IDepFrontierSource for TableFrontier {
    fn revision_of(&mut self, repo: &LocalRepo) -> anyhow::Result<String> {
        self.revision_calls += 1;
        Ok(if self.revision_drifts {
            format!("rev-{}-{}", repo.coordinate, self.revision_calls)
        } else {
            format!("rev-{}", repo.coordinate)
        })
    }

    fn targets_at(&mut self, repo: &LocalRepo, _revision: &str) -> anyhow::Result<Vec<String>> {
        self.visits.push(repo.coordinate.clone());
        Ok(self
            .targets
            .get(&repo.coordinate)
            .cloned()
            .unwrap_or_default())
    }
}

fn roster(coordinates: &[&str]) -> LocalRepoRoster {
    LocalRepoRoster::from_entries(
        coordinates
            .iter()
            .map(|c| ((*c).to_string(), PathBuf::from(format!("/checkouts/{c}")))),
    )
}

/// TERMINATION GATE. A depends on B, B depends back on A. The visited set is
/// keyed on (repo, revision), so the second arrival at A is a set hit and the
/// queue drains. The receipt is the visited set itself plus a hop count that
/// cannot exceed the roster size.
#[test]
fn a_dependency_cycle_closes_with_the_visited_set_as_the_receipt() {
    let roster = roster(&["example.com/a", "example.com/b"]);
    let mut frontier = TableFrontier::new(&[
        ("example.com/a", &["example.com/b"]),
        ("example.com/b", &["example.com/a"]),
    ]);

    let outcome = DepResolver::new(&roster)
        .run(&["example.com/a".to_string()], &mut frontier)
        .expect("cycle resolves");

    assert_eq!(outcome.stopped, StopReason::FrontierClosed);
    assert_eq!(
        outcome
            .visited
            .iter()
            .map(|at| at.coordinate.as_str())
            .collect::<Vec<_>>(),
        vec!["example.com/a", "example.com/b"]
    );
    assert_eq!(frontier.visits, vec!["example.com/a", "example.com/b"]);
    assert!(outcome.visited.len() <= roster.len());
    assert_eq!(outcome.unresolved.len(), 0);
}

/// A frontier whose revision answer changes on every call cannot reopen a
/// closed repository: the resolver asks once per coordinate and reuses that
/// answer, so the bound stays the roster size rather than the call count.
#[test]
fn a_drifting_revision_answer_cannot_reopen_a_closed_repository() {
    let roster = roster(&["example.com/a", "example.com/b"]);
    let mut frontier = TableFrontier::new(&[
        ("example.com/a", &["example.com/b", "example.com/b"]),
        ("example.com/b", &["example.com/a", "example.com/b"]),
    ])
    .drifting();

    let outcome = DepResolver::new(&roster)
        .run(&["example.com/a".to_string()], &mut frontier)
        .expect("drifting revisions still close");

    assert_eq!(outcome.stopped, StopReason::FrontierClosed);
    assert_eq!(outcome.visited.len(), 2);
    assert_eq!(frontier.revision_calls, 2);
}

/// Two hops: the seed reaches the far repository through an intermediate, and
/// the hop column records the distance rather than the discovery order.
#[test]
fn a_two_hop_chain_reaches_the_far_repository() {
    let roster = roster(&["example.com/a", "example.com/b", "example.com/c"]);
    let mut frontier = TableFrontier::new(&[
        ("example.com/a", &["example.com/b"]),
        ("example.com/b", &["example.com/c"]),
    ]);

    let outcome = DepResolver::new(&roster)
        .run(&["example.com/a".to_string()], &mut frontier)
        .expect("chain resolves");

    let hops: Vec<(&str, u32)> = outcome
        .visited
        .iter()
        .map(|at| (at.coordinate.as_str(), at.hop))
        .collect();
    assert_eq!(
        hops,
        vec![
            ("example.com/a", 0),
            ("example.com/b", 1),
            ("example.com/c", 2)
        ]
    );
    assert_eq!(outcome.edges.len(), 2);
}

/// A target whose coordinates have no local checkout is a NAMED row, never a
/// network call and never a silent drop. Remote acquisition is the separate
/// remote-acquisition-policy decision.
#[test]
fn an_absent_checkout_emits_a_named_unresolved_row() {
    let roster = roster(&["example.com/a"]);
    let mut frontier = TableFrontier::new(&[(
        "example.com/a",
        &["example.com/absent", "./sibling", "../up", "/abs"],
    )]);

    let outcome = DepResolver::new(&roster)
        .run(&["example.com/a".to_string()], &mut frontier)
        .expect("absent targets resolve to rows");

    let rows: Vec<(&str, UnresolvedReason)> = outcome
        .unresolved
        .iter()
        .map(|row| (row.target.as_str(), row.reason))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("../up", UnresolvedReason::RelativePath),
            ("./sibling", UnresolvedReason::RelativePath),
            ("/abs", UnresolvedReason::RelativePath),
            ("example.com/absent", UnresolvedReason::NoLocalCheckout),
        ]
    );
    assert!(outcome.edges.is_empty());
}

/// A seed the roster has never heard of is the same named row, not an error
/// and not an empty answer.
#[test]
fn an_absent_seed_is_the_same_named_row() {
    let roster = roster(&["example.com/a"]);
    let mut frontier = TableFrontier::new(&[]);

    let outcome = DepResolver::new(&roster)
        .run(&["example.com/missing".to_string()], &mut frontier)
        .expect("absent seed resolves to a row");

    assert!(outcome.visited.is_empty());
    assert_eq!(outcome.unresolved.len(), 1);
    assert_eq!(outcome.unresolved[0].target, "example.com/missing");
    assert_eq!(
        outcome.unresolved[0].reason,
        UnresolvedReason::NoLocalCheckout
    );
}

/// Package-path targets are deeper than repository coordinates. The roster
/// match is the longest prefix that is a checkout, and a Go major-version
/// suffix is not part of the repository path.
#[test]
fn a_package_path_resolves_to_its_longest_prefix_checkout() {
    let roster = roster(&["github.com/grafana/loki", "github.com/grafana/loki/tools"]);
    let mut frontier = TableFrontier::new(&[]);
    let resolver = DepResolver::new(&roster);

    let hit = |target: &str| {
        resolver
            .locate(target)
            .expect("target resolves")
            .coordinate
            .clone()
    };
    assert_eq!(hit("github.com/grafana/loki/pkg/util"), "github.com/grafana/loki");
    assert_eq!(
        hit("github.com/grafana/loki/tools/lambda"),
        "github.com/grafana/loki/tools"
    );
    assert_eq!(hit("github.com/grafana/loki/v3/pkg/x"), "github.com/grafana/loki");
    assert!(resolver.locate("github.com/grafana/mimir").is_none());
    let _ = &mut frontier;
}

/// The relation rows the resolver posts are the crate's ordinary signed
/// arrivals, so a program declaring dep_repo/dep_edge/dep_unresolved/dep_visited
/// can consume a crawl without a bespoke boundary.
#[test]
fn the_outcome_projects_to_signed_arrivals() {
    let roster = roster(&["example.com/a", "example.com/b"]);
    let mut frontier = TableFrontier::new(&[
        ("example.com/a", &["example.com/b", "example.com/absent"]),
    ]);

    let outcome = DepResolver::new(&roster)
        .run(&["example.com/a".to_string()], &mut frontier)
        .expect("crawl resolves");
    let arrivals = outcome.arrivals();

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for arrival in &arrivals {
        *counts.entry(arrival.rel.as_str()).or_default() += 1;
    }
    assert_eq!(counts.get("dep_repo").copied(), Some(2));
    assert_eq!(counts.get("dep_visited").copied(), Some(2));
    assert_eq!(counts.get("dep_edge").copied(), Some(1));
    assert_eq!(counts.get("dep_unresolved").copied(), Some(1));
}

/// The frontier-closure receipt over a real checkout root, e.g.
/// `DEP_RESOLVE_CORPUS=$HOME/orgs/grafana/repos DEP_RESOLVE_SEED=github.com/grafana/loki`.
#[test]
#[ignore]
fn a_local_corpus_frontier_closes() {
    let Ok(corpus) = std::env::var("DEP_RESOLVE_CORPUS") else {
        panic!("set DEP_RESOLVE_CORPUS to a checkout root");
    };
    let seed = std::env::var("DEP_RESOLVE_SEED").expect("set DEP_RESOLVE_SEED");
    let roster = LocalRepoRoster::scan_checkout_root(&corpus).expect("scan corpus");
    let mut frontier = GoModFrontier::new();

    let started = std::time::Instant::now();
    let outcome = DepResolver::new(&roster)
        .run(&[seed.clone()], &mut frontier)
        .expect("corpus crawl");
    let elapsed = started.elapsed();

    let mut reasons: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &outcome.unresolved {
        *reasons.entry(row.reason.as_str()).or_default() += 1;
    }
    let deepest = outcome.visited.iter().map(|at| at.hop).max().unwrap_or(0);
    println!("corpus       {corpus}");
    println!("seed         {seed}");
    println!("roster       {} coordinates", roster.len());
    println!("visited      {} (repo, rev)", outcome.visited.len());
    println!("deepest hop  {deepest}");
    println!("edges        {}", outcome.edges.len());
    println!("unresolved   {} {reasons:?}", outcome.unresolved.len());
    println!("stopped      {:?}", outcome.stopped);
    println!("wall         {:.2}s", elapsed.as_secs_f64());
    for at in &outcome.visited {
        println!("visit hop={} {} {}", at.hop, at.coordinate, at.revision);
    }

    assert_eq!(outcome.stopped, StopReason::FrontierClosed);
    assert!(outcome.visited.len() <= roster.len());
}
