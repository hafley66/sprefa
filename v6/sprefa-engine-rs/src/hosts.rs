// Live host execution for the batch runtime: the Rust port of the tsv2
// HostRunner contract (serve/1_hosts.ts), sharing its emitted HostPlanData.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};

use crate::types::{
    Arrival, ArrivalSign, BoundaryError, HostAdapterRow, HostColumnPlan, HostPlanData, ScalarSeam,
    ScalarValue, TickDeltas, Value,
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
        "soopy" => Some(&SoopyMutationExecutor),
        // The linked twin: sprefa-extract runs in this process, no child spawn.
        "sprefa_extract" => Some(&*EXTRACT),
        "soopy_files" => Some(&SoopyFilesExecutor),
        _ => None,
    }
}

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

fn is_applicative(execution: &str) -> bool {
    execution == "sprefa_extract"
}

/// Linked executor for the tracked-file surface: a pathspec in, one
/// `{path, digest}` row per tracked file out.
///
/// Shelling this out costs a `sh -c`, a `mktemp`, a `paste`, and a second
/// `git ls-files` to line the two streams up. Soopy already enumerates a
/// pathspec and batch-hashes the worktree through one
/// `git hash-object --stdin-paths`, which is the same work without the shell
/// and without a temp file to leak.
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
        let span = tracing::info_span!("soopy_files", host);
        let _entered = span.enter();
        let glob = source_mutation_input(host, env, "glob")?;
        let root = env
            .get("repo")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let repository = soopy::discover(root.clone())
            .map_err(|error| named(format!("open a repository at {}: {error}", root.display())))?;
        let query = soopy::GitFilesQuery {
            revision: soopy::Revision::Worktree,
            pathspecs: vec![glob.clone()],
        };
        let entries = soopy::enumerate(&repository, &query)
            .map_err(|error| named(format!("enumerate `{glob}`: {error}")))?;
        let mut lines = Vec::with_capacity(entries.len());
        for entry in entries {
            let digest = match &entry.content {
                soopy::ContentId::GitBlob(oid) => oid.0.to_string(),
                // Only the worktree and commit revisions are reachable here and
                // both yield a blob oid; anything else has no address the
                // extract executor's blob reader could resolve.
                other => {
                    return Err(named(format!(
                        "tracked file {} carries {other:?}, not a git blob",
                        entry.source.path.0
                    )))
                }
            };
            lines.push(
                serde_json::json!({ "path": entry.source.path.0.as_ref(), "digest": digest })
                    .to_string(),
            );
        }
        Ok(lines.join("\n"))
    }
}

// The command shapes a linked executor already answers. Matched on the first
// shell token only, so a pipeline that merely mentions one is not accused.
fn linked_twin_for(command_line: &str) -> Option<&'static str> {
    let first = command_line.split_whitespace().next()?.trim_matches('"');
    if first == "$DL_EXTRACT_BIN" || first.ends_with("/extract") || first == "extract" {
        return Some("sprefa_extract");
    }
    None
}

pub struct ShellExecutor;

impl IHostExecutor for ShellExecutor {
    fn run(
        &self,
        host: &str,
        command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<String, HostError> {
        // Every spawn is visible: a host that silently fell back to `sh` is a
        // 100x cliff that reads as ordinary slowness in a whole-run timing.
        let span = tracing::warn_span!("sh_spawn", host, bytes = command_line.len());
        let _entered = span.enter();
        // Shelling `git` or a one-off is ordinary. Shelling a command a linked
        // executor already answers means an adapter row is missing, which is
        // never intended and costs a process per demand.
        if let Some(twin) = linked_twin_for(command_line) {
            tracing::warn!(
                host,
                twin,
                "host shells a command the linked `{twin}` executor answers in-process; \
                 its adapter row is missing from the program's .adapters.json"
            );
        }
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

/// The shell target is a compatibility adapter for the typed host boundary.
///
/// A legacy `sh` declaration can interpolate only scalar columns.  The
/// compiler still emits its original `HostPlanData` shape, including the
/// original `execution: "shell"` and template bytes.  This adapter is the
/// runtime seam that checks the declared type before converting the row value
/// to the shell's string transport.  Keeping the check beside the conversion
/// prevents a structured value represented by an integer relation reference
/// from being mistaken for an ordinary integer argument.
pub struct ShellHostAdapter;

impl ShellHostAdapter {
    fn input(host: &str, column: &HostColumnPlan, value: &Value) -> Result<ScalarValue, HostError> {
        if column.column_type == "bytes" {
            return Err(HostError {
                host: host.to_string(),
                message: "bytes_host_transport_unsupported".to_string(),
            });
        }
        if !matches!(
            column.column_type.as_str(),
            "text" | "int" | "float" | "bool"
        ) {
            return Err(HostError {
                host: host.to_string(),
                message: format!(
                    "typed_host_transport_unsupported for input column '{}' of type '{}'",
                    column.name, column.column_type
                ),
            });
        }
        ScalarValue::at_seam(value, ScalarSeam::HostTemplateArgument).map_err(|error| HostError {
            host: host.to_string(),
            message: error.to_string(),
        })
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
    ) -> Result<String, HostError> {
        match host {
            "source_stage" => source_stage_response(host, env),
            "source_commit" => source_commit_response(host, env),
            _ => Err(HostError {
                host: host.to_string(),
                message: "not a Soopy source-mutation host".to_string(),
            }),
        }
    }
}

fn source_mutation_input(
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
) -> Result<String, HostError> {
    serde_json::to_string(&serde_json::json!({
        "stage_id": stage_id,
        "outcome": outcome,
        "detail": detail,
        "document": document,
    }))
    .map_err(|error| HostError {
        host: "soopy_mutation".to_string(),
        message: format!("serialize source-mutation response: {error}"),
    })
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

fn source_stage_response(host: &str, env: &BTreeMap<String, String>) -> Result<String, HostError> {
    let target_root = PathBuf::from(source_mutation_input(host, env, "root")?);
    let state_root = PathBuf::from(source_mutation_input(host, env, "state")?);
    let request_json = source_mutation_input(host, env, "request")?;
    let request: soopy::StageRequest = match serde_json::from_str(&request_json) {
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
    let state_root = match mutation_state_root(&target_root, &state_root) {
        Ok(state_root) => state_root,
        Err(detail) => return mutation_row("", "refused", detail, serde_json::json!([])),
    };
    let mut root = match mutation_root(&target_root, &request) {
        Ok(root) => root,
        Err(detail) => return mutation_row("", "refused", detail, serde_json::json!([])),
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

fn source_commit_response(host: &str, env: &BTreeMap<String, String>) -> Result<String, HostError> {
    let target_root = PathBuf::from(source_mutation_input(host, env, "root")?);
    let state_root = PathBuf::from(source_mutation_input(host, env, "state")?);
    let stage_id_text = source_mutation_input(host, env, "stage_id")?;
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
}

static EXTRACT: LazyLock<SprefaExtractExecutor> = LazyLock::new(SprefaExtractExecutor::default);

impl SprefaExtractExecutor {
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
        let worktree_digest = soopy::hash_object(&soopy::discover(PathBuf::from(path)).map_err(
            |error| named(format!("no repository for the worktree read of {path}: {error}")),
        )?, &bytes)
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
        command_line: &str,
        env: &BTreeMap<String, String>,
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
        let digest = env
            .get("digest")
            .map(String::as_str)
            .filter(|d| !d.is_empty());
        let content = match digest {
            Some(digest) => {
                let repo_root = env
                    .get("repo")
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        soopy::discover(std::path::PathBuf::from(&path))
                            .ok()
                            .map(|repository| repository.root)
                    })
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
                    message: format!("unknown process adapter '{execution}'"),
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
            let scalar = if self.execution_for(plan) == "shell" {
                ShellHostAdapter::input(&plan.name, input, &value)?
            } else {
                // Native executors own their typed decode seam.  The current
                // demand ABI still carries scalar values for those executors,
                // but must not route them through the shell adapter or its
                // HostTemplateArgument boundary.  SqlParameter retains the
                // pre-adapter bytes behavior until the native decoder lands.
                ScalarValue::at_seam(&value, ScalarSeam::SqlParameter).map_err(|error| {
                    HostError {
                        host: plan.name.clone(),
                        message: error.to_string(),
                    }
                })?
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
        stdout: &str,
        rel_columns: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<Arrival>, HostError> {
        let span = tracing::info_span!("project", host = %demand.plan.name, bytes = stdout.len());
        let _entered = span.enter();
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
            let key = format!("{}|{:?}", execution, ordered_inputs);
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
        for group in groups {
            let first = group[0];
            let executor = executor_for_plan(first.plan, &self.adapter_rows)
                .expect("validated at construction");
            let command_line = fill_template(&first.plan.template, &first.inputs);
            let env = env_for_inputs(&first.inputs);
            let stdout = {
                let span = tracing::info_span!("host_run", host = %first.plan.name);
                let _entered = span.enter();
                executor.run(&first.plan.name, &command_line, &env)?
            };
            for demand in group {
                arrivals.extend(Self::project(demand, &stdout, self.rel_columns)?);
            }
        }
        Ok(arrivals)
    }
}
