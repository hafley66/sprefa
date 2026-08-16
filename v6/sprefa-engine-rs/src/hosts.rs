// Live host execution for the batch runtime: the Rust port of the tsv2
// HostRunner contract (serve/1_hosts.ts), sharing its emitted HostPlanData.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::{Arc, LazyLock, Mutex};

use crate::change_facts::{
    IRevisionDiffer, RevisionDiff, SoopyRevisionDiffer, GIT_CHANGED_LINE_COLUMNS,
    GIT_CHANGE_COLUMNS, GIT_RENAME_COLUMNS,
};
use crate::dep_resolve::{
    DepResolveOutcome, DepResolver, GoModFrontier, IDepFrontierSource, LocalRepoRoster,
    SpecifierFrontier, DEP_EDGE_COLUMNS, DEP_REPO_COLUMNS, DEP_UNRESOLVED_COLUMNS,
    DEP_VISITED_COLUMNS,
};
use crate::types::{
    Arrival, ArrivalSign, BoundaryError, HostColumnPlan, HostPlanData, ScalarSeam, ScalarValue,
    TickDeltas, Value,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub host: String,
    pub message: String,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sh host '{}': {}", self.host, self.message)
    }
}

pub trait IHostExecutor: Sync {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError>;
}

pub fn executor_for(execution: &str) -> Option<&'static dyn IHostExecutor> {
    match execution {
        "shell" => Some(&ShellExecutor),
        // The linked twin: sprefa-extract runs in this process, no child spawn.
        "sprefa_extract" | "sprefa_extract_repo" => Some(&SprefaExtractExecutor),
        _ => None,
    }
}

// `sh files*` is an established emitted HostPlanData contract. Keeping its
// execution tag and template byte-for-byte preserves the TS/runtime ABI; the
// Rust live runner recognizes these four ruled names and delegates their Git
// mechanics to Soopy instead of spawning the emitted pipeline.
fn executor_for_plan(plan: &HostPlanData) -> Option<&'static dyn IHostExecutor> {
    if plan.execution == "shell"
        && matches!(
            plan.name.as_str(),
            "files" | "files_at" | "repo_files" | "repo_files_at"
        )
    {
        return Some(&SoopyFilesExecutor);
    }
    if plan.execution == "shell" && DEP_CRAWL_HOSTS.contains(&plan.name.as_str()) {
        return Some(&*DEP_CRAWL);
    }
    if plan.execution == "shell" && GIT_REF_HOSTS.contains(&plan.name.as_str()) {
        return Some(&*GIT_REFS);
    }
    if plan.execution == "shell" && GIT_REVISION_HOSTS.contains(&plan.name.as_str()) {
        return Some(&*GIT_REVISIONS);
    }
    if plan.execution == "shell" && GIT_CHANGE_HOSTS.contains(&plan.name.as_str()) {
        return Some(&*GIT_CHANGES);
    }
    executor_for(&plan.execution)
}

// Executors whose invocations fold across identical (execution, template,
// inputs), the ApplicativeExecutors set in serve/1_hosts.ts.
fn is_applicative(execution: &str) -> bool {
    matches!(execution, "sprefa_extract" | "sprefa_extract_repo")
}

pub struct ShellExecutor;

impl IHostExecutor for ShellExecutor {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command_line)
            .envs(env)
            .output()
            .map_err(|failure| HostError {
                host: host.to_string(),
                message: format!("spawn failed for `{command_line}`: {failure}"),
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(HostError {
                host: host.to_string(),
                message: format!("exited {}: {}", output.status, stderr.trim()),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

pub struct SprefaExtractExecutor;

/// Linked implementation of the four ruled Git file-feed hosts. The emitted
/// templates still document the portable shell contract; this executor reads
/// exactly the same input columns and emits the same whitespace grid.
pub struct SoopyFilesExecutor;

impl IHostExecutor for SoopyFilesExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let glob = env
            .get("glob")
            .cloned()
            .ok_or_else(|| named("missing required host input `glob`".to_string()))?;
        let cwd =
            std::env::current_dir().map_err(|error| named(format!("read process cwd: {error}")))?;
        let root = match host {
            "files" | "files_at" => cwd.clone(),
            "repo_files" | "repo_files_at" => env
                .get("repo")
                .map(std::path::PathBuf::from)
                .ok_or_else(|| named("missing required host input `repo`".to_string()))?,
            _ => return Err(named("not a Soopy file host".to_string())),
        };
        let revision = match host {
            "files" | "repo_files" => soopy::Revision::Worktree,
            "files_at" | "repo_files_at" => soopy::Revision::Named(std::sync::Arc::from(
                env.get("rev")
                    .ok_or_else(|| named("missing required host input `rev`".to_string()))?
                    .as_str(),
            )),
            _ => unreachable!(),
        };
        let repository =
            soopy::discover(root).map_err(|error| named(format!("open repository: {error}")))?;
        let mut tree = soopy::SourceTree::open(repository);
        let entries = tree
            .git_files_from(
                &soopy::GitFilesQuery {
                    revision,
                    pathspecs: vec![glob],
                },
                &cwd,
            )
            .map_err(|error| named(format!("enumerate tracked files: {error}")))?;
        let repo_root = tree.repository().root.clone();
        let mut lines = Vec::with_capacity(entries.len());
        for entry in entries {
            let soopy::ContentId::GitBlob(oid) = entry.content else {
                return Err(named(
                    "Git file feed returned a non-Git content id".to_string(),
                ));
            };
            let path = if matches!(host, "files" | "files_at") {
                path_from_cwd(&cwd, &repo_root, entry.source.path.0.as_ref())
                    .map_err(|error| named(format!("render cwd-relative path: {error}")))?
            } else {
                entry.source.path.0.to_string()
            };
            lines.push(format!("{path} {}", oid.0));
        }
        Ok(lines.join("\n"))
    }
}

fn path_from_cwd(
    cwd: &Path,
    repository_root: &Path,
    repository_path: &str,
) -> std::io::Result<String> {
    let cwd = cwd.canonicalize()?;
    let target = repository_root.join(repository_path);
    let from: Vec<_> = cwd.components().collect();
    let to: Vec<_> = target.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = std::path::PathBuf::new();
    for _ in &from[common..] {
        result.push("..");
    }
    for component in &to[common..] {
        if let Component::Normal(part) = component {
            result.push(part);
        }
    }
    Ok(result.to_string_lossy().replace('\\', "/"))
}

// ═══ the dependency crawl, linked the way the Git file feeds are ════════════

// Four names, ONE frontier closure: the memo is keyed on the crawl's inputs,
// so a program declaring all four pays for one traversal.
const DEP_CRAWL_HOSTS: &[&str] = &[
    "dep_crawl_repo",
    "dep_crawl_visited",
    "dep_crawl_edge",
    "dep_crawl_unresolved",
];

static DEP_CRAWL: LazyLock<DepCrawlExecutor> = LazyLock::new(DepCrawlExecutor::default);

#[derive(Default)]
pub struct DepCrawlExecutor {
    crawls: Mutex<BTreeMap<String, Arc<DepResolveOutcome>>>,
}

impl DepCrawlExecutor {
    fn crawl(
        &self,
        host: &str,
        checkout_root: &str,
        seed: &str,
        frontier: &str,
    ) -> Result<Arc<DepResolveOutcome>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let key = format!("{frontier}|{checkout_root}|{seed}");
        if let Some(memo) = self.crawls.lock().expect("dep crawl memo").get(&key) {
            return Ok(memo.clone());
        }
        let roster = LocalRepoRoster::scan_checkout_root(checkout_root)
            .map_err(|error| named(format!("scan checkout root {checkout_root}: {error}")))?;
        // An unlinked spelling is a named stop, never a silent empty answer.
        let mut source: Box<dyn IDepFrontierSource> = match frontier {
            "" | "go_mod" => Box::new(GoModFrontier::new()),
            "specifier" => Box::new(SpecifierFrontier::default()),
            other => {
                return Err(named(format!(
                    "frontier '{other}' is not linked; the kinds are go_mod and specifier"
                )))
            }
        };
        let outcome = Arc::new(
            DepResolver::new(&roster)
                .run(&[seed.to_string()], source.as_mut())
                .map_err(|error| named(format!("crawl from {seed}: {error}")))?,
        );
        self.crawls
            .lock()
            .expect("dep crawl memo")
            .insert(key, outcome.clone());
        Ok(outcome)
    }
}

// One object per output row, named by the host's own declared columns, so a
// linked arm and the decl it answers cannot drift apart.
fn host_row(columns: &[&str], values: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Object(
        columns
            .iter()
            .map(|name| (*name).to_string())
            .zip(values)
            .collect(),
    )
}

// The column names come from dep_resolve.rs's own constants, so a host output
// column and the module's relation declaration cannot drift apart.
fn dep_crawl_row(columns: &[&str], values: Vec<String>) -> serde_json::Value {
    host_row(
        columns,
        values.into_iter().map(serde_json::Value::String).collect(),
    )
}

fn ndjson(rows: &[serde_json::Value]) -> String {
    rows.iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<String>>()
        .join("\n")
}

impl IHostExecutor for DepCrawlExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let required = |name: &str| {
            env.get(name)
                .cloned()
                .ok_or_else(|| named(format!("missing required host input `{name}`")))
        };
        let checkout_root = required("checkout_root")?;
        let seed = required("seed")?;
        let frontier = env.get("frontier").cloned().unwrap_or_default();
        let outcome = self.crawl(host, &checkout_root, &seed, &frontier)?;
        let rows: Vec<serde_json::Value> = match host {
            "dep_crawl_repo" => outcome
                .repos
                .iter()
                .map(|repo| {
                    dep_crawl_row(
                        DEP_REPO_COLUMNS,
                        vec![
                            repo.coordinate.clone(),
                            repo.root.to_string_lossy().to_string(),
                        ],
                    )
                })
                .collect(),
            "dep_crawl_visited" => outcome
                .visited
                .iter()
                .map(|at| {
                    dep_crawl_row(
                        DEP_VISITED_COLUMNS,
                        vec![
                            at.coordinate.clone(),
                            at.revision.clone(),
                            at.hop.to_string(),
                        ],
                    )
                })
                .collect(),
            "dep_crawl_edge" => outcome
                .edges
                .iter()
                .map(|edge| {
                    dep_crawl_row(
                        DEP_EDGE_COLUMNS,
                        vec![
                            edge.from_repo.clone(),
                            edge.from_revision.clone(),
                            edge.target.clone(),
                            edge.to_repo.clone(),
                        ],
                    )
                })
                .collect(),
            "dep_crawl_unresolved" => outcome
                .unresolved
                .iter()
                .map(|row| {
                    dep_crawl_row(
                        DEP_UNRESOLVED_COLUMNS,
                        vec![
                            row.from_repo.clone(),
                            row.target.clone(),
                            row.reason.as_str().to_string(),
                        ],
                    )
                })
                .collect(),
            _ => return Err(named("not a dependency-crawl host".to_string())),
        };
        Ok(ndjson(&rows))
    }
}

// ═══ refs, tags, and the revision graph, linked the same way ════════════════

// Two names, ONE `for-each-ref` pass: a program declaring both settles one
// snapshot per repository.
const GIT_REF_HOSTS: &[&str] = &["git_ref", "git_tag"];

// Three names, ONE revision-graph answer per (repo, rev_a, rev_b). Git has no
// batch form for the pairwise relations, so the memo is what caps the spawns.
const GIT_REVISION_HOSTS: &[&str] = &["git_merge_base", "git_ahead_behind", "git_ancestor"];

pub const GIT_REF_COLUMNS: &[&str] = &["ref_name", "kind", "target_sha"];
pub const GIT_TAG_COLUMNS: &[&str] = &["tag_name", "target_sha", "tagged_at", "annotated"];
pub const GIT_MERGE_BASE_COLUMNS: &[&str] = &["base_sha"];
pub const GIT_AHEAD_BEHIND_COLUMNS: &[&str] = &["ahead_count", "behind_count"];
pub const GIT_ANCESTOR_COLUMNS: &[&str] = &["ancestor_sha", "descendant_sha"];

const TAG_PREFIX: &str = "refs/tags/";

static GIT_REFS: LazyLock<GitRefExecutor> = LazyLock::new(GitRefExecutor::default);
static GIT_REVISIONS: LazyLock<GitRevisionExecutor> = LazyLock::new(GitRevisionExecutor::default);

#[derive(Default)]
pub struct GitRefExecutor {
    snapshots: Mutex<BTreeMap<String, Arc<soopy::RefSnapshot>>>,
}

// `kind` names the ref's namespace, never the Git object type: every branch tip
// is a `commit` object, so the object type cannot tell branches from tags.
fn ref_kind(name: &str) -> &'static str {
    if name.starts_with("refs/heads/") {
        "branch"
    } else if name.starts_with(TAG_PREFIX) {
        "tag"
    } else if name.starts_with("refs/remotes/") {
        "remote"
    } else {
        "other"
    }
}

// An annotated tag's ref points at the tag object; `peeled` carries the commit.
fn ref_target(observation: &soopy::RefObservation) -> String {
    observation
        .peeled
        .as_ref()
        .unwrap_or(&observation.direct)
        .0
        .to_string()
}

impl GitRefExecutor {
    fn snapshot(&self, host: &str, repo: &str) -> Result<Arc<soopy::RefSnapshot>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        if let Some(memo) = self.snapshots.lock().expect("git ref memo").get(repo) {
            return Ok(memo.clone());
        }
        let repository = soopy::discover(std::path::PathBuf::from(repo))
            .map_err(|error| named(format!("open repository {repo}: {error}")))?;
        let query = soopy::RefQuery {
            repository: repository.identity.clone(),
            namespace: Arc::from(""),
            name: None,
            pattern: None,
        };
        let snapshot = Arc::new(
            soopy::Refs::open(repository)
                .snapshot(&query)
                .map_err(|error| named(format!("enumerate refs in {repo}: {error}")))?,
        );
        self.snapshots
            .lock()
            .expect("git ref memo")
            .insert(repo.to_string(), snapshot.clone());
        Ok(snapshot)
    }
}

impl IHostExecutor for GitRefExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let repo = env
            .get("repo")
            .cloned()
            .ok_or_else(|| named("missing required host input `repo`".to_string()))?;
        let snapshot = self.snapshot(host, &repo)?;
        let rows: Vec<serde_json::Value> = match host {
            "git_ref" => {
                let mut rows: Vec<serde_json::Value> = snapshot
                    .refs
                    .iter()
                    .map(|observation| {
                        host_row(
                            GIT_REF_COLUMNS,
                            vec![
                                serde_json::Value::String(observation.name.to_string()),
                                serde_json::Value::String(ref_kind(&observation.name).to_string()),
                                serde_json::Value::String(ref_target(observation)),
                            ],
                        )
                    })
                    .collect();
                // An unborn HEAD names a branch with no commit, so it has no
                // target_sha and produces no row.
                if let Some(target) = &snapshot.head_target {
                    rows.push(host_row(
                        GIT_REF_COLUMNS,
                        vec![
                            serde_json::Value::String("HEAD".to_string()),
                            serde_json::Value::String("head".to_string()),
                            serde_json::Value::String(target.0.to_string()),
                        ],
                    ));
                }
                rows
            }
            // A lightweight tag has no tag object and so no tag date;
            // `annotated` keeps a zero `tagged_at` from reading as the epoch.
            "git_tag" => snapshot
                .refs
                .iter()
                .filter_map(|observation| {
                    let tag_name = observation.name.strip_prefix(TAG_PREFIX)?;
                    let tagged_at = observation
                        .tag
                        .as_ref()
                        .map(|tag| tag.tagger.when)
                        .unwrap_or(0);
                    Some(host_row(
                        GIT_TAG_COLUMNS,
                        vec![
                            serde_json::Value::String(tag_name.to_string()),
                            serde_json::Value::String(ref_target(observation)),
                            serde_json::Value::Number(tagged_at.into()),
                            serde_json::Value::Bool(observation.tag.is_some()),
                        ],
                    ))
                })
                .collect(),
            _ => return Err(named("not a Git ref host".to_string())),
        };
        Ok(ndjson(&rows))
    }
}

#[derive(Default)]
pub struct GitRevisionExecutor {
    pairs: Mutex<BTreeMap<String, Arc<soopy::RevisionGraphResult>>>,
}

impl GitRevisionExecutor {
    fn pair(
        &self,
        host: &str,
        repo: &str,
        rev_a: &str,
        rev_b: &str,
    ) -> Result<Arc<soopy::RevisionGraphResult>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let key = format!("{repo}|{rev_a}|{rev_b}");
        if let Some(memo) = self.pairs.lock().expect("git revision memo").get(&key) {
            return Ok(memo.clone());
        }
        let repository = soopy::discover(std::path::PathBuf::from(repo))
            .map_err(|error| named(format!("open repository {repo}: {error}")))?;
        let identity = repository.identity.clone();
        let graph = soopy::RevisionGraph::open(repository);
        let empty = soopy::RevisionGraphQuery {
            repository: identity.clone(),
            resolve: Vec::new(),
            parents: Vec::new(),
            ancestry: Vec::new(),
            merge_bases: Vec::new(),
            ahead_behind: Vec::new(),
            walks: Vec::new(),
        };
        // The pairwise questions take commit ids, so the two spellings resolve
        // first; the answers then ride ONE batched query.
        let resolved = graph
            .query(&soopy::RevisionGraphQuery {
                resolve: vec![
                    soopy::Revision::Named(Arc::from(rev_a)),
                    soopy::Revision::Named(Arc::from(rev_b)),
                ],
                ..empty.clone()
            })
            .map_err(|error| named(format!("resolve {rev_a} and {rev_b} in {repo}: {error}")))?;
        let mut commits = Vec::with_capacity(2);
        for (spelling, resolution) in [rev_a, rev_b].iter().zip(&resolved.resolutions) {
            // Zero rows would read as "no shared history", so an unresolvable
            // revision stops by name; a shallow boundary answers locally.
            match resolution {
                soopy::RevisionResolution::Present(oid)
                | soopy::RevisionResolution::ShallowBoundary(oid) => commits.push(oid.clone()),
                soopy::RevisionResolution::Absent => {
                    return Err(named(format!(
                        "revision '{spelling}' does not resolve in {repo}"
                    )))
                }
                soopy::RevisionResolution::CorruptObject => {
                    return Err(named(format!(
                        "revision '{spelling}' names a corrupt object in {repo}"
                    )))
                }
            }
        }
        let (left, right) = (commits[0].clone(), commits[1].clone());
        // Both directions, so `git_ancestor` answers the relation rather than
        // one ordered question. Two equal revisions ask it once.
        let mut ancestry = vec![(left.clone(), right.clone())];
        if left != right {
            ancestry.push((right.clone(), left.clone()));
        }
        let answer = Arc::new(
            graph
                .query(&soopy::RevisionGraphQuery {
                    ancestry,
                    merge_bases: vec![(left.clone(), right.clone())],
                    ahead_behind: vec![(left, right)],
                    ..empty
                })
                .map_err(|error| {
                    named(format!(
                        "revision graph for {rev_a} and {rev_b} in {repo}: {error}"
                    ))
                })?,
        );
        self.pairs
            .lock()
            .expect("git revision memo")
            .insert(key, answer.clone());
        Ok(answer)
    }
}

impl IHostExecutor for GitRevisionExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let required = |name: &str| {
            env.get(name)
                .cloned()
                .ok_or_else(|| named(format!("missing required host input `{name}`")))
        };
        let repo = required("repo")?;
        let rev_a = required("rev_a")?;
        let rev_b = required("rev_b")?;
        let answer = self.pair(host, &repo, &rev_a, &rev_b)?;
        let rows: Vec<serde_json::Value> = match host {
            // Two commits with no shared history have no merge base, and that
            // is zero rows rather than an error.
            "git_merge_base" => answer
                .merge_bases
                .iter()
                .flat_map(|pair| pair.bases.iter())
                .map(|base| {
                    host_row(
                        GIT_MERGE_BASE_COLUMNS,
                        vec![serde_json::Value::String(base.0.to_string())],
                    )
                })
                .collect(),
            "git_ahead_behind" => answer
                .ahead_behind
                .iter()
                .map(|counts| {
                    host_row(
                        GIT_AHEAD_BEHIND_COLUMNS,
                        vec![
                            serde_json::Value::Number((counts.ahead as i64).into()),
                            serde_json::Value::Number((counts.behind as i64).into()),
                        ],
                    )
                })
                .collect(),
            "git_ancestor" => answer
                .ancestry
                .iter()
                .filter(|edge| edge.is_ancestor)
                .map(|edge| {
                    host_row(
                        GIT_ANCESTOR_COLUMNS,
                        vec![
                            serde_json::Value::String(edge.ancestor.0.to_string()),
                            serde_json::Value::String(edge.descendant.0.to_string()),
                        ],
                    )
                })
                .collect(),
            _ => return Err(named("not a Git revision host".to_string())),
        };
        Ok(ndjson(&rows))
    }
}

// ═══ the rev-pair change plane, linked the same way ═════════════════════════

// Three names, ONE diff per (repo, rev_base, rev_head): the two tree listings
// and every blob read are settled once and read three ways.
const GIT_CHANGE_HOSTS: &[&str] = &["git_change", "git_rename", "git_changed_line"];

static GIT_CHANGES: LazyLock<ChangeFactExecutor> = LazyLock::new(ChangeFactExecutor::default);

#[derive(Default)]
pub struct ChangeFactExecutor {
    diffs: Mutex<BTreeMap<String, Arc<RevisionDiff>>>,
    differ: SoopyRevisionDiffer,
}

impl ChangeFactExecutor {
    fn diff(
        &self,
        host: &str,
        repo: &str,
        rev_base: &str,
        rev_head: &str,
    ) -> Result<Arc<RevisionDiff>, HostError> {
        let key = format!("{repo}|{rev_base}|{rev_head}");
        if let Some(memo) = self.diffs.lock().expect("change fact memo").get(&key) {
            return Ok(memo.clone());
        }
        // A revision that does not resolve is a named stop: zero rows would
        // read as "these two trees are identical", a different fact.
        let answer = Arc::new(self.differ.diff(repo, rev_base, rev_head).map_err(|error| {
            HostError {
                host: host.to_string(),
                message: format!("diff {rev_base}..{rev_head} in {repo}: {error:#}"),
            }
        })?);
        self.diffs
            .lock()
            .expect("change fact memo")
            .insert(key, answer.clone());
        Ok(answer)
    }
}

impl IHostExecutor for ChangeFactExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let required = |name: &str| {
            env.get(name)
                .cloned()
                .ok_or_else(|| named(format!("missing required host input `{name}`")))
        };
        let repo = required("repo")?;
        let rev_base = required("rev_base")?;
        let rev_head = required("rev_head")?;
        let answer = self.diff(host, &repo, &rev_base, &rev_head)?;
        let rows: Vec<serde_json::Value> = match host {
            "git_change" => answer
                .changes
                .iter()
                .map(|change| {
                    host_row(
                        GIT_CHANGE_COLUMNS,
                        vec![
                            serde_json::Value::String(change.kind.as_str().to_string()),
                            serde_json::Value::String(change.path.clone()),
                        ],
                    )
                })
                .collect(),
            "git_rename" => answer
                .renames
                .iter()
                .map(|rename| {
                    host_row(
                        GIT_RENAME_COLUMNS,
                        vec![
                            serde_json::Value::String(rename.path_from.clone()),
                            serde_json::Value::String(rename.path_to.clone()),
                        ],
                    )
                })
                .collect(),
            "git_changed_line" => answer
                .changed_lines
                .iter()
                .map(|line| {
                    host_row(
                        GIT_CHANGED_LINE_COLUMNS,
                        vec![
                            serde_json::Value::String(line.path.clone()),
                            serde_json::Value::Number(line.line_number.into()),
                        ],
                    )
                })
                .collect(),
            _ => return Err(named("not a change-fact host".to_string())),
        };
        Ok(ndjson(&rows))
    }
}

impl IHostExecutor for SprefaExtractExecutor {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        _env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let tokens = shell_tokens(command_line)
            .map_err(|message| named(format!("unparseable command `{command_line}`: {message}")))?;
        let mut rest = tokens.iter();
        match rest.next() {
            Some(first) if first == "$DL_EXTRACT_BIN" || first.ends_with("extract") => {}
            other => {
                return Err(named(format!(
                    "expected the extract binary first in `{command_line}`, got {other:?}"
                )))
            }
        }
        let mut family: Option<String> = None;
        let mut want_file_fact = false;
        let mut path: Option<String> = None;
        while let Some(token) = rest.next() {
            if token == "--file-fact" {
                want_file_fact = true;
            } else if token == "--family" {
                family = Some(
                    rest.next()
                        .ok_or_else(|| named("--family needs a value".to_string()))?
                        .clone(),
                );
            } else if let Some(value) = token.strip_prefix("--family=") {
                family = Some(value.to_string());
            } else if token.starts_with("--") {
                // The in-process twin covers the per-file surface; an
                // unrecognized flag is a named stop, never a silent shell fall-back.
                return Err(named(format!("flag `{token}` is not linked in-process")));
            } else {
                path = Some(token.clone());
            }
        }
        let path = path.ok_or_else(|| named(format!("no path in `{command_line}`")))?;
        let content = std::fs::read(&path)
            .map_err(|failure| named(format!("read {path} failed: {failure}")))?;
        let mask = match &family {
            None => sprefa_extract::FamilyMask::ALL,
            Some(spec) => {
                let mut mask = sprefa_extract::FamilyMask::NONE;
                for name in spec.split(',') {
                    match name.trim() {
                        "cst" => mask.cst = true,
                        "type" | "types" => mask.types = true,
                        "call" => mask.call = true,
                        "df" => mask.df = true,
                        _ => {}
                    }
                }
                mask
            }
        };
        let mut lines: Vec<String> = Vec::new();
        if want_file_fact {
            let fact = sprefa_extract::file_fact(&path, &content);
            lines.push(serde_json::to_string(&fact).expect("file fact serializes"));
        }
        if let Some(out) = sprefa_extract::dispatch(&path, &content, mask) {
            for fact in sprefa_extract::flatten(&out) {
                lines.push(serde_json::to_string(&fact).expect("flat fact serializes"));
            }
        }
        Ok(lines.join("\n"))
    }
}

// Minimal POSIX-style word split for a filled template: single quotes are
// literal, double quotes keep `\` escapes, bare backslash escapes one char.
fn shell_tokens(command_line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = command_line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return Err("unterminated single quote".to_string()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped) => current.push(escaped),
                            None => return Err("dangling escape".to_string()),
                        },
                        Some(inner) => current.push(inner),
                        None => return Err("unterminated double quote".to_string()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match chars.next() {
                    Some(escaped) => current.push(escaped),
                    None => return Err("dangling escape".to_string()),
                }
            }
            character if character.is_whitespace() => {
                if in_word {
                    tokens.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            character => {
                in_word = true;
                current.push(character);
            }
        }
    }
    if in_word {
        tokens.push(current);
    }
    Ok(tokens)
}

// ═══ template fill (the quote-context walk from serve/1_hosts.ts) ═══════════

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteContext {
    Bare,
    Single,
    Double,
}

fn escape_for_shell(value: &str, context: QuoteContext) -> String {
    match context {
        QuoteContext::Single => value.replace('\'', "'\\''"),
        QuoteContext::Double => {
            let mut escaped = String::with_capacity(value.len());
            for character in value.chars() {
                if matches!(character, '\\' | '$' | '`' | '"') {
                    escaped.push('\\');
                }
                escaped.push(character);
            }
            escaped
        }
        QuoteContext::Bare => format!("'{}'", value.replace('\'', "'\\''")),
    }
}

pub fn shell_text(value: &ScalarValue) -> String {
    match value {
        ScalarValue::Text(text) => text.clone(),
        ScalarValue::Integer(number) => number.to_string(),
        ScalarValue::Real(number) => crate::ticklog::js_float_text(*number),
        ScalarValue::Bool(flag) => if *flag { "true" } else { "false" }.to_string(),
    }
}

pub fn fill_template(template: &str, inputs: &BTreeMap<String, ScalarValue>) -> String {
    let mut context = QuoteContext::Bare;
    let mut filled = String::new();
    let characters: Vec<char> = template.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '\\' && context != QuoteContext::Single {
            filled.push(character);
            if index + 1 < characters.len() {
                filled.push(characters[index + 1]);
            }
            index += 2;
            continue;
        }
        if character == '\'' && context != QuoteContext::Double {
            context = if context == QuoteContext::Single {
                QuoteContext::Bare
            } else {
                QuoteContext::Single
            };
            filled.push(character);
            index += 1;
            continue;
        }
        if character == '"' && context != QuoteContext::Single {
            context = if context == QuoteContext::Double {
                QuoteContext::Bare
            } else {
                QuoteContext::Double
            };
            filled.push(character);
            index += 1;
            continue;
        }
        if character == '{' {
            if let Some(close) = characters[index..].iter().position(|&c| c == '}') {
                let name: String = characters[index + 1..index + close].iter().collect();
                if let Some(value) = inputs.get(&name) {
                    filled.push_str(&escape_for_shell(&shell_text(value), context));
                    index += close + 1;
                    continue;
                }
            }
        }
        filled.push(character);
        index += 1;
    }
    filled
}

fn env_for_inputs(inputs: &BTreeMap<String, ScalarValue>) -> BTreeMap<String, String> {
    inputs
        .iter()
        .map(|(name, value)| (name.clone(), shell_text(value)))
        .collect()
}

// ═══ output decode (three shapes, same precedence as serve/1_hosts.ts) ══════

fn coerce(
    host: &str,
    column: &HostColumnPlan,
    raw: &serde_json::Value,
) -> Result<Value, HostError> {
    let named = |message: String| HostError {
        host: host.to_string(),
        message,
    };
    match column.column_type.as_str() {
        "bool" => match raw {
            serde_json::Value::Bool(flag) => Ok(Value::Bool(*flag)),
            serde_json::Value::String(text) if text == "true" => Ok(Value::Bool(true)),
            serde_json::Value::String(text) if text == "false" => Ok(Value::Bool(false)),
            other => Err(named(format!(
                "produced a non-boolean value for bool column '{}': {other}",
                column.name
            ))),
        },
        "float" => {
            let value = match raw {
                serde_json::Value::Number(number) => number.as_f64(),
                other => text_of(other).trim().parse::<f64>().ok(),
            };
            match value {
                Some(number) if number.is_finite() => {
                    Ok(Value::Real(if number == 0.0 { 0.0 } else { number }))
                }
                _ => Err(named(format!(
                    "produced a non-finite value for float column '{}': {raw}",
                    column.name
                ))),
            }
        }
        "int" => {
            let value = match raw {
                serde_json::Value::Number(number) => number.as_i64(),
                other => text_of(other).trim().parse::<i64>().ok(),
            };
            match value {
                Some(number) => Ok(Value::Integer(number)),
                None => Err(named(format!(
                    "produced a non-integer value for int column '{}': {raw}",
                    column.name
                ))),
            }
        }
        // Every other declared type crosses the arrival seam as text; object
        // stdout becomes JSON text for the struct/json planes to intern.
        _ => Ok(Value::Text(text_of(raw))),
    }
}

fn text_of(raw: &serde_json::Value) -> String {
    match raw {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        other => other.to_string(),
    }
}

fn parse_json_items(text: &str) -> Option<Vec<serde_json::Value>> {
    if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(text) {
        return Some(items);
    }
    let lines: Vec<&str> = text
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut items = Vec::with_capacity(lines.len());
    for line in lines {
        match serde_json::from_str(line) {
            Ok(item) => items.push(item),
            Err(_) => return None,
        }
    }
    Some(items)
}

fn carries_every_column(
    item: &serde_json::Map<String, serde_json::Value>,
    outputs: &[HostColumnPlan],
) -> bool {
    outputs
        .iter()
        .all(|column| matches!(item.get(&column.name), Some(value) if !value.is_null()))
}

fn decode_object_items(
    host: &str,
    items: &[serde_json::Value],
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    let objects: Vec<&serde_json::Map<String, serde_json::Value>> =
        items.iter().filter_map(|item| item.as_object()).collect();
    if objects.len() != items.len() {
        // Mixed or non-object JSON keeps the positional reading.
        return items
            .iter()
            .map(|item| {
                outputs
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        let raw = match item {
                            serde_json::Value::Array(fields) => fields
                                .get(index)
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                            other => other.clone(),
                        };
                        coerce(host, column, &raw)
                    })
                    .collect()
            })
            .collect();
    }
    objects
        .iter()
        .filter(|item| carries_every_column(item, outputs))
        .map(|item| {
            outputs
                .iter()
                .map(|column| {
                    coerce(
                        host,
                        column,
                        item.get(&column.name).unwrap_or(&serde_json::Value::Null),
                    )
                })
                .collect()
        })
        .collect()
}

// GRID before PER-COLUMN, and grid only when every line splits into exactly
// the declared column count (bug host_grid_answer_folded).
fn parse_whitespace(
    host: &str,
    text: &str,
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let fields_per_line: Vec<Vec<&str>> = lines
        .iter()
        .map(|line| line.split_whitespace().collect())
        .collect();
    let is_grid = fields_per_line
        .iter()
        .all(|fields| fields.len() == outputs.len());
    let text_value =
        |field: Option<&str>| serde_json::Value::String(field.unwrap_or("").to_string());
    if !is_grid && outputs.len() > 1 && lines.len() == outputs.len() {
        let row: Result<Vec<Value>, HostError> = outputs
            .iter()
            .enumerate()
            .map(|(index, column)| coerce(host, column, &text_value(lines.get(index).copied())))
            .collect();
        return Ok(vec![row?]);
    }
    fields_per_line
        .iter()
        .map(|fields| {
            outputs
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    coerce(host, column, &text_value(fields.get(index).copied()))
                })
                .collect()
        })
        .collect()
}

pub fn decode_output(
    host: &str,
    stdout: &str,
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || outputs.is_empty() {
        return Ok(Vec::new());
    }
    match parse_json_items(trimmed) {
        Some(items) => decode_object_items(host, &items, outputs),
        None => parse_whitespace(host, trimmed, outputs),
    }
}

// ═══ the live runner ═════════════════════════════════════════════════════════

struct HostDemand<'p> {
    plan: &'p HostPlanData,
    witness_digest: String,
    inputs: BTreeMap<String, ScalarValue>,
}

pub struct HostLiveRunner<'p> {
    plans: Vec<&'p HostPlanData>,
    rel_columns: &'p HashMap<String, Vec<String>>,
    claimed: HashSet<String>,
}

impl<'p> HostLiveRunner<'p> {
    // An executor this runtime does not know is named at construction rather
    // than skipped in silence (the serve/1_hosts.ts refused-plan contract).
    pub fn new(
        plans: &'p [HostPlanData],
        rel_columns: &'p HashMap<String, Vec<String>>,
    ) -> Result<Self, HostError> {
        for plan in plans {
            if executor_for_plan(plan).is_none() {
                return Err(HostError {
                    host: plan.name.clone(),
                    message: format!("unknown host executor '{}'", plan.execution),
                });
            }
        }
        Ok(HostLiveRunner {
            plans: plans.iter().collect(),
            rel_columns,
            claimed: HashSet::new(),
        })
    }

    pub fn has_plans(&self) -> bool {
        !self.plans.is_empty()
    }

    // The one place a demand row's values become template arguments, so the
    // one place a list column wired into a host input is named.
    fn demand_of(
        &self,
        plan: &'p HostPlanData,
        row: &[Value],
    ) -> Result<HostDemand<'p>, HostError> {
        let columns = self
            .rel_columns
            .get(&plan.demand_rel)
            .map(|columns| columns.as_slice())
            .unwrap_or(&[]);
        let named = |error: BoundaryError| HostError {
            host: plan.name.clone(),
            message: error.to_string(),
        };
        let mut inputs = BTreeMap::new();
        for input in &plan.inputs {
            let value = columns
                .iter()
                .position(|column| *column == input.name)
                .and_then(|index| row.get(index).cloned())
                .unwrap_or(Value::Text(String::new()));
            let scalar =
                ScalarValue::at_seam(&value, ScalarSeam::HostTemplateArgument).map_err(&named)?;
            inputs.insert(input.name.clone(), scalar);
        }
        let witness = columns
            .iter()
            .position(|column| column == "witness_digest")
            .and_then(|index| row.get(index).cloned());
        let witness_digest = match witness {
            None => String::new(),
            Some(value) => shell_text(
                &ScalarValue::at_seam(&value, ScalarSeam::HostTemplateArgument).map_err(&named)?,
            ),
        };
        Ok(HostDemand {
            plan,
            witness_digest,
            inputs,
        })
    }

    fn claim_once(&mut self, plan_name: &str, witness_digest: &str) -> bool {
        self.claimed.insert(format!("{plan_name}|{witness_digest}"))
    }

    fn project(
        demand: &HostDemand<'p>,
        stdout: &str,
        rel_columns: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<Arrival>, HostError> {
        let output_rows = decode_output(&demand.plan.name, stdout, &demand.plan.outputs)?;
        let response_columns = rel_columns
            .get(&demand.plan.response_rel)
            .map(|columns| columns.as_slice())
            .unwrap_or(&[]);
        Ok(output_rows
            .into_iter()
            .enumerate()
            .map(|(ordinal, output_row)| Arrival {
                rel: demand.plan.response_rel.clone(),
                sign: ArrivalSign::Add,
                row: response_columns
                    .iter()
                    .map(|column| {
                        if column == "witness_digest" {
                            return Value::Text(demand.witness_digest.clone());
                        }
                        if column == "ordinal" {
                            return Value::Integer(ordinal as i64);
                        }
                        if let Some(input) = demand.inputs.get(column) {
                            return Value::from(input.clone());
                        }
                        demand
                            .plan
                            .outputs
                            .iter()
                            .position(|output| output.name == *column)
                            .and_then(|index| output_row.get(index).cloned())
                            .unwrap_or(Value::Text(String::new()))
                    })
                    .collect(),
            })
            .collect())
    }

    // One tick's +deltas on every demand rel, claimed, folded per the
    // applicative rule, executed, projected into response-rel arrivals.
    pub fn collect(&mut self, deltas: &TickDeltas) -> Result<Vec<Arrival>, HostError> {
        let mut demands: Vec<HostDemand<'p>> = Vec::new();
        for delta in &deltas.rels {
            let Some(plan) = self
                .plans
                .iter()
                .find(|plan| plan.demand_rel == delta.rel)
                .copied()
            else {
                continue;
            };
            for row in &delta.add {
                demands.push(self.demand_of(plan, row)?);
            }
        }
        let claimed: Vec<HostDemand<'p>> = demands
            .into_iter()
            .filter(|demand| {
                let name = demand.plan.name.clone();
                let witness = demand.witness_digest.clone();
                self.claim_once(&name, &witness)
            })
            .collect();
        let mut arrivals = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<Vec<&HostDemand<'p>>> = Vec::new();
        for demand in &claimed {
            if !is_applicative(&demand.plan.execution) {
                groups.push(vec![demand]);
                continue;
            }
            let ordered_inputs: Vec<(String, String)> = demand
                .plan
                .inputs
                .iter()
                .map(|input| {
                    (
                        input.name.clone(),
                        demand
                            .inputs
                            .get(&input.name)
                            .map(shell_text)
                            .unwrap_or_default(),
                    )
                })
                .collect();
            let key = format!(
                "{}|{}|{:?}",
                demand.plan.execution, demand.plan.template, ordered_inputs
            );
            match group_index.get(&key) {
                Some(&index) => groups[index].push(demand),
                None => {
                    group_index.insert(key, groups.len());
                    groups.push(vec![demand]);
                }
            }
        }
        for group in groups {
            let first = group[0];
            let executor = executor_for_plan(first.plan).expect("validated at construction");
            let command_line = fill_template(&first.plan.template, &first.inputs);
            let env = env_for_inputs(&first.inputs);
            let stdout = executor.run(&first.plan.name, &command_line, &env)?;
            for demand in group {
                arrivals.extend(Self::project(demand, &stdout, self.rel_columns)?);
            }
        }
        Ok(arrivals)
    }
}
