//! S4 SCIP: the Tier-1 resolution wire — the `ScipSource` LOGIC (the seam
//! trait + diet types live in `crate::types`, re-exported through
//! `crate::seams`; this module is the `wire.rs`-style logic half).
//!
//! Commit 4c-i lands `ScipTypescript`:
//! - `build`: subprocess `scip-typescript index` over a directory (v5's argv,
//!   `src/scip_setup.rs` INDEXERS). HERMETIC: the index lands in a fresh temp
//!   dir, and when the root has no tsconfig.json the sources are copied to a
//!   temp workdir first — the indexer's `--infer-tsconfig` WRITES a tsconfig,
//!   and the source dir must never be mutated (fixtures are committed).
//! - `load`: prost decode of index.scip into the diet `ScipIndex` (v5
//!   `src/scip_import.rs::load`, re-runtimed: rust-protobuf -> prost, see the
//!   Cargo.toml dep note).
//!
//! Commit 4d-ii-go lands `ScipGo`: same seam, v5's go argv (`scip-go --output
//! {out}`, verbatim on scip-go 0.2.7), PATH-first with a version-pinned `go
//! run` fallback. scip-go needs a go.mod at the root and writes nothing but
//! the redirected output, so no staging copy exists on the go side.
//! Commit 4d-ii-rust adds `ScipRust`: `rust-analyzer scip` (v5's rust INDEXERS
//! row), PATH-only, ALWAYS staged (cargo metadata writes `target/` under the
//! project root unconditionally). The indexer's symbol model differs from
//! scip-typescript's: symbols are qualified (`rust-analyzer cargo <crate>
//! <version> <path>`), locals are `local N` DOCUMENT-scoped, and position
//! encoding is UTF-8. `load` is shared — the wire is indexer-agnostic.
//!
//! The generated bindings are committed at `scip/scip_proto.rs` (from the
//! vendored `proto/scip.proto`); they stay private to `crate::scip_decode`,
//! which owns the protobuf -> flat-types decode. Only the types in
//! `crate::types` cross the seam.

//! EVERY SPAWN HERE IS BUDGETED. The child runs in its own process group and
//! the whole group dies on the deadline: these indexers fork.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::scip_decode::load_index;
use crate::scip_ensure::{run_capped, Capped};
use crate::shape::Span;
use crate::types::{
    OccurrenceRole, PositionEncoding, ScipDocument, ScipError, ScipIndex, SymbolId,
    ScipSource,
};

/// One budgeted indexer attempt, translated to the seam's error vocabulary.
/// `Ok(None)` means the binary was not launchable, which is the PATH-first
/// probe's signal to try its fallback; a timeout is a hard error and never
/// falls back, because "this took too long" is not answered by running a second
/// copy of the same work.
fn attempt(
    argv: &[&str],
    cwd: &Path,
    log_dir: &Path,
    out: &Path,
) -> Result<Option<PathBuf>, ScipError> {
    match run_capped(argv, cwd, log_dir) {
        Capped::Exited {
            success: true,
            stderr_tail: _,
        } => Ok(Some(out.to_path_buf())),
        Capped::Exited {
            success: false,
            stderr_tail,
        } => Err(ScipError::IndexerFailed(stderr_tail)),
        Capped::Killed { secs } => Err(ScipError::IndexerFailed(format!(
            "{} exceeded the {secs}s budget; process group killed",
            argv.first().copied().unwrap_or("indexer")
        ))),
        Capped::NotLaunched => Ok(None),
    }
}

/// scip-typescript 0.4.0 (the ledger ORACLE entry's version). `build` probes
/// PATH first (v5's `dl index` convention), then falls back to the
/// version-pinned npx form so a machine without the global install still runs
/// the same indexer release.
pub struct ScipTypescript;

/// rust-analyzer's `scip` subcommand (v5's rust INDEXERS row,
/// src/scip_setup.rs:52-58). PATH ONLY: the indexer ships with the toolchain
/// (`rustup component add rust-analyzer`); there is no npx fallback. The
/// indexer resolves through cargo metadata, so `root` must be a Cargo project
/// and every indexed file must be crate-reachable.
pub struct ScipRust;

/// The source extensions scip-typescript (and `TsSource`) covers; the staging
/// copy preserves these, directory structure included.
const TS_EXTS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The rust staging set: the sources + every Cargo.toml (workspace member
/// manifests carry the crate graph the indexer resolves through).
const RUST_EXTS: &[&str] = &["rs"];
const RUST_EXTRA_NAMES: &[&str] = &["Cargo.toml"];

/// The jvm staging set. The build files are what scip-java drives; without
/// them the staged copy has no project to index.
const JAVA_EXTS: &[&str] = &["java", "scala", "kt", "kts"];
const JAVA_EXTRA_NAMES: &[&str] = &[
    "build.gradle.kts",
    "build.gradle",
    "settings.gradle.kts",
    "settings.gradle",
    "pom.xml",
    "gradle.properties",
];

impl ScipSource for ScipTypescript {
    fn indexer(&self) -> &'static str {
        "scip-typescript"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        build_indexer(&TS_SPEC, root)
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        load_index(index_path)
    }
}

impl ScipSource for ScipRust {
    fn indexer(&self) -> &'static str {
        "rust-analyzer"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        build_indexer(&RUST_SPEC, root)
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        load_index(index_path)
    }
}

/// scip-go 0.2.7 (the go row of v5's `src/scip_setup.rs` INDEXERS: bin
/// `scip-go`, argv `scip-go --output {out}` — the argv runs VERBATIM on 0.2.7,
/// the kong CLI routing bare flags to the default `index` command; verified
/// against a scratch module). `build` probes PATH first (v5's `dl index`
/// convention), then falls back to the version-pinned `go run` form (the
/// go-toolchain analog of the ts npx fallback) so a machine without the
/// install still runs the same indexer release.
pub struct ScipGo;

impl ScipSource for ScipGo {
    fn indexer(&self) -> &'static str {
        "scip-go"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        build_indexer(&GO_SPEC, root)
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        // The proto is language-agnostic, so every indexer shares one decode.
        load_index(index_path)
    }
}

/// scip-python (v5's python INDEXERS row, `src/scip_setup.rs:66-72`).
pub struct ScipPython;

/// scip-java (v5's kotlin/java INDEXERS row, `src/scip_setup.rs:80-86`).
pub struct ScipJava;

/// scip-clang (v5's cpp INDEXERS row, `src/scip_setup.rs:87-99`).
pub struct ScipClang;

impl ScipSource for ScipPython {
    fn indexer(&self) -> &'static str {
        "scip-python"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        build_indexer(&PYTHON_SPEC, root)
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        load_index(index_path)
    }
}

impl ScipSource for ScipJava {
    fn indexer(&self) -> &'static str {
        "scip-java"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        build_indexer(&JAVA_SPEC, root)
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        load_index(index_path)
    }
}

impl ScipSource for ScipClang {
    fn indexer(&self) -> &'static str {
        "scip-clang"
    }

    fn build(&self, root: &Path) -> Result<PathBuf, ScipError> {
        build_indexer(&CLANG_SPEC, root)
    }

    fn load(&self, index_path: &Path) -> Result<ScipIndex, ScipError> {
        load_index(index_path)
    }
}

// ── the indexer roster as DATA ────────────────────────────────────────────────
// `bin` + `args` mirror v5 `src/scip_setup.rs` INDEXERS run arrays verbatim.

/// How a non-PATH indexer is reached when the direct binary is not installed.
/// `None` for indexers that ship with their toolchain (rust-analyzer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    None,
    /// `npx -y <pkg>` prepended to the argv; the payload is a pinned release.
    Npx(&'static str),
    GoRun(&'static str),
}

/// Whether the indexer may write into the project root, or must run over a
/// hermetic copy. Three variants, one per existing indexer policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Staging {
    /// Run in place: the indexer writes nothing but the redirected output
    /// (scip-go).
    InPlace,
    /// Always run over a staged copy: the indexer writes the source dir
    /// unconditionally (rust-analyzer's cargo metadata writes `target/`).
    Always {
        exts: &'static [&'static str],
        extra_names: &'static [&'static str],
    },
    /// Run in place when `marker` is present, else stage: the indexer writes
    /// the marker when it is missing (scip-typescript's `--infer-tsconfig`).
    Conditional {
        marker: &'static str,
        exts: &'static [&'static str],
        extra_names: &'static [&'static str],
    },
}

/// One language's SCIP build spec: the binary, the trailing argv (`{out}` is
/// the absolute output path), the fallback, and the staging policy.
#[derive(Clone, Copy, Debug)]
pub struct IndexerSpec {
    pub bin: &'static str,
    pub args: &'static [&'static str],
    pub fallback: Fallback,
    pub staging: Staging,
}

pub static TS_SPEC: IndexerSpec = IndexerSpec {
    bin: "scip-typescript",
    args: &["index", "--infer-tsconfig", "--output", "{out}"],
    fallback: Fallback::Npx("@sourcegraph/scip-typescript@0.4.0"),
    staging: Staging::Conditional {
        marker: "tsconfig.json",
        exts: TS_EXTS,
        extra_names: &[],
    },
};

pub static RUST_SPEC: IndexerSpec = IndexerSpec {
    bin: "rust-analyzer",
    args: &["scip", ".", "--output", "{out}"],
    fallback: Fallback::None,
    staging: Staging::Always {
        exts: RUST_EXTS,
        extra_names: RUST_EXTRA_NAMES,
    },
};

pub static GO_SPEC: IndexerSpec = IndexerSpec {
    bin: "scip-go",
    args: &["--output", "{out}", "./..."],
    fallback: Fallback::GoRun("github.com/scip-code/scip-go/cmd/scip-go@v0.2.7"),
    staging: Staging::InPlace,
};

/// scip-python writes only the redirected output; its pyright analysis reads
/// the tree and caches outside it.
pub static PYTHON_SPEC: IndexerSpec = IndexerSpec {
    bin: "scip-python",
    args: &["index", ".", "--output", "{out}"],
    fallback: Fallback::Npx("@sourcegraph/scip-python"),
    staging: Staging::InPlace,
};

/// scip-java drives gradle or maven, which write `build/` and `target/` under
/// the root, so the copy is not optional.
pub static JAVA_SPEC: IndexerSpec = IndexerSpec {
    bin: "scip-java",
    args: &["index", "--output", "{out}"],
    fallback: Fallback::None,
    staging: Staging::Always {
        exts: JAVA_EXTS,
        extra_names: JAVA_EXTRA_NAMES,
    },
};

/// scip-clang reads the compilation database and writes only `-o`. The compdb
/// names absolute paths, so a staged copy would break every entry.
pub static CLANG_SPEC: IndexerSpec = IndexerSpec {
    bin: "scip-clang",
    args: &["--compdb-path", "compile_commands.json", "-o", "{out}"],
    fallback: Fallback::None,
    staging: Staging::InPlace,
};

/// A PATH binary that runs and FAILS is reported, not retried: a fallback
/// answers "not installed", never "crashed".
fn build_indexer(spec: &IndexerSpec, root: &Path) -> Result<PathBuf, ScipError> {
    let stage = fresh_temp_dir(spec.bin)?;
    let out = stage.join("index.scip");
    let out_str = out.to_string_lossy().into_owned();
    let work = match spec.staging {
        Staging::InPlace => root.to_path_buf(),
        Staging::Always { exts, extra_names } => {
            let work = persistent_stage(spec.bin, root)?;
            copy_sources(root, &work, exts, extra_names)?;
            work
        }
        Staging::Conditional {
            marker,
            exts,
            extra_names,
        } => {
            if root.join(marker).is_file() {
                root.to_path_buf()
            } else {
                let work = persistent_stage(spec.bin, root)?;
                copy_sources(root, &work, exts, extra_names)?;
                work
            }
        }
    };
    let args: Vec<&str> = spec
        .args
        .iter()
        .map(|a| if *a == "{out}" { out_str.as_str() } else { a })
        .collect();
    let direct: Vec<&str> = std::iter::once(spec.bin)
        .chain(args.iter().copied())
        .collect();
    if let Some(path) = attempt(&direct, &work, &stage, &out)? {
        return Ok(path);
    }
    match spec.fallback {
        Fallback::None => Err(ScipError::IndexerMissing(spec.bin)),
        Fallback::Npx(pkg) => {
            let fallback: Vec<&str> = ["npx", "-y", pkg]
                .into_iter()
                .chain(args.iter().copied())
                .collect();
            attempt(&fallback, &work, &stage, &out)?.ok_or(ScipError::IndexerMissing(spec.bin))
        }
        Fallback::GoRun(module) => {
            let fallback: Vec<&str> = ["go", "run", module]
                .into_iter()
                .chain(args.iter().copied())
                .collect();
            attempt(&fallback, &work, &stage, &out)?.ok_or(ScipError::IndexerMissing(spec.bin))
        }
    }
}

/// The SAME staging dir every run for one (root, indexer). A fresh dir each
/// time hands the indexer no `target/`, so every run recompiles every build
/// script and proc-macro cold; on hafley-rs that turned a 12s in-place index
/// into a 25-minute one.
///
/// Under the OS temp dir keyed by the root's path digest, NEVER under the root:
/// the corpus is committed fixture trees in this crate's own tests and the seam
/// law is that reading a corpus never writes to it.
fn persistent_stage(bin: &str, root: &Path) -> Result<PathBuf, ScipError> {
    let key = crate::scip_ensure::root_key(root);
    let work = std::env::temp_dir().join(format!("sprefa-scip-stage-{bin}-{key}"));
    std::fs::create_dir_all(&work)
        .map_err(|e| ScipError::IndexerFailed(format!("stage {}: {e}", work.display())))?;
    Ok(work)
}

/// A fresh uniquely-named temp dir (no tempfile dep): base + pid + nanos.
fn fresh_temp_dir(prefix: &str) -> Result<PathBuf, ScipError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| ScipError::IndexerFailed(format!("mktemp {}: {e}", dir.display())))?;
    Ok(dir)
}

/// Copy the sources under `src_root` to `dst_root`, preserving relative
/// structure: files whose extension is in `exts` plus files whose bare name is
/// in `extra_names` (rust's Cargo.toml manifests).
///
/// A CHILD DIRECTORY CARRYING ITS OWN `.git` IS A DIFFERENT CHECKOUT and is
/// never staged: a nested worktree or submodule is not part of this workspace,
/// and copying one hands the indexer a second copy of every crate. Measured on
/// hafley-rs, whose `.boop-worktrees/**` holds lane checkouts: 2320 `.rs` staged
/// before this rule, 129 after.
pub fn copy_sources(
    src_root: &Path,
    dst_root: &Path,
    exts: &[&str],
    extra_names: &[&str],
) -> Result<(), ScipError> {
    let mut staged: Vec<PathBuf> = Vec::new();
    let mut stack = vec![src_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| ScipError::IndexerFailed(format!("read_dir {}: {e}", dir.display())))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                let is_build_output = matches!(
                    name.as_ref(),
                    "node_modules" | ".git" | "dist" | "out" | "target"
                );
                if !is_build_output && !path.join(".git").exists() {
                    stack.push(path);
                }
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !exts.contains(&ext) && !extra_names.contains(&name.as_ref()) {
                continue;
            }
            let rel = path.strip_prefix(src_root).unwrap_or(&path);
            let dst = dst_root.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ScipError::IndexerFailed(format!("mkdir: {e}")))?;
            }
            std::fs::copy(&path, &dst)
                .map_err(|e| ScipError::IndexerFailed(format!("copy: {e}")))?;
            staged.push(dst);
        }
    }
    prune_unstaged(dst_root, &staged, exts, extra_names)
}

/// A persistent stage keeps whatever a previous run left, so a source deleted
/// from the corpus would still be indexed. Only source files are pruned; the
/// indexer's own `target/` is what the stage exists to keep warm.
fn prune_unstaged(
    dst_root: &Path,
    staged: &[PathBuf],
    exts: &[&str],
    extra_names: &[&str],
) -> Result<(), ScipError> {
    let keep: std::collections::HashSet<&Path> = staged.iter().map(PathBuf::as_path).collect();
    let mut stack = vec![dst_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if name.as_ref() != "target" {
                    stack.push(path);
                }
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !exts.contains(&ext) && !extra_names.contains(&name.as_ref()) {
                continue;
            }
            if !keep.contains(path.as_path()) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

/// The line/col -> byte bridge. SCIP ranges are 0-based (line, col) with cols
/// in the document's `PositionEncoding`; v6 `Span` is byte offsets. The
/// consumer holds the content, so the conversion lives here as a pure fn.
/// `Unspecified` is UTF-16 per the SCIP spec; a col landing mid-character or
/// past the line end is None (malformed range, never clamped into a lie).
pub fn byte_range(content: &[u8], range: [i32; 4], encoding: PositionEncoding) -> Option<Span> {
    byte_range_at(content, &LineTable::build(content), range, encoding)
}

/// Byte offset of each 0-based line start, with the document end as the final
/// entry. One per document, never one per range.
pub struct LineTable {
    starts: Vec<u32>,
}

/// Document bytes a range conversion reads: one per line lookup under the
/// table, one per byte of the document under a scan.
static LINE_READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn line_reads() -> u64 {
    LINE_READS.load(std::sync::atomic::Ordering::Relaxed)
}

impl LineTable {
    pub fn build(content: &[u8]) -> LineTable {
        LINE_READS.fetch_add(content.len() as u64, std::sync::atomic::Ordering::Relaxed);
        let mut starts = Vec::new();
        starts.push(0u32);
        for at in memchr::memchr_iter(b'\n', content) {
            starts.push(at as u32 + 1);
        }
        // The sentinel answers a range naming the line after the final newline,
        // which the scan form answered with content.len().
        if starts.last() != Some(&(content.len() as u32)) {
            starts.push(content.len() as u32);
        }
        LineTable { starts }
    }

    fn line_start(&self, line: i32) -> Option<usize> {
        LINE_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if line < 0 {
            return None;
        }
        self.starts.get(line as usize).map(|start| *start as usize)
    }
}

pub fn byte_range_at(
    content: &[u8],
    lines: &LineTable,
    range: [i32; 4],
    encoding: PositionEncoding,
) -> Option<Span> {
    let line_start = |line: i32| -> Option<usize> { lines.line_start(line) };
    let byte_col = |line: i32, col: i32| -> Option<u32> {
        if col < 0 {
            return None;
        }
        let start = line_start(line)?;
        let line_end = memchr::memchr(b'\n', &content[start..])
            .map(|p| start + p)
            .unwrap_or(content.len());
        let text = std::str::from_utf8(&content[start..line_end]).ok()?;
        let col = col as usize;
        let within = match encoding {
            PositionEncoding::Utf8 => (col <= text.len()).then_some(col),
            PositionEncoding::Unspecified | PositionEncoding::Utf16 => {
                let mut acc = 0usize;
                let mut hit = None;
                for (i, c) in text.char_indices() {
                    if acc == col {
                        hit = Some(i);
                        break;
                    }
                    acc += c.len_utf16();
                }
                hit.or(if acc == col { Some(text.len()) } else { None })
            }
            PositionEncoding::Utf32 => {
                let mut hit = text.char_indices().nth(col).map(|(i, _)| i);
                if hit.is_none() && col == text.chars().count() {
                    hit = Some(text.len());
                }
                hit
            }
        }?;
        Some((start + within) as u32)
    };
    let start = byte_col(range[0], range[1])?;
    let end = byte_col(range[2], range[3])?;
    if end < start {
        return None;
    }
    Some(Span {
        start,
        len: end - start,
    })
}

// ── the resolution joins (4c-ii; shared by the Resolve<CallF> arms and the
//    golden_parity scip ratchet — the arm and the test MUST read the same
//    occurrence the same way, so the conventions live here exactly once) ────

// ── the index-the-index caches ────────────────────────────────────────────────
// The seam functions below are called once per call site over an immutable
// `ScipIndex`, so each doc's range->span table and the index's symbol->def
// map are computed once and reused. The caches key on the document/index
// address plus a cheap fingerprint (occurrence count, content length, path
// digest): within one process a freed index's address can be reused by a
// later allocation, and the fingerprint is what keeps a stale entry from
// answering for a different index.

/// Per document: the byte span of every convertible occurrence, sorted by
/// (start, end), plus the document's line table. `site_occurrence` binary
/// searches this instead of scanning `doc.occurrences` and rebuilding the
/// line table per site. The third slot is the occurrence's interned symbol
/// (`ScipIndex::symbol`), so the search never touches the occurrence rows.
struct DocOccCache {
    spans: Vec<(u32, u32, SymbolId)>,
    lines: LineTable,
}

#[derive(Hash, PartialEq, Eq)]
struct DocKey {
    doc: usize,
    occ_len: usize,
    content_len: usize,
    path_hash: u64,
}

fn path_digest(path: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn doc_cache(doc: &ScipDocument, content: &[u8]) -> Arc<DocOccCache> {
    let key = DocKey {
        doc: doc as *const ScipDocument as usize,
        occ_len: doc.occurrences.len(),
        content_len: content.len(),
        path_hash: path_digest(&doc.relative_path),
    };
    let mut guard = DOC_CACHES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(map) = guard.as_ref() {
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }
    let lines = LineTable::build(content);
    let mut spans: Vec<(u32, u32, SymbolId)> = Vec::with_capacity(doc.occurrences.len());
    for occ in &doc.occurrences {
        if let Some(span) = byte_range_at(content, &lines, occ.range, doc.position_encoding) {
            spans.push((span.start, span.end(), occ.symbol));
        }
    }
    spans.sort_unstable_by_key(|(start, end, _)| (*start, *end));
    let cache = Arc::new(DocOccCache { spans, lines });
    guard
        .get_or_insert_with(HashMap::new)
        .insert(key, cache.clone());
    cache
}

static DOC_CACHES: Mutex<Option<HashMap<DocKey, Arc<DocOccCache>>>> = Mutex::new(None);

/// symbol -> (document ix, occurrence ix) for the first definition-role
/// occurrence, first-wins in document order — the same resolution
/// `definition_of` answers by scan.
type DefMap = HashMap<SymbolId, (usize, u32)>;

#[derive(Hash, PartialEq, Eq)]
struct IndexKey {
    index: usize,
    doc_len: usize,
    external_len: usize,
    first_path_hash: u64,
}

fn def_map(index: &ScipIndex) -> Arc<DefMap> {
    let key = IndexKey {
        index: index as *const ScipIndex as usize,
        doc_len: index.documents.len(),
        external_len: index.external_symbols.len(),
        first_path_hash: index
            .documents
            .first()
            .map(|d| path_digest(&d.relative_path))
            .unwrap_or(0),
    };
    let mut guard = DEF_MAPS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(map) = guard.as_ref() {
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }
    let mut map: DefMap = HashMap::new();
    for (doc_ix, doc) in index.documents.iter().enumerate() {
        for (occ_ix, occ) in doc.occurrences.iter().enumerate() {
            if !occ.roles.contains(OccurrenceRole::DEFINITION) {
                continue;
            }
            map.entry(occ.symbol).or_insert((doc_ix, occ_ix as u32));
        }
    }
    let map = Arc::new(map);
    guard.get_or_insert_with(HashMap::new).insert(key, map.clone());
    map
}

static DEF_MAPS: Mutex<Option<HashMap<IndexKey, Arc<DefMap>>>> = Mutex::new(None);

/// The content join for one loaded index: for every document, its content id +
/// bytes from the rev-correct reader (None when the reader can't read the
/// document — it is then external to the corpus). Parallel to
/// `index.documents` (same order, same length). Whole-project state built once
/// per refresh; the resolve arms build it per call at fixture scale (the
/// engine caches when this gets hot — the OnceLock discipline of `IndexBag`
/// covers it).
pub fn join_documents(
    index: &ScipIndex,
    reader: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Vec<Option<(crate::shape::ContentId, Vec<u8>)>> {
    index
        .documents
        .iter()
        .map(|doc| {
            reader(&doc.relative_path)
                .map(|content| (crate::shape::content_id_of(&content), content))
        })
        .collect()
}

/// The scip occurrence answering one call site: contained in the site span
/// (v6's site convention — a call site is the callee expression, a new-
/// expression the whole expression; the callee identifier's occurrence sits
/// inside either) whose source text equals the callee name (the trailing
/// segment; filters the receiver/path occurrences — `Math` in `Math.sqrt` —
/// and the argument occurrences inside a new-expression). Deterministic:
/// first by (start, end). Returns the site's interned symbol.
pub fn site_occurrence(
    doc: &ScipDocument,
    content: &[u8],
    site: Span,
    callee: &str,
) -> Option<SymbolId> {
    let cache = doc_cache(doc, content);
    // Containment needs span.start >= site.start, so the first candidate is
    // the first cached span at or after the site's start byte.
    let first = cache
        .spans
        .partition_point(|(start, _, _)| *start < site.start);
    // The spans are sorted by (start, end), so the first text match IS the
    // min-(start, end) hit the range comparison used to pick.
    cache.spans[first..]
        .iter()
        .take_while(|&&(start, _, _)| start <= site.end())
        .filter(|&&(_, end, _)| end <= site.end())
        .find(|&&(start, end, _)| &content[start as usize..end as usize] == callee.as_bytes())
        .map(|&(_, _, symbol)| symbol)
}

/// The definition occurrence of a symbol: `local ` symbols are DOCUMENT-
/// scoped (scip reuses `local 0` per file — v5's per-document keying), so the
/// search starts and ends at the site's own document; global symbols are
/// corpus-unique, so the first definition-role occurrence across all
/// documents answers. Returns (document_ix, def range) — None means the
/// symbol has no definition in the indexed corpus (an EXTERNAL: a library
/// symbol, or an unresolved reference).
pub fn definition_of(
    index: &ScipIndex,
    doc_ix: usize,
    symbol: SymbolId,
) -> Option<(usize, [i32; 4])> {
    if index.symbol(symbol).starts_with("local ") {
        // `local N` is per-document: the map's first-wins entry names some
        // other file's local, so the search stays at the site's own document.
        let doc = &index.documents[doc_ix];
        return doc
            .occurrences
            .iter()
            .find(|occ| occ.symbol == symbol && occ.roles.contains(OccurrenceRole::DEFINITION))
            .map(|occ| (doc_ix, occ.range));
    }
    let map = def_map(index);
    let (def_doc_ix, occ_ix) = map.get(&symbol)?;
    let occ = &index.documents[*def_doc_ix].occurrences[*occ_ix as usize];
    Some((*def_doc_ix, occ.range))
}
