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

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use sprefa_engine_rs::dep_resolve::{
    DepResolver, GoModFrontier, IDepFrontierSource, LocalRepo, LocalRepoRoster, StopReason,
    UnresolvedReason,
};
use sprefa_engine_rs::hosts::{decode_output, DepCrawlExecutor, HostLiveRunner, IHostExecutor};
use sprefa_engine_rs::types::{HostColumnPlan, HostPlanData, RelDelta, TickDeltas, Value};

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
    assert_eq!(
        hit("github.com/grafana/loki/pkg/util"),
        "github.com/grafana/loki"
    );
    assert_eq!(
        hit("github.com/grafana/loki/tools/lambda"),
        "github.com/grafana/loki/tools"
    );
    assert_eq!(
        hit("github.com/grafana/loki/v3/pkg/x"),
        "github.com/grafana/loki"
    );
    assert!(resolver.locate("github.com/grafana/mimir").is_none());
    let _ = &mut frontier;
}

/// The relation rows the resolver posts are the crate's ordinary signed
/// arrivals, so a program declaring dep_repo/dep_edge/dep_unresolved/dep_visited
/// can consume a crawl without a bespoke boundary.
#[test]
fn the_outcome_projects_to_signed_arrivals() {
    let roster = roster(&["example.com/a", "example.com/b"]);
    let mut frontier =
        TableFrontier::new(&[("example.com/a", &["example.com/b", "example.com/absent"])]);

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

// ═══ the host arm: dep_resolve reached through an authored `sh` decl ═════════

/// Two one-commit checkouts under one root, the shape
/// `goldens/multirepo_crawl/1_corpus.sh` pins, small enough to assert row by
/// row. Nothing here reaches the network.
fn checkout_root_fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sprefa_dep_crawl_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let write_repo = |slug: &str, gomod: &str| {
        let repo = root.join(slug);
        std::fs::create_dir_all(&repo).expect("fixture directory");
        std::fs::write(repo.join("go.mod"), gomod).expect("write go.mod");
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
            vec!["commit", "-qm", "pinned fixture"],
        ] {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(&args)
                .env("GIT_AUTHOR_NAME", "sprefa-engine-rs")
                .env("GIT_AUTHOR_EMAIL", "sprefa-engine-rs@example.invalid")
                .env("GIT_COMMITTER_NAME", "sprefa-engine-rs")
                .env("GIT_COMMITTER_EMAIL", "sprefa-engine-rs@example.invalid")
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };
    write_repo(
        "alpha",
        "module example.com/alpha\n\ngo 1.21\n\nrequire (\nexample.com/shared v1.2.0\ngithub.com/pkg/errors v0.9.1\n)\n",
    );
    write_repo(
        "shared",
        "module example.com/shared\n\ngo 1.21\n\nrequire (\ngithub.com/pkg/errors v0.9.1\n)\n",
    );
    root
}

fn crawl_inputs(root: &std::path::Path, seed: &str, frontier: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("checkout_root".to_string(), root.display().to_string()),
        ("seed".to_string(), seed.to_string()),
        ("frontier".to_string(), frontier.to_string()),
    ])
}

fn column(name: &str, column_type: &str) -> HostColumnPlan {
    HostColumnPlan {
        name: name.to_string(),
        column_type: column_type.to_string(),
    }
}

fn columns(names: &[(&str, &str)]) -> Vec<HostColumnPlan> {
    names
        .iter()
        .map(|(name, kind)| column(name, kind))
        .collect()
}

/// The plan the emitter writes for a `sh dep_crawl_*` decl, template included:
/// `4_dep_crawl.dl6`'s templates exit 3, so a row can only come from the arm.
fn crawl_plan(name: &str, outputs: &[(&str, &str)]) -> HostPlanData {
    HostPlanData {
        name: name.to_string(),
        inputs: columns(&[
            ("checkout_root", "text"),
            ("seed", "text"),
            ("frontier", "text"),
        ]),
        outputs: columns(outputs),
        template: format!("echo '{name} {{checkout_root}} {{seed}} {{frontier}} is linked in-process' >&2; exit 3"),
        demand_rel: format!("__host_demand_{name}"),
        response_rel: format!("__host_response_{name}"),
        execution: "shell".to_string(),
    }
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

/// Every projection of one crawl, decoded through the runtime's own
/// `decode_output`, so the test reads exactly what a response rel receives.
#[test]
fn the_host_arm_projects_one_crawl_into_four_relations() {
    let root = checkout_root_fixture();
    let executor = DepCrawlExecutor::default();
    let inputs = crawl_inputs(&root, "example.com/alpha", "go_mod");
    let answer = |host: &str, outputs: &[(&str, &str)]| {
        let stdout = executor
            .run(host, "deliberately not a shell command", &inputs)
            .expect("linked crawl");
        decode_output(host, &stdout, &columns(outputs)).expect("decode")
    };

    let repos = answer(
        "dep_crawl_repo",
        &[("coordinate", "text"), ("root", "text")],
    );
    assert_eq!(
        repos,
        vec![
            vec![
                text("example.com/alpha"),
                text(&root.join("alpha").display().to_string())
            ],
            vec![
                text("example.com/shared"),
                text(&root.join("shared").display().to_string())
            ],
        ]
    );

    let visited = answer(
        "dep_crawl_visited",
        &[("repo", "text"), ("rev", "text"), ("hop", "int")],
    );
    let hops: Vec<(&Value, &Value)> = visited.iter().map(|row| (&row[0], &row[2])).collect();
    assert_eq!(
        hops,
        vec![
            (&text("example.com/alpha"), &Value::Integer(0)),
            (&text("example.com/shared"), &Value::Integer(1)),
        ]
    );

    let edges = answer(
        "dep_crawl_edge",
        &[
            ("from_repo", "text"),
            ("from_rev", "text"),
            ("target", "text"),
            ("to_repo", "text"),
        ],
    );
    assert_eq!(edges.len(), 1, "{edges:?}");
    assert_eq!(edges[0][0], text("example.com/alpha"));
    assert_eq!(edges[0][2], text("example.com/shared"));
    assert_eq!(edges[0][3], text("example.com/shared"));
    assert_eq!(visited[0][1], edges[0][1], "the edge carries alpha's HEAD");

    let unresolved = answer(
        "dep_crawl_unresolved",
        &[
            ("from_repo", "text"),
            ("target", "text"),
            ("reason", "text"),
        ],
    );
    assert_eq!(
        unresolved,
        vec![
            vec![
                text("example.com/alpha"),
                text("github.com/pkg/errors"),
                text("no_local_checkout")
            ],
            vec![
                text("example.com/shared"),
                text("github.com/pkg/errors"),
                text("no_local_checkout")
            ],
        ]
    );
    std::fs::remove_dir_all(root).expect("remove fixture root");
}

/// THE ARM ITSELF. The four ruled names carry `execution: shell` and a template
/// that exits 3, so rows arriving through `HostLiveRunner` prove the runner
/// routed them in-process rather than to `ShellExecutor`.
#[test]
fn the_four_ruled_names_reach_the_linked_arm_through_the_host_plan() {
    let root = checkout_root_fixture();
    let plans = vec![
        crawl_plan(
            "dep_crawl_repo",
            &[("coordinate", "text"), ("root", "text")],
        ),
        crawl_plan(
            "dep_crawl_visited",
            &[("repo", "text"), ("rev", "text"), ("hop", "int")],
        ),
        crawl_plan(
            "dep_crawl_edge",
            &[
                ("from_repo", "text"),
                ("from_rev", "text"),
                ("target", "text"),
                ("to_repo", "text"),
            ],
        ),
        crawl_plan(
            "dep_crawl_unresolved",
            &[
                ("from_repo", "text"),
                ("target", "text"),
                ("reason", "text"),
            ],
        ),
    ];
    let mut rel_columns: HashMap<String, Vec<String>> = HashMap::new();
    let mut rels = Vec::new();
    for plan in &plans {
        let demand: Vec<String> = ["identity_digest", "witness_digest"]
            .iter()
            .map(|name| (*name).to_string())
            .chain(plan.inputs.iter().map(|input| input.name.clone()))
            .collect();
        let response: Vec<String> = ["witness_digest", "ordinal"]
            .iter()
            .map(|name| (*name).to_string())
            .chain(plan.inputs.iter().map(|input| input.name.clone()))
            .chain(plan.outputs.iter().map(|output| output.name.clone()))
            .collect();
        rel_columns.insert(plan.demand_rel.clone(), demand);
        rel_columns.insert(plan.response_rel.clone(), response);
        rels.push(RelDelta {
            rel: plan.demand_rel.clone(),
            add: vec![vec![
                text(&format!("identity|{}", plan.name)),
                text(&format!("witness|{}", plan.name)),
                text(&root.display().to_string()),
                text("example.com/alpha"),
                text("go_mod"),
            ]],
            del: vec![],
        });
    }
    let deltas = TickDeltas {
        rels,
        carry_pending: false,
    };

    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("every plan has an executor");
    let arrivals = runner.collect(&deltas).expect("the linked arm answers");

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for arrival in &arrivals {
        *counts.entry(arrival.rel.as_str()).or_default() += 1;
    }
    assert_eq!(
        counts.get("__host_response_dep_crawl_repo").copied(),
        Some(2)
    );
    assert_eq!(
        counts.get("__host_response_dep_crawl_visited").copied(),
        Some(2)
    );
    assert_eq!(
        counts.get("__host_response_dep_crawl_edge").copied(),
        Some(1)
    );
    assert_eq!(
        counts.get("__host_response_dep_crawl_unresolved").copied(),
        Some(2)
    );
    std::fs::remove_dir_all(root).expect("remove fixture root");
}

/// An unlinked frontier spelling stops by name. A silent empty answer would
/// read as "this corpus has no dependencies".
#[test]
fn an_unlinked_frontier_kind_is_a_named_stop() {
    let root = checkout_root_fixture();
    let failure = DepCrawlExecutor::default()
        .run(
            "dep_crawl_edge",
            "",
            &crawl_inputs(&root, "example.com/alpha", "ripgrep"),
        )
        .expect_err("an unlinked frontier must stop");
    assert_eq!(failure.host, "dep_crawl_edge");
    assert!(failure.message.contains("ripgrep"), "{failure}");
    assert!(failure.message.contains("go_mod"), "{failure}");
    std::fs::remove_dir_all(root).expect("remove fixture root");
}

/// A demand row missing the checkout root is named, not defaulted to the
/// process cwd: crawling the wrong tree is worse than not crawling.
#[test]
fn a_missing_checkout_root_is_a_named_stop() {
    let mut inputs = BTreeMap::new();
    inputs.insert("seed".to_string(), "example.com/alpha".to_string());
    let failure = DepCrawlExecutor::default()
        .run("dep_crawl_repo", "", &inputs)
        .expect_err("a missing input must stop");
    assert!(failure.message.contains("checkout_root"), "{failure}");
}

// ═══ the manifest leg: the roster reads go.mod / package.json at HEAD ════════

/// One commit per checkout, then an arbitrary set of files committed under the
/// same init/add/commit sequence `checkout_root_fixture` uses, so a follow-up
/// overwrite in the worktree is what a HEAD read must ignore.
fn git_checkout(root: &std::path::Path, slug: &str, files: &[(&str, &str)]) -> PathBuf {
    let repo = root.join(slug);
    std::fs::create_dir_all(&repo).expect("fixture directory");
    for (name, content) in files {
        std::fs::write(repo.join(name), content).expect("write manifest");
    }
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec!["commit", "-qm", "pinned fixture"],
    ] {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(&args)
            .env("GIT_AUTHOR_NAME", "sprefa-engine-rs")
            .env("GIT_AUTHOR_EMAIL", "sprefa-engine-rs@example.invalid")
            .env("GIT_COMMITTER_NAME", "sprefa-engine-rs")
            .env("GIT_COMMITTER_EMAIL", "sprefa-engine-rs@example.invalid")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    repo
}

/// The roster's manifest leg must read at the same HEAD every target leg reads.
/// A committed manifest overwritten in the worktree still answers to the
/// committed coordinate; the dirty spelling resolves nothing. Before the fix
/// this test is RED the other way round: the dirty coordinate resolves and the
/// committed one does not, because the old code read the live worktree.
#[test]
fn the_roster_reads_manifests_at_head_not_the_worktree() {
    let root = std::env::temp_dir().join(format!(
        "sprefa_dep_manifest_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("fixture root");

    let go = git_checkout(
        &root,
        "gomod",
        &[("go.mod", "module example.com/committed\n\ngo 1.21\n")],
    );
    std::fs::write(go.join("go.mod"), "module example.com/dirty\n\ngo 1.21\n")
        .expect("dirty the worktree go.mod");

    let pkg = git_checkout(
        &root,
        "pkgjson",
        &[(
            "package.json",
            "{\"name\":\"@scope/committed\",\"version\":\"1.0.0\"}\n",
        )],
    );
    std::fs::write(
        pkg.join("package.json"),
        "{\"name\":\"@scope/dirty\",\"version\":\"1.0.0\"}\n",
    )
    .expect("dirty the worktree package.json");

    let plain = root.join("plain");
    std::fs::create_dir_all(&plain).expect("plain directory");
    std::fs::write(plain.join("go.mod"), "module example.com/plain\n\ngo 1.21\n")
        .expect("plain go.mod");

    let roster = LocalRepoRoster::scan_checkout_root(&root).expect("scan");

    assert!(roster.locate("example.com/committed").is_some());
    assert!(roster.locate("example.com/dirty").is_none());
    assert!(roster.locate("@scope/committed").is_some());
    assert!(roster.locate("@scope/dirty").is_none());

    assert!(roster.locate("plain").is_some());
    assert!(roster.locate("example.com/plain").is_none());

    std::fs::remove_dir_all(root).expect("remove fixture root");
}
