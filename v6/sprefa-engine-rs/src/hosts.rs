// Live host execution for the batch runtime: the Rust port of the tsv2
// HostRunner contract (serve/1_hosts.ts), sharing its emitted HostPlanData.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};

use sha2::{Digest, Sha256};

use crate::types::{
    Arrival, ArrivalSign, BoundaryError, HostAdapterRow, HostColumnPlan, HostPlanData, HostRow,
    ScalarSeam, ScalarValue, TickDeltas, Value,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub host: String,
    pub message: String,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "host '{}': {}", self.host, self.message)
    }
}

/// `Once` answers a demand and is done; `Continuing` re-answers on its own
/// clock or its own filesystem watcher, and each re-answer is a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorCadence {
    Once,
    Continuing,
}

/// A host answer is a row set, never a byte stream: the executor names its own
/// columns and the runner projects them, so nothing between the two parses text.
pub trait IHostExecutor: Sync {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError>;

    fn cadence(&self) -> ExecutorCadence {
        ExecutorCadence::Once
    }
}

/// The SAME slash paths as registry.pl's arrival_executor/2 rows (ruling
/// executor_namespacing), pinned equal by executor_roster_matches_registry.
pub const LINKED_EXECUTORS: &str = "/soopy/files, /soopy/files_at, /soopy/stage, \
     /soopy/commit, /soopy/refs, /soopy/history, /soopy/repo_at, /soopy/checkout, \
     /soopy/mirror_pr_heads, /soopy/watch, /clock/tick, \
     /extract/records, \
     /extract/repo_records, /extract/call_node, /extract/call_node_at, \
     /extract/call_ref, /extract/cfg_at, /extract/specifier_at, \
     /extract/type_node_at, /extract/sig_at, /extract/df_node_at, \
     /extract/df_edge_at, /extract/df_param_at, /extract/df_arg_at, \
     /extract/data_doc_at, /extract/comment_fact, /extract/ast_rule, /scip/call, \
     /scip/type, /scip/diet/call, /scip/diet/type, /cargo/targets, /http/get, \
     /http/post, /env/var, /toml/json, \
     /dl/tick_cost";

static GIT_REFS: LazyLock<crate::executors::git_refs::GitRefsExecutor> =
    LazyLock::new(crate::executors::git_refs::GitRefsExecutor::new);
static GIT_HISTORY: LazyLock<crate::executors::git_history::GitHistoryExecutor> =
    LazyLock::new(crate::executors::git_history::GitHistoryExecutor::new);
static REPO_AT: LazyLock<crate::executors::repo_at::RepoAtExecutor> =
    LazyLock::new(crate::executors::repo_at::RepoAtExecutor::new);
static CLOCK_TICK: LazyLock<crate::executors::clock::ClockExecutor> =
    LazyLock::new(crate::executors::clock::ClockExecutor::new);
static SOOPY_WATCH: LazyLock<crate::executors::watch::SoopyWatchExecutor> =
    LazyLock::new(crate::executors::watch::SoopyWatchExecutor::new);

pub fn executor_for(execution: &str) -> Option<&'static dyn IHostExecutor> {
    match execution {
        // The /extract/* questions share the in-process extractor; each rel
        // selects its own rows out of the one stream by column presence.
        "/extract/records"
        | "/extract/repo_records"
        | "/extract/call_node"
        | "/extract/call_node_at"
        | "/extract/call_ref"
        | "/extract/cfg_at"
        | "/extract/specifier_at"
        | "/extract/type_node_at"
        | "/extract/sig_at"
        | "/extract/df_node_at"
        | "/extract/df_edge_at"
        | "/extract/df_param_at"
        | "/extract/df_arg_at"
        | "/extract/data_doc_at"
        | "/extract/comment_fact" => Some(&*EXTRACT),
        "/extract/ast_rule" => Some(&AstRuleExecutor),
        // Two names, one executor: files_revision/2 reads which revision the
        // name asks for, so a worktree read and a pinned read cannot collide.
        "/soopy/files" | "/soopy/files_at" => Some(&SoopyFilesExecutor),
        "/soopy/stage" | "/soopy/commit" => Some(&SoopyMutationExecutor),
        // ONE transport, no GitHub knowledge: a conditional GET, a page walk
        // and a GraphQL batch are rules in the program that spelled them.
        "/http/get" => Some(&crate::executors::HttpGetExecutor),
        "/http/post" => Some(&crate::executors::HttpPostExecutor),
        "/env/var" => Some(&crate::executors::EnvExecutor),
        // The engine measuring itself: the trace table and the resident set, as
        // rows the same fold folds.
        "/dl/tick_cost" => Some(&crate::executors::TickCostExecutor),
        "/soopy/checkout" | "/soopy/mirror_pr_heads" => {
            Some(&crate::executors::SoopyCheckoutExecutor)
        }
        "/toml/json" => Some(&crate::executors::TomlJsonExecutor),
        // The cross-repository families, each a fresh soopy call per demand;
        // this runner's own per-tick claim is what keeps a shared pass to one call.
        "/soopy/refs" => Some(&*GIT_REFS),
        "/soopy/history" => Some(&*GIT_HISTORY),
        "/soopy/repo_at" => Some(&*REPO_AT),
        "/clock/tick" => Some(&*CLOCK_TICK),
        "/soopy/watch" => Some(&*SOOPY_WATCH),
        // The two scip namespaces, both in-process: no child spawn for the
        // diet side, one budgeted indexer run for the index side.
        "/scip/call" | "/scip/type" | "/scip/diet/call" | "/scip/diet/type" => Some(&*SCIP),
        "/cargo/targets" => Some(&CargoMetadataExecutor),
        _ => None,
    }
}

/// The applicative fold scope: every /extract/* question folds with its
/// siblings on equal ordered inputs; anything else folds only with itself.
fn fold_scope(execution: &str) -> &str {
    if execution.starts_with("/extract/") && execution != "/extract/ast_rule" {
        "extract"
    } else {
        execution
    }
}

// The compiler still spells every `sh` declaration `execution: "shell"`. That
// name is the sidecar's lookup key now; no executor answers to it.
fn execution_for_plan(plan: &HostPlanData, adapter_rows: &[HostAdapterRow]) -> String {
    if plan.execution != "shell" {
        return plan.execution.clone();
    }
    adapter_rows
        .iter()
        .find(|row| row.demand_rel == plan.demand_rel && row.response_rel == plan.response_rel)
        .map(|row| row.adapter.clone())
        .unwrap_or_else(|| plan.execution.clone())
}

fn executor_for_plan(
    plan: &HostPlanData,
    adapter_rows: &[HostAdapterRow],
) -> Option<&'static dyn IHostExecutor> {
    executor_for(&execution_for_plan(plan, adapter_rows))
}

/// `run.rs`'s resident loop reads this to decide which routed rels are its own
/// continuing sources, and never invokes `IHostExecutor::run` for anything else.
pub fn plan_is_continuing(plan: &HostPlanData, adapter_rows: &[HostAdapterRow]) -> bool {
    executor_for_plan(plan, adapter_rows)
        .map(|executor| executor.cadence() == ExecutorCadence::Continuing)
        .unwrap_or(false)
}

fn is_applicative(execution: &str) -> bool {
    (execution.starts_with("/extract/") && execution != "/extract/ast_rule")
        || execution.starts_with("/scip/")
}

/// Typed ast-grep rule execution. The request arrives as the DL6 `text`
/// transport for the documented `AstRuleRequest` YAML schema; parsing and rule
/// matching happen through the linked extractor library, never through `sg`.
///
/// The digest accepts the two Soopy content identities used by source watch:
/// `git:<oid>` or a bare Git blob OID for Git content, and `blake3:<hex>` for a
/// worktree or plain-filesystem read. The latter path does not require Git
/// discovery. The v5 TypeScript `live_watch` adapter emitted a bare SHA-256
/// digest, which is accepted as a compatibility input and converted to
/// Soopy's BLAKE3 filesystem identity after verification.
pub struct AstRuleExecutor;

impl IHostExecutor for AstRuleExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let path = required_input(host, env, "path")?;
        let digest = required_input(host, env, "digest")?;
        let request_yaml = required_input(host, env, "request")?;
        let request =
            sprefa_extract::decode_ast_rule_yaml(&request_yaml).map_err(|error| HostError {
                host: host.to_string(),
                message: format!("decode ast_rule request: {error}"),
            })?;
        let input = decode_content_id(host, &digest)?;
        let (bytes, content) = match input {
            AstRuleInput::Content(soopy::ContentId::GitBlob(oid)) => {
                let repo_root = env
                    .get("repo")
                    .map(PathBuf::from)
                    .or_else(|| EXTRACT.repository_root(&path))
                    .ok_or_else(|| HostError {
                        host: host.to_string(),
                        message: format!("no repository root for digest read of {path}"),
                    })?;
                (
                    EXTRACT.read_blob(host, &repo_root, oid.0.as_ref(), &path)?,
                    soopy::ContentId::GitBlob(oid),
                )
            }
            AstRuleInput::Content(soopy::ContentId::Blake3(expected)) => {
                let bytes = std::fs::read(&path).map_err(|error| HostError {
                    host: host.to_string(),
                    message: format!("read worktree content {path}: {error}"),
                })?;
                let actual = soopy::ContentId::blake3(&bytes);
                let content = soopy::ContentId::Blake3(expected);
                if actual != content {
                    return Err(HostError {
                        host: host.to_string(),
                        message: format!(
                            "read worktree content {path}: digest is {}, expected {}",
                            actual, content
                        ),
                    });
                }
                (bytes, content)
            }
            AstRuleInput::LegacySha256(expected) => {
                let bytes = std::fs::read(&path).map_err(|error| HostError {
                    host: host.to_string(),
                    message: format!("read legacy live_watch content {path}: {error}"),
                })?;
                let actual = Sha256::digest(&bytes);
                if actual.as_slice() != expected {
                    return Err(HostError {
                        host: host.to_string(),
                        message: format!(
                            "read legacy live_watch content {path}: SHA-256 digest mismatch"
                        ),
                    });
                }
                (bytes.clone(), soopy::ContentId::blake3(&bytes))
            }
        };
        let rows = sprefa_extract::query_ast_rule_with_content(&path, &bytes, &request, content)
            .map_err(|error| HostError {
                host: host.to_string(),
                message: format!("execute ast_rule request: {error}"),
            })?;
        rows.into_iter()
            .map(|row| {
                serde_json::to_value(row)
                    .map_err(|error| HostError {
                        host: host.to_string(),
                        message: format!("serialize ast_rule row: {error}"),
                    })?
                    .as_object()
                    .cloned()
                    .ok_or_else(|| HostError {
                        host: host.to_string(),
                        message: "serialize ast_rule row: expected JSON object".to_string(),
                    })
            })
            .collect()
    }
}

enum AstRuleInput {
    Content(soopy::ContentId),
    LegacySha256([u8; 32]),
}

fn decode_content_id(host: &str, digest: &str) -> Result<AstRuleInput, HostError> {
    let named = |message: String| HostError {
        host: host.to_string(),
        message,
    };
    if let Some(hex) = digest.strip_prefix("blake3:") {
        let bytes = hex.as_bytes();
        if bytes.len() != 64 {
            return Err(named(format!(
                "decode blake3 content identity: expected 64 hex digits, got {}",
                bytes.len()
            )));
        }
        let mut digest = [0u8; 32];
        for (index, pair) in bytes.chunks_exact(2).enumerate() {
            let high = (pair[0] as char).to_digit(16).ok_or_else(|| {
                named(format!(
                    "decode blake3 content identity: invalid hex at {index}"
                ))
            })?;
            let low = (pair[1] as char).to_digit(16).ok_or_else(|| {
                named(format!(
                    "decode blake3 content identity: invalid hex at {}",
                    index * 2 + 1
                ))
            })?;
            digest[index] = ((high << 4) | low) as u8;
        }
        return Ok(AstRuleInput::Content(soopy::ContentId::Blake3(digest)));
    }
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let mut bytes = [0u8; 32];
        for (index, pair) in digest.as_bytes().chunks_exact(2).enumerate() {
            let high = (pair[0] as char).to_digit(16).expect("ASCII hex");
            let low = (pair[1] as char).to_digit(16).expect("ASCII hex");
            bytes[index] = ((high << 4) | low) as u8;
        }
        return Ok(AstRuleInput::LegacySha256(bytes));
    }
    let oid = digest.strip_prefix("git:").unwrap_or(digest);
    if oid.is_empty() {
        return Err(named(
            "decode Git blob content identity: empty object id".to_string(),
        ));
    }
    Ok(AstRuleInput::Content(soopy::ContentId::GitBlob(
        soopy::ObjectId(Arc::from(oid)),
    )))
}

// @comment-ok: the single-repository file surface's contract, its one doc site.
//
// TWO NAMES AND THE REVISION IS IN THE NAME (ruling `files_naming =
// files_unmarked_worktree_marked_rev`, rulings.pl:544). No column carries
// "which revision" because this language has no nulls and spells optionality as
// variants:
//
//   files(glob)          the worktree
//   files_at(rev, glob)  a pinned revision
//
// The repo-scoped twins are their own hosts by the same ruling line
// (`repo_column_spelling = distinct_name_hosts`, rulings.pl:559) and live in
// `executors/repo_at.rs`.
//
// THE WITNESS IS WHY THE PINNED NAMES EXIST. A host witness is
// content-addressed over its input columns, so `files` caches for the life of
// the db and worktree freshness is the watcher's job; a `_at` answer is keyed by
// a rev whose tree never changes, so it caches forever and correctly.
//
// EVERY NAME ANSWERS A BLOB OID because all four ride the `git_files` door:
// worktree bytes go through one `git hash-object --stdin-paths`, a commit
// through one `git cat-file --batch-check`. A worktree oid names bytes
// `hash-object` never wrote to the object database, which is why `read_blob`
// falls back to the file and re-hashes. The FILESYSTEM walk is a different door
// (`soopy::SourceTree::enumerate`), it answers `ContentId::Blake3`, and nothing
// here reaches it.
pub struct SoopyFilesExecutor;

/// The revision each name walks. The name is the marker, so a name off this
/// roster is a stop and never a worktree fallback.
fn files_revision(
    host: &str,
    env: &BTreeMap<String, String>,
) -> Result<soopy::Revision, HostError> {
    // `host` is the plan name, so it is module_path_name/2's `__` join of the
    // declaration's path: `/soopy/files` arrives here as `soopy__files`.
    match host {
        "soopy__files" => Ok(soopy::Revision::Worktree),
        "soopy__files_at" => Ok(crate::change_facts::parse_revision(&required_input(
            host, env, "rev",
        )?)),
        other => Err(HostError {
            host: host.to_string(),
            message: format!(
                "`{other}` is not a file-surface host; the roster is /soopy/files and \
                 /soopy/files_at, and the repo-scoped twins are /soopy/repo_at's"
            ),
        }),
    }
}

impl IHostExecutor for SoopyFilesExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let revision = files_revision(host, env)?;
        let glob = required_input(host, env, "glob")?;
        let span = tracing::info_span!("soopy_files", host, revision = ?revision);
        let _entered = span.enter();
        let root = env
            .get("repo")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let repository = soopy::discover(root.clone())
            .map_err(|error| named(format!("open a repository at {}: {error}", root.display())))?;
        let query = soopy::GitFilesQuery {
            revision: revision.clone(),
            pathspecs: vec![glob.clone()],
        };
        let entries = soopy::enumerate(&repository, &query)
            .map_err(|error| named(format!("enumerate `{glob}` at {revision:?}: {error}")))?;
        let mut rows = Vec::with_capacity(entries.len());
        for entry in entries {
            let digest = match &entry.content {
                soopy::ContentId::GitBlob(oid) => oid.0.to_string(),
                // Only the filesystem walk mints a Blake3 address, and a Blake3
                // here would have no address `read_blob` could resolve.
                other => {
                    return Err(named(format!(
                        "tracked file {} at {revision:?} carries {other:?}, not a git blob",
                        entry.source.path.0
                    )))
                }
            };
            rows.push(host_row([
                ("path", serde_json::json!(entry.source.path.0.as_ref())),
                ("digest", serde_json::json!(digest)),
            ]));
        }
        Ok(rows)
    }
}

/// `MetadataCommand` spawns the `cargo` binary itself, so nothing between this
/// engine and the workspace document is a shell.
pub struct CargoMetadataExecutor;

impl IHostExecutor for CargoMetadataExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let manifest_dir = required_input(host, env, "manifest_dir")?;
        let span = tracing::info_span!("cargo_metadata", host, dir = %manifest_dir);
        let _entered = span.enter();
        let metadata = cargo_metadata::MetadataCommand::new()
            .current_dir(&manifest_dir)
            .no_deps()
            .exec()
            .map_err(|error| named(format!("read the workspace at {manifest_dir}: {error}")))?;
        let packages = serde_json::to_value(&metadata.packages)
            .map_err(|error| named(format!("serialize packages: {error}")))?;
        Ok(vec![host_row([
            (
                "workspace_root",
                serde_json::json!(metadata.workspace_root.as_str()),
            ),
            ("packages", packages),
        ])])
    }
}

/// Linked executor for the two authored source-mutation hosts.
///
/// `source_stage` receives one complete canonical `StageRequest` document and
/// persists its preview under `state/stages`. `source_commit` later receives
/// the exact stage id that an approval relation joined, reloads the sealed
/// transaction, and applies it through Soopy's durable commit engine. The
/// ordinary host response rows carry both successes and typed refusals, so a
/// refusal remains observable data instead of becoming a runner failure.
pub struct SoopyMutationExecutor;

impl IHostExecutor for SoopyMutationExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        match host {
            "soopy__stage" => source_stage_response(host, env),
            "soopy__commit" => source_commit_response(host, env),
            _ => Err(HostError {
                host: host.to_string(),
                message: "not a Soopy source-mutation host".to_string(),
            }),
        }
    }
}

/// One answered row from named column/value pairs.
pub(crate) fn host_row<const N: usize>(columns: [(&str, serde_json::Value); N]) -> HostRow {
    columns
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

pub(crate) fn required_input(
    host: &str,
    env: &BTreeMap<String, String>,
    name: &str,
) -> Result<String, HostError> {
    env.get(name).cloned().ok_or_else(|| HostError {
        host: host.to_string(),
        message: format!("missing required host input `{name}`"),
    })
}

fn mutation_row(
    stage_id: &str,
    outcome: &str,
    detail: String,
    document: serde_json::Value,
) -> Result<Vec<HostRow>, HostError> {
    Ok(vec![host_row([
        ("stage_id", serde_json::json!(stage_id)),
        ("outcome", serde_json::json!(outcome)),
        ("detail", serde_json::json!(detail)),
        ("document", document),
    ])])
}

fn mutation_root(
    target_root: &Path,
    request: &soopy::StageRequest,
) -> Result<soopy::SourceRoot, String> {
    match request.root {
        soopy::SourceRootId::Directory { .. } => soopy::SourceRoot::open_directory(target_root),
        soopy::SourceRootId::GitWorktree { .. } => soopy::SourceRoot::discover_git(target_root),
    }
    .map_err(|error| format!("open mutation root {}: {error}", target_root.display()))
}

fn mutation_state_root(target_root: &Path, state_root: &Path) -> Result<PathBuf, String> {
    let target_root = target_root.canonicalize().map_err(|error| {
        format!(
            "canonicalize mutation root {}: {error}",
            target_root.display()
        )
    })?;
    let state_root = state_root.canonicalize().map_err(|error| {
        format!(
            "canonicalize mutation state {}: {error}",
            state_root.display()
        )
    })?;
    if !state_root.is_dir() {
        return Err(format!(
            "mutation state root is not a directory: {}",
            state_root.display()
        ));
    }
    if state_root.starts_with(&target_root) {
        return Err(format!(
            "mutation state root must be outside the target root: {}",
            state_root.display()
        ));
    }
    Ok(state_root)
}

fn source_stage_response(
    host: &str,
    env: &BTreeMap<String, String>,
) -> Result<Vec<HostRow>, HostError> {
    let target_root = PathBuf::from(required_input(host, env, "root")?);
    let state_root = PathBuf::from(required_input(host, env, "state")?);
    let request_json = required_input(host, env, "request")?;
    let mut request_value: serde_json::Value = match serde_json::from_str(&request_json) {
        Ok(value) => value,
        Err(error) => {
            return mutation_row(
                "",
                "refused",
                format!("decode StageRequest: {error}"),
                serde_json::json!([]),
            )
        }
    };
    // The program emits a Create action's shim body as `text`; the executor
    // rewrites it to the `bytes` a canonical StageRequest carries (UTF-8), so a
    // datalog program never has to build a byte array.
    fill_create_text(&mut request_value);
    let state_root = match mutation_state_root(&target_root, &state_root) {
        Ok(state_root) => state_root,
        Err(detail) => return mutation_row("", "refused", detail, serde_json::json!([])),
    };
    // A dl6 program never spells the derived identity soopy mints from a
    // canonical path: DirectoryId, RepositoryId and WorktreeId are blake3
    // hashes no datalog builtin computes. When the request omits `root`, the
    // executor derives the root from the target path and fills the git
    // identities the actions' sources carry. A request that spells `root`
    // keeps the strict path below.
    let mut root = if request_value.get("root").is_none() {
        match soopy::SourceRoot::discover_git(&target_root) {
            Ok(soopy::SourceRoot::GitWorktree(git)) => {
                let repository = git.repository.identity.clone();
                let worktree = git.repository.worktree.clone();
                request_value["root"] = serde_json::json!({
                    "kind": "git_worktree",
                    "repository": repository,
                    "worktree": worktree,
                });
                fill_git_action_sources(&mut request_value, &repository, &worktree);
                soopy::SourceRoot::GitWorktree(git)
            }
            Ok(soopy::SourceRoot::Directory(_)) => {
                unreachable!("discover_git returns a Git root")
            }
            Err(_git_error) => {
                let directory = match soopy::SourceRoot::open_directory(&target_root) {
                    Ok(soopy::SourceRoot::Directory(directory)) => directory,
                    Ok(soopy::SourceRoot::GitWorktree(_)) => {
                        unreachable!("open_directory stays filesystem-only")
                    }
                    Err(error) => {
                        return mutation_row(
                            "",
                            "refused",
                            format!("open mutation root {}: {error}", target_root.display()),
                            serde_json::json!([]),
                        )
                    }
                };
                request_value["root"] = serde_json::json!({
                    "kind": "directory",
                    "directory": directory.identity,
                });
                soopy::SourceRoot::Directory(directory)
            }
        }
    } else {
        let request: soopy::StageRequest =
            match serde_json::from_value(request_value.clone()) {
                Ok(request) => request,
                Err(error) => {
                    return mutation_row(
                        "",
                        "refused",
                        format!("decode StageRequest: {error}"),
                        serde_json::json!([]),
                    )
                }
            };
        match mutation_root(&target_root, &request) {
            Ok(root) => root,
            Err(detail) => return mutation_row("", "refused", detail, serde_json::json!([])),
        }
    };
    let request: soopy::StageRequest = match serde_json::from_value(request_value) {
        Ok(request) => request,
        Err(error) => {
            return mutation_row(
                "",
                "refused",
                format!("decode StageRequest: {error}"),
                serde_json::json!([]),
            )
        }
    };
    let mut store = match soopy::DurableStageStore::open(state_root.join("stages")) {
        Ok(store) => store,
        Err(error) => {
            return mutation_row(
                "",
                "refused",
                format!("open stage store: {error}"),
                serde_json::json!([]),
            )
        }
    };
    match soopy::stage_mutations(&mut root, &request, &mut store) {
        Ok(stage) => mutation_row(
            &stage.id.to_string(),
            "staged",
            String::new(),
            serde_json::to_value(stage.previews).map_err(|error| HostError {
                host: host.to_string(),
                message: format!("serialize stage previews: {error}"),
            })?,
        ),
        Err(refusal) => mutation_row("", "refused", refusal.to_string(), serde_json::json!([])),
    }
}

/// Fill the repository and worktree identities of every Git-kind action source
/// from the derived root. The program spells only the path; the executor mints
/// the blake3 identities so a datalog program never has to.
fn fill_git_action_sources(
    request: &mut serde_json::Value,
    repository: &soopy::RepositoryId,
    worktree: &soopy::WorktreeId,
) {
    let Some(actions) = request.get_mut("actions").and_then(|a| a.as_array_mut()) else {
        return;
    };
    for action in actions {
        let Some(source) = action.get_mut("source") else {
            continue;
        };
        if source.get("kind").and_then(|kind| kind.as_str()) != Some("git") {
            continue;
        }
        let Some(source_ref) = source.get_mut("source") else {
            continue;
        };
        source_ref["repository"] = serde_json::json!(repository);
        source_ref["revision"] = serde_json::json!({
            "kind": "worktree",
            "worktree": worktree,
            "head": serde_json::Value::Null,
            "dirty": false,
        });
    }
}

/// Rewrite every Create action's `text` body to the `bytes` a canonical
/// StageRequest carries. The program emits the shim body as UTF-8 text; this
/// is the one boundary where a byte array is spelled.
fn fill_create_text(request: &mut serde_json::Value) {
    let Some(actions) = request.get_mut("actions").and_then(|a| a.as_array_mut()) else {
        return;
    };
    for action in actions {
        let Some(text) = action.get_mut("text") else {
            continue;
        };
        let Some(text) = text.as_str() else {
            continue;
        };
        action["bytes"] = serde_json::Value::Array(
            text.as_bytes().iter().map(|byte| serde_json::json!(byte)).collect(),
        );
        if let Some(object) = action.as_object_mut() {
            object.remove("text");
        }
    }
}

fn source_commit_response(
    host: &str,
    env: &BTreeMap<String, String>,
) -> Result<Vec<HostRow>, HostError> {
    let target_root = PathBuf::from(required_input(host, env, "root")?);
    let state_root = PathBuf::from(required_input(host, env, "state")?);
    let stage_id_text = required_input(host, env, "stage_id")?;
    let stage_id = match soopy::StageId::from_str(&stage_id_text) {
        Ok(id) => id,
        Err(error) => {
            return mutation_row(
                &stage_id_text,
                "refused",
                format!("decode StageId: {error}"),
                serde_json::json!({}),
            )
        }
    };
    let state_root = match mutation_state_root(&target_root, &state_root) {
        Ok(state_root) => state_root,
        Err(detail) => {
            return mutation_row(&stage_id_text, "refused", detail, serde_json::json!({}))
        }
    };
    let store = match soopy::DurableStageStore::open(state_root.join("stages")) {
        Ok(store) => store,
        Err(error) => {
            return mutation_row(
                &stage_id_text,
                "refused",
                format!("open stage store: {error}"),
                serde_json::json!({}),
            )
        }
    };
    let stage = match soopy::show_stage(&store, stage_id) {
        Ok(Some(stage)) => stage,
        Ok(None) => {
            return mutation_row(
                &stage_id_text,
                "refused",
                "stage is not present in this state root".to_string(),
                serde_json::json!({}),
            )
        }
        Err(error) => {
            return mutation_row(
                &stage_id_text,
                "refused",
                format!("load stage: {error}"),
                serde_json::json!({}),
            )
        }
    };
    let engine = match soopy::CommitEngine::open(&target_root, state_root.join("commits")) {
        Ok(engine) => engine,
        Err(error) => {
            return mutation_row(
                &stage_id_text,
                "refused",
                format!("open commit engine: {error}"),
                serde_json::json!({}),
            )
        }
    };
    match engine.commit(&stage) {
        Ok(receipt) => mutation_row(
            &stage_id_text,
            "committed",
            String::new(),
            serde_json::to_value(receipt).map_err(|error| HostError {
                host: host.to_string(),
                message: format!("serialize commit receipt: {error}"),
            })?,
        ),
        Err(refusal) => mutation_row(
            &stage_id_text,
            "refused",
            refusal.to_string(),
            serde_json::json!({}),
        ),
    }
}

// One long-lived `git cat-file --batch` per repository root, so a digest-
// carrying demand is one blob read and never one process per blob.
// Deduplicating extractions belongs to the runner's applicative grouping, not
// here: demands sharing one command over one file already collapse to a single
// `run`, so a response memo at this layer measured 5.48s against 5.33s without
// it, never hit once, and copied every response into an Arc for nothing.
#[derive(Default)]
pub struct SprefaExtractExecutor {
    batches: Mutex<BTreeMap<String, soopy::GitBatch>>,
    // Repository roots by containing directory. `soopy::discover` walks the
    // filesystem upward for `.git` and measured 28ms, so asking it once per
    // FILE re-derives the same root 82 times for one crate and cost 2.29s of a
    // 3.55s run. Keyed on the directory rather than the file so siblings share
    // one answer, and only ever grown, since a checkout root does not move
    // under a running fold.
    roots: Mutex<HashMap<PathBuf, PathBuf>>,
}

static EXTRACT: LazyLock<SprefaExtractExecutor> = LazyLock::new(SprefaExtractExecutor::default);

// ═══ the scip namespaces ════════════════════════════════════════════════════

/// Which evidence a `scip.*` host answers from.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ScipEvidence {
    /// A real SCIP index, built by the language's own indexer under the budget.
    Index,
    /// The tree-sitter front-ends resolved by name match over the file set.
    Diet,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ScipInterface {
    Call,
    Type,
}

/// The `__` join is what the parser's host path lowers to, so this is
/// the whole namespace roster the registry declares.
fn scip_namespace(host: &str) -> Option<(ScipInterface, ScipEvidence)> {
    match host {
        "scip__call" => Some((ScipInterface::Call, ScipEvidence::Index)),
        "scip__diet__call" => Some((ScipInterface::Call, ScipEvidence::Diet)),
        "scip__type" => Some((ScipInterface::Type, ScipEvidence::Index)),
        "scip__diet__type" => Some((ScipInterface::Type, ScipEvidence::Diet)),
        _ => None,
    }
}

/// One resolve over one file set, indexed by the file that owns each row.
#[derive(Default)]
struct ScipFold {
    call: HashMap<String, Vec<HostRow>>,
    types: HashMap<String, Vec<HostRow>>,
    /// Named skips ride every file's answer: a root with no installed indexer
    /// says so on the wire and exits 0, it never stops the fold.
    skips: Vec<HostRow>,
}

/// `/scip/<x>` and `/scip/diet/<x>`, both answered in this process.
///
/// A DEMAND ARRIVES PER FILE AND AN INDEX IS PER REPOSITORY, so the runner's
/// applicative grouping cannot collapse these into one call: its key is the
/// whole ordered input list and `path` is in it, so every file is its own
/// group. `prime` is the smallest change that closes the gap. `collect` sees a
/// tick's whole claimed demand list before it runs any group, so it hands the
/// set here first and each file's answer then comes out of one fold.
#[derive(Default)]
pub struct ScipNamespaceExecutor {
    /// Per repository root, the (path, digest) pairs demanded in this process.
    /// Unioned across ticks: a set that grows is a new set and a new fold.
    sets: Mutex<HashMap<PathBuf, std::collections::BTreeSet<(String, String)>>>,
    /// One fold per (root, set digest, evidence).
    folds: Mutex<HashMap<String, Arc<ScipFold>>>,
}

static SCIP: LazyLock<ScipNamespaceExecutor> = LazyLock::new(ScipNamespaceExecutor::default);

impl ScipNamespaceExecutor {
    /// The whole tick's file set, before any group runs.
    fn prime(&self, entries: &[(PathBuf, String, String)]) {
        let mut sets = self.sets.lock().expect("scip demand set");
        for (root, path, digest) in entries {
            sets.entry(root.clone())
                .or_default()
                .insert((path.clone(), digest.clone()));
        }
    }

    fn set_for(&self, root: &Path, path: &str, digest: &str) -> sprefa_extract::IndexSet {
        let sets = self.sets.lock().expect("scip demand set");
        match sets.get(root) {
            Some(entries) if !entries.is_empty() => {
                sprefa_extract::IndexSet::new(entries.iter().cloned())
            }
            // An unprimed demand is its own set: one file asked on its own is a
            // one-file corpus, never a silent whole-repository read.
            _ => sprefa_extract::IndexSet::new([(path.to_string(), digest.to_string())]),
        }
    }

    fn fold(
        &self,
        host: &str,
        root: &Path,
        set: &sprefa_extract::IndexSet,
        evidence: ScipEvidence,
        interface: ScipInterface,
    ) -> Result<Arc<ScipFold>, HostError> {
        let key = format!(
            "{}|{}|{evidence:?}|{interface:?}",
            root.display(),
            set.digest()
        );
        if let Some(found) = self.folds.lock().expect("scip fold memo").get(&key) {
            return Ok(found.clone());
        }
        let built = Arc::new(build_scip_fold(host, root, set, evidence, interface)?);
        self.folds
            .lock()
            .expect("scip fold memo")
            .insert(key, built.clone());
        Ok(built)
    }
}

/// The one scip resolve request shape in this crate. The mode is always
/// extract-built: this host either runs the name-match leg (no index) or loads
/// the index `ensure_index_for_set` produced.
fn resolve_request<'a>(
    paths: &'a [PathBuf],
    arms: sprefa_extract::ResolveArms,
    root: &'a Path,
    index: Option<&'a Path>,
) -> sprefa_extract::ResolveRequest<'a> {
    sprefa_extract::ResolveRequest {
        paths,
        arms,
        scip: <sprefa_extract::ScipMode>::from_flags(index, false),
        project_root: Some(root),
        scip_records: sprefa_extract::ScipRecords::all(),
        occurrence_text: false,
    }
}

/// The one place a scip fold is computed: an index build plus a resolve, or the
/// name-match resolve alone.
fn build_scip_fold(
    host: &str,
    root: &Path,
    set: &sprefa_extract::IndexSet,
    evidence: ScipEvidence,
    interface: ScipInterface,
) -> Result<ScipFold, HostError> {
    let named = |message: String| HostError {
        host: host.to_string(),
        message,
    };
    let paths: Vec<PathBuf> = set.paths().map(PathBuf::from).collect();
    // ONE arm, the one this namespace answers. Running the other is a second
    // whole-project resolve for rows nobody declared.
    let arms = sprefa_extract::ResolveArms {
        call: interface == ScipInterface::Call,
        types: interface == ScipInterface::Type,
        flow: false,
    };
    let mut fold = ScipFold::default();
    let facts = match evidence {
        ScipEvidence::Diet => {
            let span = tracing::info_span!("scip_diet", files = paths.len());
            let _entered = span.enter();
            sprefa_extract::resolve_project(&resolve_request(&paths, arms, root, None))
                .map_err(|error| named(format!("diet resolve: {error}")))?
        }
        ScipEvidence::Index => {
            let cache = sprefa_extract::default_cache_dir(root);
            let span = tracing::info_span!(
                "scip_index",
                repo = %root.display(),
                files = set.len(),
                fresh = tracing::field::Empty
            );
            let entered = span.enter();
            let report = sprefa_extract::ensure_index_for_set(
                root,
                &cache,
                sprefa_extract::IndexBudget::from_env(),
                Some(set),
            );
            span.record("fresh", report.reused);
            drop(entered);
            for skip in &report.skips {
                fold.skips.push(host_row([
                    ("record", serde_json::json!("scip_skip")),
                    ("lang", serde_json::json!(skip.lang)),
                    ("bin", serde_json::json!(skip.bin)),
                    ("reason", serde_json::json!(skip.reason.slug())),
                    ("detail", serde_json::json!(skip.reason.detail())),
                ]));
            }
            // A root with no installed indexer answers its named skips and
            // exits 0. An empty stream reads as "this project has no calls".
            let Some(index) = report.index else {
                return Ok(fold);
            };
            let span = tracing::info_span!("scip_resolve", files = paths.len());
            let _entered = span.enter();
            sprefa_extract::resolve_project(&resolve_request(&paths, arms, root, Some(&index)))
                .map_err(|error| named(format!("index resolve: {error}")))?
        }
    };
    for fact in &facts {
        let value = serde_json::to_value(fact).map_err(|error| named(error.to_string()))?;
        let Some(object) = value.as_object() else {
            continue;
        };
        match object.get("record").and_then(serde_json::Value::as_str) {
            Some("resolved_edge") => {
                let (Some(caller), Some(callee_path), Some(callee)) = (
                    string_field(object, "caller_path"),
                    string_field(object, "callee_path"),
                    string_field(object, "callee_name"),
                ) else {
                    continue;
                };
                fold.call.entry(caller.clone()).or_default().push(host_row([
                    ("record", serde_json::json!("resolved_edge")),
                    ("family", serde_json::json!("call")),
                    ("caller_path", serde_json::json!(caller)),
                    ("callee_path", serde_json::json!(callee_path)),
                    ("callee", serde_json::json!(callee)),
                    (
                        "kind",
                        serde_json::json!(string_field(object, "kind").unwrap_or_default()),
                    ),
                ]));
            }
            Some("resolved_type_edge") => {
                let (Some(owner), Some(target_path), Some(target)) = (
                    string_field(object, "owner_path"),
                    string_field(object, "target_path"),
                    string_field(object, "target_name"),
                ) else {
                    continue;
                };
                fold.types.entry(owner.clone()).or_default().push(host_row([
                    ("record", serde_json::json!("resolved_type_edge")),
                    ("family", serde_json::json!("type")),
                    ("owner_path", serde_json::json!(owner)),
                    ("target_path", serde_json::json!(target_path)),
                    ("target", serde_json::json!(target)),
                    (
                        "kind",
                        serde_json::json!(string_field(object, "kind").unwrap_or_default()),
                    ),
                ]));
            }
            _ => {}
        }
    }
    Ok(fold)
}

fn string_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    object.get(key)?.as_str().map(str::to_string)
}

impl IHostExecutor for ScipNamespaceExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let Some((interface, evidence)) = scip_namespace(host) else {
            return Err(named(
                "not a scip namespace; the roster is /scip/call, /scip/diet/call, \
                 /scip/type and /scip/diet/type"
                    .to_string(),
            ));
        };
        let path = required_input(host, env, "path")?;
        let digest = env.get("digest").cloned().unwrap_or_default();
        let root = scip_root(host, env)?;
        let set = self.set_for(&root, &path, &digest);
        let fold = self.fold(host, &root, &set, evidence, interface)?;
        let rows = match interface {
            ScipInterface::Call => fold.call.get(&path),
            ScipInterface::Type => fold.types.get(&path),
        };
        let span = tracing::info_span!(
            "scip_answer",
            host,
            path = %path,
            rows = rows.map_or(0, Vec::len)
        );
        let _entered = span.enter();
        let mut answered: Vec<HostRow> = fold
            .skips
            .iter()
            .chain(rows.into_iter().flatten())
            .cloned()
            .collect();
        // One ordinal per row, so the order a file's answer lands in is the
        // program's, not the resolve's.
        answered.sort_by_cached_key(|row| serde_json::to_string(row).unwrap_or_default());
        Ok(answered)
    }
}

/// The repository root the demand's `repo` column names, absolute: a relative
/// spelling resolves against the process cwd, the tree the harness ran in.
fn scip_root(host: &str, env: &BTreeMap<String, String>) -> Result<PathBuf, HostError> {
    let repo = required_input(host, env, "repo")?;
    let candidate = PathBuf::from(&repo);
    std::fs::canonicalize(&candidate).map_err(|failure| HostError {
        host: host.to_string(),
        message: format!("repo `{repo}` is not a readable directory: {failure}"),
    })
}

impl SprefaExtractExecutor {
    /// The checkout root containing `path`, derived once per directory.
    fn repository_root(&self, path: &str) -> Option<PathBuf> {
        let file = PathBuf::from(path);
        let directory = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        if let Some(root) = self
            .roots
            .lock()
            .expect("repository root memo")
            .get(&directory)
        {
            return Some(root.clone());
        }
        let span = tracing::info_span!("discover", dir = %directory.display());
        let _entered = span.enter();
        let root = soopy::discover(file)
            .ok()
            .map(|repository| repository.root)?;
        self.roots
            .lock()
            .expect("repository root memo")
            .insert(directory, root.clone());
        Some(root)
    }

    // One batch process per repository root, never one per blob. An oid the
    // object database has never seen is not a stop on its own: `git hash-object`
    // over a worktree file yields a real content address for content that was
    // never staged, and a rail that hashes what it reads hands us exactly that.
    // The worktree fall-back is only sound because it re-hashes and compares, so
    // the bytes served are always the bytes the digest names.
    fn read_blob(
        &self,
        host: &str,
        repo_root: &Path,
        digest: &str,
        path: &str,
    ) -> Result<Vec<u8>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        let span = tracing::info_span!("read_blob", digest = %digest);
        let _entered = span.enter();
        let key = repo_root.to_string_lossy().into_owned();
        let mut batches = self.batches.lock().expect("extract batch memo");
        if !batches.contains_key(&key) {
            let batch = soopy::GitBatch::open(repo_root)
                .map_err(|error| named(format!("open a blob batch in {key}: {error}")))?;
            batches.insert(key.clone(), batch);
        }
        let batch = batches.get_mut(&key).expect("just inserted the batch");
        let from_odb = batch
            .read(&soopy::ObjectId(Arc::from(digest)))
            .map(|bytes| bytes.to_vec());
        let odb_error = match from_odb {
            Ok(bytes) => return Ok(bytes),
            Err(error) => error,
        };
        drop(batches);
        let bytes = std::fs::read(path).map_err(|failure| {
            named(format!(
                "read blob {digest} in {key}: {odb_error}; worktree {path} unreadable too: {failure}"
            ))
        })?;
        let worktree_digest = soopy::hash_object(
            &soopy::discover(PathBuf::from(path)).map_err(|error| {
                named(format!(
                    "no repository for the worktree read of {path}: {error}"
                ))
            })?,
            &bytes,
        )
        .map_err(|error| named(format!("hash the worktree {path}: {error}")))?;
        if worktree_digest.0.as_ref() != digest {
            return Err(named(format!(
                "read blob {digest} in {key}: {odb_error}; worktree {path} hashes to {} instead",
                worktree_digest.0
            )));
        }
        Ok(bytes)
    }
}

impl IHostExecutor for SprefaExtractExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let named = |message: String| HostError {
            host: host.to_string(),
            message,
        };
        // The dead template's `--family` flag is the `families` INPUT now: a
        // comma list of family words, absent = every family.
        let path = required_input(host, env, "path")?;
        let family = env
            .get("families")
            .map(String::as_str)
            .filter(|families| !families.is_empty())
            .map(str::to_string);
        let digest = env
            .get("digest")
            .map(String::as_str)
            .filter(|d| !d.is_empty());
        let content = match digest {
            Some(digest) => {
                let repo_root = env
                    .get("repo")
                    .map(std::path::PathBuf::from)
                    .or_else(|| self.repository_root(&path))
                    .ok_or_else(|| {
                        named(format!("no repository root for digest read of {path}"))
                    })?;
                self.read_blob(host, &repo_root, digest, &path)?
            }
            // The no-digest branch: a demand that never names a revision reads
            // the worktree bytes off disk, unchanged.
            None => std::fs::read(&path)
                .map_err(|failure| named(format!("read {path} failed: {failure}")))?,
        };
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
                        "data" => mask.data = true,
                        "scip" | "diet_scip" => {
                            return Err(named(format!(
                                "mode `{}` is not linked in-process",
                                name.trim()
                            )));
                        }
                        other => {
                            return Err(named(format!(
                                "family `{other}` is not a known family; in-process families are cst, type, call, df, data"
                            )));
                        }
                    }
                }
                mask
            }
        };
        let span = tracing::info_span!("extract", host, path = %path);
        let _entered = span.enter();
        let mut rows: Vec<HostRow> = Vec::new();
        if let Some(out) = sprefa_extract::dispatch(&path, &content, mask) {
            for mut fact in sprefa_extract::flatten(&out) {
                // A one-argument load (`use_module(lower)`, `include(...)`)
                // names no module; the path IS the module, as source_bind
                // already reads it, so a `module: text` column keeps the row.
                if let sprefa_extract::FlatFact::Specifier { module, name, .. } = &mut fact {
                    if module.is_none() {
                        *module = Some(name.clone());
                    }
                }
                rows.push(fact_row(&named, &fact)?);
            }
        }
        Ok(rows)
    }
}

/// A fact becomes named columns directly; nothing between the extractor and
/// the projection is text.
fn fact_row<F, T>(named: &F, fact: &T) -> Result<HostRow, HostError>
where
    F: Fn(String) -> HostError,
    T: serde::Serialize,
{
    match serde_json::to_value(fact) {
        Ok(serde_json::Value::Object(row)) => Ok(row),
        Ok(other) => Err(named(format!("fact is not a row: {other}"))),
        Err(error) => Err(named(format!("convert a fact to a row: {error}"))),
    }
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
        ScalarValue::Bytes(_) => unreachable!("bytes must be rejected before shell interpolation"),
    }
}

fn reject_binary_host_transport(
    host: &str,
    inputs: &BTreeMap<String, ScalarValue>,
) -> Result<(), HostError> {
    if inputs
        .values()
        .any(|value| matches!(value, ScalarValue::Bytes(_)))
    {
        return Err(HostError {
            host: host.to_string(),
            message: "bytes_host_transport_unsupported".to_string(),
        });
    }
    Ok(())
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
        "bytes" => match raw {
            serde_json::Value::Object(map) if map.len() == 1 => match map.get("$bytes") {
                Some(serde_json::Value::String(encoded)) => crate::types::base64_to_bytes(encoded)
                    .map(Value::Bytes)
                    .map_err(|error| {
                        named(format!(
                            "invalid_bytes_base64 for bytes column '{}': {error}",
                            column.name
                        ))
                    }),
                _ => Err(named(format!(
                    "bytes_host_transport_unsupported for bytes column '{}'",
                    column.name
                ))),
            },
            _ => Err(named(format!(
                "bytes_host_transport_unsupported for bytes column '{}'",
                column.name
            ))),
        },
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

fn carries_every_column(row: &HostRow, outputs: &[HostColumnPlan]) -> bool {
    outputs
        .iter()
        .all(|column| matches!(row.get(&column.name), Some(value) if !value.is_null()))
}

// A declaring host selects its own columns out of a shared answer by column
// presence, so three hosts folded into one run each keep their own rows.
fn select_columns(
    host: &str,
    rows: &[HostRow],
    outputs: &[HostColumnPlan],
) -> Result<Vec<Vec<Value>>, HostError> {
    if outputs.is_empty() {
        return Ok(Vec::new());
    }
    rows.iter()
        .filter(|row| carries_every_column(row, outputs))
        .map(|row| {
            outputs
                .iter()
                .map(|column| {
                    coerce(
                        host,
                        column,
                        row.get(&column.name).unwrap_or(&serde_json::Value::Null),
                    )
                })
                .collect()
        })
        .collect()
}

// ═══ the live runner ═════════════════════════════════════════════════════════

struct HostDemand<'p> {
    plan: &'p HostPlanData,
    witness_digest: String,
    inputs: BTreeMap<String, ScalarValue>,
}

/// The same `'witness|<plan>|<col>:<type>=<value>...'` string the compiled
/// boot SQL hashes into the demand row's own column.
fn witness_digest_for(plan: &HostPlanData, inputs: &BTreeMap<String, ScalarValue>) -> String {
    let mut digest = format!("witness|{}", plan.name);
    for input in &plan.inputs {
        let value = inputs.get(&input.name).map(shell_text).unwrap_or_default();
        digest.push_str(&format!("|{}:{}={value}", input.name, input.column_type));
    }
    digest
}

pub fn project_host_answer(
    plan: &HostPlanData,
    inputs: BTreeMap<String, ScalarValue>,
    answered: &[HostRow],
    rel_columns: &HashMap<String, Vec<String>>,
    sign: ArrivalSign,
) -> Result<Vec<Arrival>, HostError> {
    let witness_digest = witness_digest_for(plan, &inputs);
    let demand = HostDemand {
        plan,
        witness_digest,
        inputs,
    };
    HostLiveRunner::project_signed(&demand, answered, rel_columns, sign)
}

pub struct HostLiveRunner<'p> {
    plans: Vec<&'p HostPlanData>,
    rel_columns: &'p HashMap<String, Vec<String>>,
    adapter_rows: Vec<HostAdapterRow>,
    claimed: HashSet<String>,
}

impl<'p> HostLiveRunner<'p> {
    // An executor this runtime does not know is named at construction rather
    // than skipped in silence (the serve/1_hosts.ts refused-plan contract).
    pub fn new(
        plans: &'p [HostPlanData],
        rel_columns: &'p HashMap<String, Vec<String>>,
    ) -> Result<Self, HostError> {
        Self::with_adapter_rows(plans, rel_columns, &[])
    }

    pub fn with_adapter_rows(
        plans: &'p [HostPlanData],
        rel_columns: &'p HashMap<String, Vec<String>>,
        adapter_rows: &[HostAdapterRow],
    ) -> Result<Self, HostError> {
        for plan in plans {
            let execution = execution_for_plan(plan, adapter_rows);
            if executor_for_plan(plan, adapter_rows).is_none() {
                return Err(HostError {
                    host: plan.name.clone(),
                    message: format!(
                        "no executor links host '{}' (adapter '{execution}'); \
                         linked executors: {LINKED_EXECUTORS}",
                        plan.name
                    ),
                });
            }
        }
        Ok(HostLiveRunner {
            plans: plans.iter().collect(),
            rel_columns,
            adapter_rows: adapter_rows.to_vec(),
            claimed: HashSet::new(),
        })
    }

    pub fn has_plans(&self) -> bool {
        !self.plans.is_empty()
    }

    fn execution_for(&self, plan: &HostPlanData) -> String {
        execution_for_plan(plan, &self.adapter_rows)
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
            // Bytes cross this seam and are named by the binary-transport stop
            // in `collect`; a list has no host spelling at all.
            let scalar = match &value {
                Value::Bytes(bytes) => ScalarValue::Bytes(bytes.clone()),
                other => {
                    ScalarValue::at_seam(other, ScalarSeam::HostTemplateArgument).map_err(named)?
                }
            };
            inputs.insert(input.name.clone(), scalar);
        }
        let witness = columns
            .iter()
            .position(|column| column == "witness_digest")
            .and_then(|index| row.get(index).cloned());
        let witness_digest = match witness {
            None => String::new(),
            Some(value) => match ScalarValue::at_seam(&value, ScalarSeam::HostTemplateArgument) {
                Ok(scalar) => shell_text(&scalar),
                Err(BoundaryError::BytesAtScalarSeam(_)) => {
                    return Err(HostError {
                        host: plan.name.clone(),
                        message: "bytes_host_transport_unsupported".to_string(),
                    })
                }
                Err(error) => return Err(named(error)),
            },
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
        answered: &[HostRow],
        rel_columns: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<Arrival>, HostError> {
        Self::project_signed(demand, answered, rel_columns, ArrivalSign::Add)
    }

    /// The resident loop's own continuing sources re-answer outside any demand
    /// delta (`collect` below only ever emits `Add`), so they need the sign too.
    fn project_signed(
        demand: &HostDemand<'p>,
        answered: &[HostRow],
        rel_columns: &HashMap<String, Vec<String>>,
        sign: ArrivalSign,
    ) -> Result<Vec<Arrival>, HostError> {
        let span = tracing::info_span!("project", host = %demand.plan.name, rows = answered.len());
        let _entered = span.enter();
        let output_rows = select_columns(&demand.plan.name, answered, &demand.plan.outputs)?;
        let response_columns = rel_columns
            .get(&demand.plan.response_rel)
            .map(|columns| columns.as_slice())
            .unwrap_or(&[]);
        Ok(output_rows
            .into_iter()
            .enumerate()
            .map(|(ordinal, output_row)| Arrival {
                rel: demand.plan.response_rel.clone(),
                sign,
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
        // The whole tick's scip file set reaches the executor BEFORE any group
        // runs: an index is per repository and a demand is per file, so the
        // applicative key (which carries `path`) can never collapse them.
        let mut scip_demands: Vec<(PathBuf, String, String)> = Vec::new();
        for demand in &claimed {
            if !self.execution_for(demand.plan).starts_with("/scip/") {
                continue;
            }
            let text = |name: &str| demand.inputs.get(name).map(shell_text).unwrap_or_default();
            let repo = text("repo");
            if let Ok(root) = std::fs::canonicalize(PathBuf::from(&repo)) {
                scip_demands.push((root, text("path"), text("digest")));
            }
        }
        if !scip_demands.is_empty() {
            SCIP.prime(&scip_demands);
        }

        let mut arrivals = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<Vec<&HostDemand<'p>>> = Vec::new();
        for demand in &claimed {
            reject_binary_host_transport(&demand.plan.name, &demand.inputs)?;
            let execution = self.execution_for(demand.plan);
            if !is_applicative(&execution) {
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
            // Fold scope + ordered inputs decide the answer: extract.* fold on
            // one (path, digest, families); scip namespaces never cross-fold.
            let key = format!("{}|{:?}", fold_scope(&execution), ordered_inputs);
            match group_index.get(&key) {
                Some(&index) => groups[index].push(demand),
                None => {
                    group_index.insert(key, groups.len());
                    groups.push(vec![demand]);
                }
            }
        }
        let run_span = tracing::info_span!("run_groups", groups = groups.len());
        let _run_entered = run_span.enter();
        // The transport groups of one tick ride ONE bounded pool: 8 endpoints
        // answered serially took 26s on the first poll bucket (10-second law).
        let mut transported: HashMap<usize, Vec<HostRow>> = HashMap::new();
        let mut transport_index: Vec<usize> = Vec::new();
        let mut transport_requests: Vec<crate::executors::http::Request> = Vec::new();
        for (index, group) in groups.iter().enumerate() {
            let first = group[0];
            let execution = self.execution_for(first.plan);
            let env = env_for_inputs(&first.inputs);
            let Some(built) =
                crate::executors::http::transport_request(&execution, &first.plan.name, &env)
            else {
                continue;
            };
            transport_index.push(index);
            transport_requests.push(built?);
        }
        if !transport_requests.is_empty() {
            for (index, answered) in transport_index
                .iter()
                .zip(crate::executors::http::send_all(&transport_requests))
            {
                // A transport failure is a bucket with no answer, never a dead
                // process; the program's phase re-asks the endpoint later.
                let rows = match answered {
                    Ok(row) => vec![row],
                    Err(failure) => {
                        tracing::warn!(host = %failure.host, error = %failure.message, "transport failed, zero rows this bucket");
                        Vec::new()
                    }
                };
                transported.insert(*index, rows);
            }
        }
        for (index, group) in groups.into_iter().enumerate() {
            let first = group[0];
            let answered = match transported.remove(&index) {
                Some(rows) => rows,
                None => {
                    let executor = executor_for_plan(first.plan, &self.adapter_rows)
                        .expect("validated at construction");
                    let command_line = fill_template(&first.plan.template, &first.inputs);
                    let env = env_for_inputs(&first.inputs);
                    let span = tracing::info_span!("host_run", host = %first.plan.name);
                    let _entered = span.enter();
                    executor.run(&first.plan.name, &command_line, &env)?
                }
            };
            for demand in group {
                arrivals.extend(Self::project(demand, &answered, self.rel_columns)?);
            }
        }
        Ok(arrivals)
    }
}
