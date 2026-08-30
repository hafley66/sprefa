//! ENSURE-INDEX: v5's `src/scip_setup.rs` contract, ported to this crate.
//!
//! v5's `scip_want` gate answers one question — "does this root have a loadable
//! SCIP index, and if not can we make one cheaply?" — with four moves, all
//! ported here:
//!   1. an index already on disk WINS untouched (`index_path`, newest-mtime
//!      across the three known locations plus `$SPREFA_SCIP_INDEX`);
//!   2. otherwise DETECT the language by marker files at the root (`INDEXERS`);
//!   3. run every detected indexer whose binary is on PATH, merging the parts;
//!   4. a root with markers and no installed toolchain is a LOUD NAMED SKIP,
//!      never a failure — a missing indexer skips the root, it never kills the
//!      caller.
//! Freshness is digest-of-set, never mtime (user decision 2026-08-21); the
//! `SPREFA_SCIP_INDEX` override is exempt from it and wins untouched.
//!
//! WHAT CHANGED, and why.
//!
//! THE BUDGET (new here, the timeout-gun law). v5 runs the indexer with a bare
//! `Command::status()` and no bound at all: rust-analyzer over a large cargo
//! workspace can run for many minutes, and nothing stops it. Every run here goes
//! through `run_capped`, which puts the child in ITS OWN PROCESS GROUP and kills
//! the whole group on the deadline. The group is the part that matters: these
//! indexers fork (rust-analyzer runs `cargo metadata`, scip-typescript runs
//! `tsc`), so killing the direct child alone orphans the real worker. A run that
//! exceeds the budget is a named skip with its wall time, never a hang.
//!
//! THE RUN ITSELF is delegated to the `ScipSource` impls in `crate::scip`, not
//! re-spawned from a v5 argv table. Those impls already carry v5's argv verbatim
//! (see their headers) AND add the hermetic staging v5 lacks: the indexers write
//! into the source dir when left alone (scip-typescript's `--infer-tsconfig`
//! writes a tsconfig.json, rust-analyzer's cargo metadata writes `target/`), and
//! the seam's law is that a corpus is never mutated by reading it. So `INDEXERS`
//! here carries detection, the PATH probe and the install hint; the spawn lives
//! where the staging already is.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::types::{ScipError, ScipSource};

/// One language's SCIP indexer: how to detect the language, the binary to probe
/// on PATH, the one-line install hint, and the `ScipSource` that runs it.
/// Ported from v5 `src/scip_setup.rs` INDEXERS (marker files, bin and install
/// strings are v5's, verbatim).
pub struct Indexer {
    /// The language name this row answers for; also the merge part's filename.
    pub lang: &'static str,
    /// Any-of marker files at the root that identify the language/build.
    pub markers: &'static [&'static str],
    /// The indexer executable probed on PATH.
    pub bin: &'static str,
    /// One-line install command, printed on a no-toolchain skip.
    pub install: &'static str,
    /// The seam that runs it (hermetic staging + the indexer-agnostic decode).
    pub source: &'static dyn ScipSource,
}

/// The roster. Marker files, binaries and install hints are v5's INDEXERS rows
/// verbatim, in v5's order (`src/scip_setup.rs:51-99`).
pub static INDEXERS: &[Indexer] = &[
    Indexer {
        lang: "rust",
        markers: &["Cargo.toml"],
        bin: "rust-analyzer",
        install: "rustup component add rust-analyzer",
        source: &crate::scip::ScipRust,
    },
    Indexer {
        lang: "typescript",
        markers: &["tsconfig.json", "package.json"],
        bin: "scip-typescript",
        install: "npm install -g @sourcegraph/scip-typescript",
        source: &crate::scip::ScipTypescript,
    },
    Indexer {
        lang: "python",
        markers: &["pyproject.toml", "setup.py", "requirements.txt"],
        bin: "scip-python",
        install: "npm install -g @sourcegraph/scip-python",
        source: &crate::scip::ScipPython,
    },
    Indexer {
        lang: "go",
        markers: &["go.mod"],
        bin: "scip-go",
        install: "go install github.com/scip-code/scip-go/cmd/scip-go@latest",
        source: &crate::scip::ScipGo,
    },
    Indexer {
        lang: "kotlin/java",
        markers: &["build.gradle.kts", "build.gradle", "pom.xml"],
        bin: "scip-java",
        install: "coursier install --contrib scip-java  (see sourcegraph/scip-java)",
        source: &crate::scip::ScipJava,
    },
    Indexer {
        lang: "cpp",
        markers: &["compile_commands.json", "CMakeLists.txt"],
        bin: "scip-clang",
        install: "download from github.com/sourcegraph/scip-clang/releases",
        source: &crate::scip::ScipClang,
    },
];

/// How long ONE indexer may run before its process group is killed.
///
/// The default is a ceiling, not a target: a real indexer run over a workspace
/// is minutes, and the budget exists so an indexer that wedges cannot hold the
/// caller forever. Measured walls that set it are in the lane report; the
/// fixture corpora here run in seconds.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct IndexBudget {
    pub secs: u64,
}

impl Default for IndexBudget {
    fn default() -> Self {
        Self { secs: 600 }
    }
}

impl IndexBudget {
    /// `SPREFA_SCIP_TIMEOUT_SECS` when it parses to a positive integer, else the
    /// default. A zero or unparseable value is the default rather than "no
    /// budget": the law has no opt-out.
    pub fn from_env() -> Self {
        match std::env::var("SPREFA_SCIP_TIMEOUT_SECS")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
        {
            Some(secs) if secs > 0 => Self { secs },
            _ => Self::default(),
        }
    }
}

/// Why one detected indexer produced no index. Every variant is a NAMED skip
/// that reaches the caller as data, so a root that cannot be indexed says which
/// of the three things went wrong instead of yielding an empty stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// No marker file at the root names any language in the roster, so no
    /// indexer was even a candidate. A row rather than silence: an empty stream
    /// with no explanation reads as "this project has no symbols", and "you
    /// pointed me at a directory that is not a project root" is the far more
    /// likely truth.
    NoMarkers,
    /// Markers matched, the binary is not on PATH. Carries v5's install hint.
    NotInstalled { install: &'static str },
    /// The run exceeded `IndexBudget`; the process group was killed.
    TimedOut { secs: u64 },
    /// The indexer ran and failed, or could not be launched. Carries its own
    /// last stderr line.
    Failed { detail: String },
}

impl SkipReason {
    /// The stable slug a consumer matches on, distinct from the human detail.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::NoMarkers => "no_markers",
            Self::NotInstalled { .. } => "not_installed",
            Self::TimedOut { .. } => "timed_out",
            Self::Failed { .. } => "failed",
        }
    }

    /// The human half of the skip line.
    pub fn detail(&self) -> String {
        match self {
            Self::NoMarkers => format!("no marker file at the root; looked for {}", markers()),
            Self::NotInstalled { install } => format!("not on PATH; install: {install}"),
            Self::TimedOut { secs } => {
                format!("exceeded the {secs}s budget; process group killed")
            }
            Self::Failed { detail } => detail.clone(),
        }
    }
}

/// One detected indexer that produced nothing, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexerSkip {
    pub lang: &'static str,
    pub bin: &'static str,
    pub reason: SkipReason,
}

/// The file set an index was built from: sorted, deduplicated (path, digest)
/// pairs plus the digest of that list, which is the freshness coordinate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexSet {
    entries: Vec<(String, String)>,
    digest: String,
}

impl IndexSet {
    /// Sorted and deduplicated at construction, so one corpus arriving in two
    /// orders is one set and one digest.
    pub fn new<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut entries: Vec<(String, String)> = pairs.into_iter().collect();
        entries.sort();
        entries.dedup();
        let joined: String = entries
            .iter()
            .map(|(path, digest)| format!("{path} {digest}\n"))
            .collect();
        let digest = match crate::shape::ContentId::blake3(joined.as_bytes()) {
            crate::shape::ContentId::Blake3(bytes) => hex_of(&bytes),
            crate::shape::ContentId::GitBlob(oid) => oid.0.to_string(),
        };
        Self { entries, digest }
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(path, _)| path.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|(path, digest)| format!("{path} {digest}"))
            .collect()
    }
}

fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[derive(serde::Serialize, serde::Deserialize)]
struct IndexSetSidecar {
    digest: String,
    files: Vec<String>,
}

/// The sidecar sits at the index path plus a suffix, so the two are found,
/// moved and deleted together and no second directory is invented.
fn sidecar_path(index: &Path) -> PathBuf {
    let mut name = index.as_os_str().to_os_string();
    name.push(".set.json");
    PathBuf::from(name)
}

fn recorded_digest(index: &Path) -> Option<String> {
    let text = std::fs::read_to_string(sidecar_path(index)).ok()?;
    serde_json::from_str::<IndexSetSidecar>(&text)
        .ok()
        .map(|sidecar| sidecar.digest)
}

/// Stamp `index` with the set it was built from, which is what later makes it
/// reusable. Best effort: an unwritable dir costs the next run its reuse only.
pub fn record_index_set(index: &Path, set: &IndexSet) {
    let sidecar = IndexSetSidecar {
        digest: set.digest().to_string(),
        files: set.lines(),
    };
    if let Ok(text) = serde_json::to_string(&sidecar) {
        let _ = std::fs::write(sidecar_path(index), text);
    }
}

/// What `ensure_index` found or made. `index` is None only when no index could
/// be produced, and then `skips` says why for every detected indexer; an empty
/// `skips` with no index means the root carries no marker file at all.
#[derive(Clone, Debug)]
pub struct EnsureReport {
    /// The loadable index, when there is one.
    pub index: Option<PathBuf>,
    /// True when an index already on disk answered and no indexer ran.
    pub reused: bool,
    /// Languages whose indexer ran and contributed a part, with wall times.
    pub ran: Vec<(&'static str, Duration)>,
    /// Detected indexers that produced nothing, each with a named reason.
    pub skips: Vec<IndexerSkip>,
}

/// Ensure `root` has a loadable SCIP index (v5 `ensure_index`, budgeted).
///
/// `cache_dir` is where a freshly built index is placed and found again; it is
/// `<root>/.dl/.state` in v5 and the caller's choice here so a test never writes
/// into a committed fixture. Nothing under it is read as an input except the
/// index this function itself placed there (the reuse path goes through
/// `index_path`, which knows all three v5 locations).
pub fn ensure_index(root: &Path, cache_dir: &Path, budget: IndexBudget) -> EnsureReport {
    ensure_index_for_set(root, cache_dir, budget, None)
}

/// `ensure_index` with a freshness ask. `Some(set)` reuses an index only when
/// its recorded set digest equals this one; `None` is the unconditional v5 form.
pub fn ensure_index_for_set(
    root: &Path,
    cache_dir: &Path,
    budget: IndexBudget,
    set: Option<&IndexSet>,
) -> EnsureReport {
    let want = set.map(IndexSet::digest);
    if let Some(path) = index_path_for_set(root, cache_dir, want) {
        return EnsureReport {
            index: Some(path),
            reused: true,
            ran: Vec::new(),
            skips: Vec::new(),
        };
    }
    let detected = detect(root);
    let mut ran = Vec::new();
    let mut skips = Vec::new();
    if detected.is_empty() {
        return EnsureReport {
            index: None,
            reused: false,
            ran,
            skips: vec![IndexerSkip {
                lang: "none",
                bin: "none",
                reason: SkipReason::NoMarkers,
            }],
        };
    }
    let mut parts: Vec<(&'static str, PathBuf)> = Vec::new();
    for indexer in detected {
        if which(indexer.bin).is_none() {
            skips.push(IndexerSkip {
                lang: indexer.lang,
                bin: indexer.bin,
                reason: SkipReason::NotInstalled {
                    install: indexer.install,
                },
            });
            continue;
        }
        let started = Instant::now();
        match build_capped(indexer, root, budget) {
            Ok(part) => {
                ran.push((indexer.lang, started.elapsed()));
                parts.push((indexer.lang, part));
            }
            Err(reason) => skips.push(IndexerSkip {
                lang: indexer.lang,
                bin: indexer.bin,
                reason,
            }),
        }
    }
    if parts.is_empty() {
        return EnsureReport {
            index: None,
            reused: false,
            ran,
            skips,
        };
    }
    match place(&parts, cache_dir, set) {
        Ok(path) => EnsureReport {
            index: Some(path),
            reused: false,
            ran,
            skips,
        },
        Err(err) => {
            skips.push(IndexerSkip {
                lang: "merge",
                bin: "sprefa-extract",
                reason: SkipReason::Failed {
                    detail: err.to_string(),
                },
            });
            EnsureReport {
                index: None,
                reused: false,
                ran,
                skips,
            }
        }
    }
}

/// Place the built parts at the cache path: a single part moves, several merge.
/// The merge is a document union (v5 `scip_import::merge_files`), which is sound
/// because every indexer namespaces its symbols by tool and package.
fn place(
    parts: &[(&'static str, PathBuf)],
    cache_dir: &Path,
    set: Option<&IndexSet>,
) -> Result<PathBuf, ScipError> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| ScipError::Parse(format!("mkdir {}: {e}", cache_dir.display())))?;
    let out = cache_dir.join("index.scip");
    if let [(_, only)] = parts {
        std::fs::rename(only, &out)
            .or_else(|_| std::fs::copy(only, &out).map(|_| ()))
            .map_err(|e| ScipError::Parse(format!("place {}: {e}", out.display())))?;
    } else {
        let sources: Vec<PathBuf> = parts.iter().map(|(_, path)| path.clone()).collect();
        crate::scip_decode::merge_indexes(&sources, &out)?;
    }
    gitignore_state(cache_dir);
    if let Some(set) = set {
        record_index_set(&out, set);
    }
    Ok(out)
}

/// Ensure a `.dl/.gitignore` covers the `.state/` runtime dir, so a turnkey
/// build never leaves a committable index blob in a worktree (v5
/// `gitignore_index`). Only fires when the cache is the v5 `.dl/.state` shape;
/// a caller-chosen temp cache needs no gitignore. Best effort by design: a
/// read-only tree is not a reason to refuse to index it.
fn gitignore_state(cache_dir: &Path) {
    if cache_dir.file_name().and_then(|n| n.to_str()) != Some(".state") {
        return;
    }
    let Some(dl_dir) = cache_dir.parent() else {
        return;
    };
    if dl_dir.file_name().and_then(|n| n.to_str()) != Some(".dl") {
        return;
    }
    let gitignore = dl_dir.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == ".state/") {
        return;
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(".state/\n");
    let _ = std::fs::create_dir_all(dl_dir);
    let _ = std::fs::write(&gitignore, out);
}

/// Run one indexer's `ScipSource::build` under the budget.
///
/// `build` spawns and waits internally, so the cap is applied by running the
/// build on a worker thread and abandoning the wait at the deadline. The
/// PROCESS side of the kill is what actually stops the work: `run_capped`
/// (which the seam's builds call) owns the process group. This wrapper exists
/// for the `IndexerMissing` / `IndexerFailed` translation.
fn build_capped(
    indexer: &Indexer,
    root: &Path,
    budget: IndexBudget,
) -> Result<PathBuf, SkipReason> {
    match capped_run_scope(budget, || indexer.source.build(root)) {
        Some(Ok(path)) => Ok(path),
        Some(Err(ScipError::IndexerMissing(bin))) => Err(SkipReason::NotInstalled {
            install: INDEXERS
                .iter()
                .find(|row| row.bin == bin)
                .map(|row| row.install)
                .unwrap_or("see the indexer's own install docs"),
        }),
        Some(Err(err)) => Err(SkipReason::Failed {
            detail: err.to_string(),
        }),
        None => Err(SkipReason::TimedOut { secs: budget.secs }),
    }
}

thread_local! {
    /// The budget the current thread's `run_capped` calls must honor. Set for
    /// the duration of one `capped_run_scope`; the seam's `build` bodies read
    /// it through `run_capped` without threading a budget argument through the
    /// `ScipSource` trait (which is shared with the non-budgeted `load` path).
    static ACTIVE_BUDGET: std::cell::Cell<Option<IndexBudget>> = const { std::cell::Cell::new(None) };
}

/// Run `body` with `budget` installed for this thread's `run_capped` calls.
/// Returns None when the body's own capped subprocess was killed on the
/// deadline, which is the only way a build overruns: every spawn inside a
/// `ScipSource::build` goes through `run_capped`.
fn capped_run_scope<T>(budget: IndexBudget, body: impl FnOnce() -> T) -> Option<T> {
    let previous = ACTIVE_BUDGET.with(|slot| slot.replace(Some(budget)));
    let out = body();
    ACTIVE_BUDGET.with(|slot| slot.set(previous));
    if TIMED_OUT.with(|flag| flag.replace(false)) {
        return None;
    }
    Some(out)
}

thread_local! {
    /// Set by `run_capped` when it kills a process group on the deadline, read
    /// and cleared by `capped_run_scope`.
    static TIMED_OUT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// What one capped subprocess did.
pub enum Capped {
    /// It exited on its own. `success` is its exit status, `stderr_tail` its
    /// last nonempty stderr line.
    Exited { success: bool, stderr_tail: String },
    /// The deadline passed and the whole process group was killed.
    Killed { secs: u64 },
    /// The binary could not be launched at all.
    NotLaunched,
}

/// Run one subprocess under the ambient budget, in its OWN PROCESS GROUP, and
/// kill the WHOLE GROUP if the deadline passes.
///
/// THE GROUP IS THE POINT. Every indexer here forks: rust-analyzer runs `cargo
/// metadata`, scip-typescript runs the TypeScript compiler. Killing the direct
/// child leaves those children running and reparented, which is the shape the
/// timeout-gun law exists to stop — a bounded wait that leaks unbounded work is
/// not a bound.
///
/// stdout and stderr go to FILES in `log_dir` rather than pipes. A piped child
/// that fills the 64KB pipe buffer while nothing reads it deadlocks, and this
/// waits by polling rather than by `output()`'s concurrent drain, so pipes are
/// exactly the wrong choice here. rust-analyzer is chatty enough to hit it.
pub fn run_capped(argv: &[&str], cwd: &Path, log_dir: &Path) -> Capped {
    let budget = ACTIVE_BUDGET
        .with(|slot| slot.get())
        .unwrap_or_else(IndexBudget::from_env);
    let (Some(program), args) = (argv.first(), &argv[1..]) else {
        return Capped::NotLaunched;
    };
    let _ = std::fs::create_dir_all(log_dir);
    let err_path = log_dir.join("indexer.stderr.log");
    let (Ok(out_file), Ok(err_file)) = (
        std::fs::File::create(log_dir.join("indexer.stdout.log")),
        std::fs::File::create(&err_path),
    ) else {
        return Capped::NotLaunched;
    };
    // Every spawn is visible: an indexer run is the one operation allowed past
    // the ten-second law, so its wall must be attributable to a named process.
    let span = tracing::warn_span!("process_spawn", bin = program, args = args.len());
    let _entered = span.enter();
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(out_file))
        .stderr(std::process::Stdio::from(err_file));
    new_process_group(&mut command);
    let Ok(mut child) = command.spawn() else {
        return Capped::NotLaunched;
    };
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(budget.secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Capped::Exited {
                    success: status.success(),
                    stderr_tail: stderr_tail(&err_path),
                }
            }
            Ok(None) => {}
            Err(_) => return Capped::NotLaunched,
        }
        if Instant::now() >= deadline {
            tracing::warn!(
                pid,
                secs = budget.secs,
                "indexer exceeded its budget; sending SIGKILL to its process group"
            );
            kill_process_group(pid);
            let _ = child.kill();
            let _ = child.wait();
            TIMED_OUT.with(|flag| flag.set(true));
            return Capped::Killed { secs: budget.secs };
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Put the child in a fresh process group with pgid == its own pid, so one
/// signal reaches every process it forks. Stable std since 1.64, no libc.
#[cfg(unix)]
fn new_process_group(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn new_process_group(_command: &mut std::process::Command) {}

/// SIGKILL the whole group led by `pid`. Spelled through `/bin/kill` with a
/// negated pid rather than a `libc::killpg` call: the crate carries no libc
/// dependency, and adding one for a single signal is not a trade worth making.
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    let span = tracing::warn_span!("process_spawn", bin = "kill", args = 2);
    let _entered = span.enter();
    let _ = std::process::Command::new("kill")
        .args(["-9", &format!("-{pid}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(unix))]
fn kill_process_group(_pid: u32) {}

/// The last nonempty line of a log file, trimmed: the indexer's own error line.
fn stderr_tail(path: &Path) -> String {
    last_error_line(&std::fs::read_to_string(path).unwrap_or_default())
}

/// The last line of `text` that is not a `note:` line, trimmed. A rust panic's
/// tail is the panic line followed by `note: Some details are omitted, run
/// with RUST_BACKTRACE=1 ...`; the panic line is the detail a skip row should
/// carry, and the note names only the runtime's own behavior.
pub fn last_error_line(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let picked = lines
        .iter()
        .rev()
        .find(|l| !l.trim_start().starts_with("note:"))
        .or(lines.last());
    picked.map(|l| l.trim().to_string()).unwrap_or_default()
}

/// Every marker file the roster looks for, as one comma-separated list. Named
/// in the `no_markers` skip so a caller reads what would have worked.
fn markers() -> String {
    let mut all: Vec<&'static str> = INDEXERS
        .iter()
        .flat_map(|indexer| indexer.markers.iter().copied())
        .collect();
    all.sort_unstable();
    all.join(", ")
}

/// The indexers whose marker files are present at `root` (v5 `detect`).
pub fn detect(root: &Path) -> Vec<&'static Indexer> {
    INDEXERS
        .iter()
        .filter(|indexer| {
            indexer
                .markers
                .iter()
                .any(|marker| root.join(marker).exists())
        })
        .collect()
}

/// First PATH entry holding an executable named `bin` (v5 `which`, verbatim
/// shape: a best-effort probe, not a shell-out).
pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if candidate
                .metadata()
                .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
            {
                return Some(candidate);
            }
        }
        #[cfg(not(unix))]
        {
            return Some(candidate);
        }
    }
    None
}

/// Where an existing index is found (v5 `scip_import::index_path`):
///   1. `$SPREFA_SCIP_INDEX` when it names a file;
///   2. the NEWEST by mtime among `<root>/index.scip`, `<cache_dir>/index.scip`
///      and `<root>/.dl/index.scip`.
///
/// NEWEST-WINS, not a fixed order, and v5 learned that the hard way: a fixed
/// order silently shadowed a fresh re-index at a lower-priority path for two
/// days. Whichever tool wrote an index most recently is the one the user means.
pub fn index_path(root: &Path, cache_dir: &Path) -> Option<PathBuf> {
    index_path_for_set(root, cache_dir, None)
}

/// The informed-by-default probe for a resolve run with no explicit SCIP
/// flags: the index that matches the file set's freshness digest exactly, in
/// v5's default cache location. `None` means the resolve falls back to the
/// plain name-match leg; a stale or sidecar-less index is never adopted.
pub fn fresh_index_for_set(root: &Path, set_digest: &str) -> Option<PathBuf> {
    index_path_for_set(root, &default_cache_dir(root), Some(set_digest))
}

/// `index_path` with the freshness ask. `Some(digest)` keeps only candidates
/// whose recorded set digest equals it; mtime then breaks the remaining tie.
pub fn index_path_for_set(root: &Path, cache_dir: &Path, want: Option<&str>) -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("SPREFA_SCIP_INDEX") {
        let explicit = PathBuf::from(explicit);
        if explicit.is_file() {
            return Some(explicit);
        }
    }
    [
        root.join("index.scip"),
        cache_dir.join("index.scip"),
        root.join(".dl").join("index.scip"),
    ]
    .into_iter()
    .filter(|candidate| candidate.is_file())
    .filter(|candidate| match want {
        None => true,
        Some(want) => recorded_digest(candidate).as_deref() == Some(want),
    })
    .max_by_key(|candidate| {
        std::fs::metadata(candidate)
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    })
}

/// The v5 cache location for a root: `<root>/.dl/.state`.
pub fn default_cache_dir(root: &Path) -> PathBuf {
    root.join(".dl").join(".state")
}

/// A cache location for a root that is OUTSIDE it, keyed by the root path so it
/// is the same directory every run. For callers that read a corpus they do not
/// own: a committed fixture tree must not gain a `.dl/` from being resolved.
pub fn external_cache_dir(root: &Path) -> PathBuf {
    std::env::temp_dir().join(format!("sprefa-scip-cache-{}", root_key(root)))
}

/// The first eight bytes of the root path's digest, hex.
pub fn root_key(root: &Path) -> String {
    match crate::shape::ContentId::blake3(root.as_os_str().as_encoded_bytes()) {
        crate::shape::ContentId::Blake3(bytes) => hex_of(&bytes[..8]),
        crate::shape::ContentId::GitBlob(oid) => oid.0.to_string(),
    }
}
