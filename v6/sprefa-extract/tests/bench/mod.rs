//! The ratchet's shared library: corpus enumeration, the
//! `src_path src_name dst_path dst_name` normal form (the Rust port of
//! `plans/extract-bench-2026-08-29/normalize.py`, which stays the reference),
//! scoring against the committed oracle tsvs, and RATCHET.tsv IO. Shared by
//! `tests/ratchet_recall.rs` and `tests/bench_normal_form.rs` via
//! `mod bench;`; a directory under `tests/` compiles no test binary of its
//! own.
//!
//! Direction convention (ORACLES.REPORT.md:583): recall = overlap / |oracle|,
//! precision = overlap / |ours|, both percent.

// Each test binary links the whole module but uses only its half (the parity
// test never scores; the ratchet never serializes raw JSONL), so unused-code
// warnings here are the sharing, not rot.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use sprefa_extract::{resolve_project, FlatFact, ResolveArms, ResolveRequest, ScipMode, ScipRecords};

/// Where the committed oracle tsvs and RATCHET.tsv live.
pub const BENCH_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plans/extract-bench-2026-08-29"
);

/// The per-call wall budget (the timeout-gun law; every extract call under
/// timeout 30). The go corpus sits near 12 s median at #579, known red and
/// owned by the speed lane, so the budget has room but is not open-ended.
pub const WALL_BUDGET_MS: u128 = 30_000;

pub struct Corpus {
    pub lang: &'static str,
    pub root: PathBuf,
    /// (family, oracle tsv file name) pairs; every file must sit in BENCH_DIR.
    pub oracles: &'static [(&'static str, &'static str)],
}

/// The three corpora in COMMON.md order. Roots are machine-local checkouts;
/// the `RATCHET_*_ROOT` overrides exist so another machine can point the
/// ratchet at its own copies.
pub fn corpus(lang: &str) -> Corpus {
    let root = |var: &str, default: &str| {
        std::env::var(var)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(default))
    };
    match lang {
        "ts5" => Corpus {
            lang: "ts5",
            root: root("RATCHET_TS_ROOT", "/Users/chrishafley/projects/TypeScript-5.9"),
            oracles: &[
                ("call", "ts5.oracle.call.tsv"),
                ("call", "ts.codeql2.call.tsv"),
                ("module", "ts.madge.module.tsv"),
            ],
        },
        "go" => Corpus {
            lang: "go",
            root: root("RATCHET_GO_ROOT", "/Users/chrishafley/projects/typescript-go"),
            oracles: &[
                ("call", "go.oracle.call.vta.bare.tsv"),
                ("call", "go.codeql2.call.tsv"),
                ("module", "go.oracle.module.tsv"),
                ("type", "go.oracle.type.typedecl.tsv"),
            ],
        },
        "rust" => Corpus {
            lang: "rust",
            root: root("RATCHET_RUST_ROOT", "/Users/chrishafley/projects/rust-analyzer"),
            oracles: &[
                ("call", "rust.oracle.call.tsv"),
                ("call", "rust.scip_override.call.tsv"),
                ("call", "rust.codeql.call.tsv"),
                ("type", "rust.oracle.type.typedecl.tsv"),
            ],
        },
        other => panic!("unknown ratchet corpus '{other}'"),
    }
}

/// The file rule the bench lab measured against (ORACLES.REPORT.md:30-31):
/// ts5 is `src/**` minus `src/lib` (the bundled lib .d.ts files), go is every
/// `.go` under the root, rust is every `.rs` under `crates/` whose path
/// carries a `src` component. Generated-but-tracked files (rust-analyzer's
/// `proc-macro-test` pair) come and go with local builds; the ratchet pins
/// whatever is on disk.
fn wants(lang: &str, rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    match lang {
        "ts5" => {
            parts.first() == Some(&"src")
                && !(parts.len() >= 2 && parts[1] == "lib")
                && rel.ends_with(".ts")
        }
        "go" => rel.ends_with(".go"),
        "rust" => {
            parts.first() == Some(&"crates")
                && parts.len() > 2
                && parts[1..].contains(&"src")
                && rel.ends_with(".rs")
        }
        _ => false,
    }
}

pub fn enumerate(corpus: &Corpus) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![corpus.root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(&corpus.root) else {
                continue;
            };
            let Some(rel) = rel.to_str() else { continue };
            if wants(corpus.lang, rel) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

// ── the normal form (the normalize.py port) ─────────────────────────────────

/// normalize.py `relp`: a path under the corpus root becomes root-relative,
/// anything else passes through untouched.
fn rel(root: &str, path: &str) -> String {
    match path.strip_prefix(root) {
        Some(stripped) => stripped.trim_start_matches('/').to_string(),
        None => path.to_string(),
    }
}

fn edge_row(root: &str, src_path: &str, src_name: Option<&str>, dst_path: &str, dst_name: Option<&str>) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        rel(root, src_path),
        src_name.unwrap_or(""),
        rel(root, dst_path),
        dst_name.unwrap_or(""),
    )
}

pub struct NormalForms {
    pub call: BTreeSet<String>,
    pub type_edges: BTreeSet<String>,
    pub module: BTreeSet<String>,
    /// call row -> the kind of the `resolved_edge` that produced it. Only the
    /// go projection reads it (`implements` marks the per-implementer fan-out
    /// edge); a row seen under several kinds keeps `implements` if it ever
    /// carried it.
    pub call_kinds: BTreeMap<String, String>,
}

/// `FlatFact` rows to the three tsv families, exactly as normalize.py's
/// `resolved_to_tsv` + `resolved_import_to_module_tsv` fold them: call rows
/// from `resolved_edge`, type rows from `resolved_type_edge`, module rows
/// from `resolved_import` with the names dropped.
pub fn normal_form(root: &Path, facts: &[FlatFact]) -> NormalForms {
    let root = root.to_str().unwrap_or_default();
    let mut forms = NormalForms {
        call: BTreeSet::new(),
        type_edges: BTreeSet::new(),
        module: BTreeSet::new(),
        call_kinds: BTreeMap::new(),
    };
    for fact in facts {
        match fact {
            FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                kind,
                ..
            } => {
                let row = edge_row(
                    root,
                    caller_path,
                    caller_name.as_deref(),
                    callee_path,
                    callee_name.as_deref(),
                );
                let fanout = kind == "implements";
                match forms.call_kinds.get(&row) {
                    Some(existing) if fanout && existing != "implements" => {
                        forms.call_kinds.insert(row.clone(), kind.clone());
                    }
                    None => {
                        forms.call_kinds.insert(row.clone(), kind.clone());
                    }
                    _ => {}
                }
                forms.call.insert(row);
            }
            FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name,
                target_path,
                target_name,
                ..
            } => {
                forms.type_edges.insert(edge_row(
                    root,
                    owner_path,
                    owner_name.as_deref(),
                    target_path,
                    target_name.as_deref(),
                ));
            }
            FlatFact::ResolvedImportRow {
                src_path, target_path, ..
            } => {
                forms
                    .module
                    .insert(format!("{}\t\t{}\t", rel(root, src_path), rel(root, target_path)));
            }
            _ => {}
        }
    }
    forms
}

// ── the go call projection (GO-PARITY.REPORT.md) ────────────────────────────

/// Which side of an interface call site a row answers. CodeQL names the
/// interface method (the spec row); vta names per-implementer rows (the
/// `implements` fan-out edges, go.rs). Ours emits both, so the projection
/// picks one per oracle. `Both` keeps every row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GoIface {
    Method,
    Impl,
}

/// The three scope flags of `plans/extract-bench-2026-08-29/go.project.py`,
/// one struct. The oracles never saw test files and are already in their own
/// iface shape, so this applies to OURS only.
#[derive(Clone, Default)]
pub struct GoProjection {
    /// Drop every row whose src_path is absent from the oracle's src_path set
    /// (test files, packages the oracle never built).
    pub scope_oracle: Option<BTreeSet<String>>,
    /// Drop `closure@<n>`-caller rows; the mirrored enclosing-fn row stays.
    pub closure: bool,
    pub iface: Option<GoIface>,
}

impl GoProjection {
    /// The projection ratchet rows use: `go.codeql2.call.tsv` scores in
    /// codeql shape, `go.oracle.call.vta.bare.tsv` in vta shape.
    pub fn per_oracle(oracle_file: &str, oracle_rows: &BTreeSet<String>) -> Option<GoProjection> {
        let oracle_srcs = || {
            oracle_rows
                .iter()
                .map(|row| row.split('\t').next().unwrap_or("").to_string())
                .collect::<BTreeSet<String>>()
        };
        match oracle_file {
            "go.codeql2.call.tsv" => Some(GoProjection {
                scope_oracle: Some(oracle_srcs()),
                closure: true,
                iface: Some(GoIface::Method),
            }),
            "go.oracle.call.vta.bare.tsv" => Some(GoProjection {
                scope_oracle: Some(oracle_srcs()),
                closure: true,
                iface: Some(GoIface::Impl),
            }),
            _ => None,
        }
    }
}

/// go.project.py over a call set, ported. `call_kinds` marks which rows are
/// `implements` fan-out edges.
pub fn go_project(
    call: &BTreeSet<String>,
    call_kinds: &BTreeMap<String, String>,
    projection: &GoProjection,
) -> BTreeSet<String> {
    let mut rows: BTreeSet<String> = call
        .iter()
        .filter(|row| {
            projection.scope_oracle.as_ref().is_none_or(|scope| {
                scope.contains(row.split('\t').next().unwrap_or(""))
            })
        })
        .filter(|row| {
            !projection.closure
                || !row
                    .split('\t')
                    .nth(1)
                    .unwrap_or("")
                    .starts_with("closure@")
        })
        .cloned()
        .collect();
    if projection.iface == Some(GoIface::Method) {
        // codeql shape: drop the per-implementer fan-out rows, keep the spec.
        rows.retain(|row| call_kinds.get(row).map(String::as_str) != Some("implements"));
    } else if projection.iface == Some(GoIface::Impl) {
        // vta shape: keep the fan-out rows; drop the spec row, detected as the
        // non-implements row whose (src_path, src_name, dst_name) triple also
        // occurs on an implements row.
        let impl_triples: BTreeSet<[String; 3]> = rows
            .iter()
            .filter(|row| call_kinds.get(*row).map(String::as_str) == Some("implements"))
            .map(|row| {
                let cols: Vec<&str> = row.split('\t').collect();
                [cols[0].to_string(), cols[1].to_string(), cols[3].to_string()]
            })
            .collect();
        rows.retain(|row| {
            if call_kinds.get(row).map(String::as_str) == Some("implements") {
                return true;
            }
            let cols: Vec<&str> = row.split('\t').collect();
            !impl_triples.contains(&[cols[0].to_string(), cols[1].to_string(), cols[3].to_string()])
        });
    }
    rows
}

#[test]
fn go_projection_drops_test_closure_and_iface_rows() {
    // 7 hand-made rows: 2 test-file callers, 1 closure caller, 1 implements
    // fan-out row, its spec row (same dst_name, the method name, as the
    // fan-out: that is how impl mode detects a spec), and 2 plain rows. Scope
    // is everything except the test files, so each iface mode must land on
    // exactly 3 rows: method mode keeps both plain rows plus the spec and
    // drops the fan-out; impl mode is the reverse.
    let plain = "internal/f.go\tHandler\tinternal/x.go\tX";
    let plain2 = "internal/f.go\tHandler\tinternal/x.go\tY";
    let spec = "internal/f.go\tHandler\tinternal/ast/ast.go\tWrite";
    let fanout = "internal/f.go\tHandler\tinternal/printer/textwriter.go\tWrite";
    let call: BTreeSet<String> = [
        plain,
        plain2,
        spec,
        fanout,
        "internal/f.go\tclosure@1\tinternal/x.go\tX",
        "internal/a_test.go\tCaller\tinternal/x.go\tX",
        "internal/b_test.go\tTestA\tinternal/x.go\tX",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let mut call_kinds = BTreeMap::new();
    call_kinds.insert(fanout.to_string(), "implements".to_string());
    call_kinds.insert(spec.to_string(), "name_resolve".to_string());
    let scope: BTreeSet<String> = [
        "internal/f.go",
        "internal/x.go",
        "internal/ast/ast.go",
        "internal/printer/textwriter.go",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let method = go_project(
        &call,
        &call_kinds,
        &GoProjection {
            scope_oracle: Some(scope.clone()),
            closure: true,
            iface: Some(GoIface::Method),
        },
    );
    assert_eq!(
        method,
        BTreeSet::from([plain.to_string(), plain2.to_string(), spec.to_string()]),
        "method mode: test, closure and fan-out rows drop; spec and plain stay"
    );

    let impl_mode = go_project(
        &call,
        &call_kinds,
        &GoProjection {
            scope_oracle: Some(scope),
            closure: true,
            iface: Some(GoIface::Impl),
        },
    );
    assert_eq!(
        impl_mode,
        BTreeSet::from([plain.to_string(), plain2.to_string(), fanout.to_string()]),
        "impl mode: test, closure and the spec row drop; fan-out and plain stay"
    );
}

// ── the rust call projection (RUST-PARITY.REPORT.md) ────────────────────────

/// The two live flags of `plans/extract-bench-2026-08-29/rust.project.py`.
/// The third flag, `--generic`, is inert on this corpus (no rust*.call.tsv
/// row carries a `<`) and has no port: it would be dead code here.
#[derive(Clone, Default)]
pub struct RustProjection {
    /// The corpus file list as root-relative paths; an oracle row whose
    /// dst_path is absent from it targets a file outside the corpus and
    /// cannot be hit.
    pub corpus_files: Option<BTreeSet<String>>,
    /// Drop a `closure@<n>` caller row when a mirrored enclosing-fn row
    /// exists: same src_path, dst_path and dst_name with a non-closure
    /// caller. The ra_ap_ide oracle has no closure rows; raw scip does.
    pub closure: bool,
}

impl RustProjection {
    /// Every rust call oracle scores under the full projection. The oracle's
    /// own rows drive the ours-side scope (a caller file the oracle never
    /// calls from cannot match).
    pub fn per_oracle(
        oracle_file: &str,
        corpus_files: &BTreeSet<String>,
    ) -> Option<RustProjection> {
        match oracle_file {
            "rust.oracle.call.tsv"
            | "rust.scip_override.call.tsv"
            | "rust.codeql.call.tsv" => Some(RustProjection {
                corpus_files: Some(corpus_files.clone()),
                closure: true,
            }),
            _ => None,
        }
    }
}

fn row_cols(row: &str) -> [&str; 4] {
    let mut cols = row.split('\t');
    [
        cols.next().unwrap_or(""),
        cols.next().unwrap_or(""),
        cols.next().unwrap_or(""),
        cols.next().unwrap_or(""),
    ]
}

/// rust.project.py over a call pair, ported. Returns both projected sets:
/// the oracle side drops rows whose dst_path is outside the corpus and
/// closure rows with a mirror; the ours side drops rows whose src_path the
/// (dst-scoped) oracle never calls from, then mirrors the closure leg.
pub fn rust_project(
    ours: &BTreeSet<String>,
    oracle: &BTreeSet<String>,
    projection: &RustProjection,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let corpus_files = projection.corpus_files.as_ref();
    let oracle_scoped: BTreeSet<String> = oracle
        .iter()
        .filter(|row| {
            corpus_files.is_none_or(|files| {
                files.contains(row.split('\t').nth(2).unwrap_or(""))
            })
        })
        .cloned()
        .collect();
    // Our side: the caller must be a file the (dst-scoped) oracle calls
    // from, the `--scope corpus` ours leg.
    let oracle_srcs: BTreeSet<&str> = oracle_scoped
        .iter()
        .map(|row| row.split('\t').next().unwrap_or(""))
        .collect();
    let ours_scoped: BTreeSet<String> = ours
        .iter()
        .filter(|row| oracle_srcs.contains(row.split('\t').next().unwrap_or("")))
        .cloned()
        .collect();
    if projection.closure {
        (
            closure_enclosing(&ours_scoped),
            closure_enclosing(&oracle_scoped),
        )
    } else {
        (ours_scoped, oracle_scoped)
    }
}

/// The `--closure enclosing` leg: a `closure@<n>` caller row drops when a
/// non-closure row shares its (src_path, dst_path, dst_name) triple.
fn closure_enclosing(rows: &BTreeSet<String>) -> BTreeSet<String> {
    let plain_triples: BTreeSet<[&str; 3]> = rows
        .iter()
        .filter_map(|row| {
            let c = row_cols(row);
            (!c[1].starts_with("closure@")).then_some([c[0], c[2], c[3]])
        })
        .collect();
    rows.iter()
        .filter(|row| {
            let c = row_cols(row);
            !c[1].starts_with("closure@") || !plain_triples.contains(&[c[0], c[2], c[3]])
        })
        .cloned()
        .collect()
}

#[test]
fn rust_projection_drops_out_of_corpus_and_mirrored_closure_rows() {
    // 6 hand rows: 1 oracle row whose dst is outside the corpus, 1 of ours
    // from a file the oracle never calls from, 1 closure row with its
    // enclosing mirror (drops), 1 closure row with no mirror (stays), and 2
    // plain rows that must survive on both sides.
    let plain = "crates/a/src/lib.rs\tf\tcrates/a/src/other.rs\tg";
    let plain2 = "crates/b/src/lib.rs\tf\tcrates/b/src/other.rs\tg";
    let outside = "crates/a/src/lib.rs\tf\tcrates/x/src/gen.rs\tg";
    let closure = "crates/a/src/lib.rs\tclosure@7\tcrates/a/src/other.rs\tg";
    let lone_closure = "crates/c/src/lib.rs\tclosure@9\tcrates/c/src/lib.rs\tinner";
    let ours_stray = "crates/z/src/tests.rs\tt\tcrates/a/src/other.rs\tg";
    let ours: BTreeSet<String> = [plain, plain2, closure, lone_closure, ours_stray]
        .into_iter()
        .map(String::from)
        .collect();
    let oracle: BTreeSet<String> = [plain, plain2, outside, closure]
        .into_iter()
        .map(String::from)
        .collect();
    let corpus_files: BTreeSet<String> = [
        "crates/a/src/lib.rs",
        "crates/a/src/other.rs",
        "crates/b/src/lib.rs",
        "crates/b/src/other.rs",
        "crates/c/src/lib.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    let (ours_p, oracle_p) = rust_project(
        &ours,
        &oracle,
        &RustProjection {
            corpus_files: Some(corpus_files),
            closure: true,
        },
    );
    assert_eq!(
        oracle_p,
        BTreeSet::from([plain.to_string(), plain2.to_string()]),
        "oracle: the out-of-corpus dst drops; the mirrored closure drops because its mirror is the plain row"
    );
    assert_eq!(
        ours_p,
        BTreeSet::from([plain.to_string(), plain2.to_string()]),
        "ours: the stray caller (a file the oracle never calls from) drops; the mirrored closure drops on the closure leg"
    );
    // The lone closure row drops on scope: its file is in the corpus, but
    // the oracle never calls from it.
    assert!(!ours_p.contains(lone_closure));
    assert!(!ours_p.contains(ours_stray));
}

pub fn family_rows<'a>(forms: &'a NormalForms, family: &str) -> &'a BTreeSet<String> {
    match family {
        "call" => &forms.call,
        "type" => &forms.type_edges,
        "module" => &forms.module,
        other => panic!("unknown oracle family '{other}'"),
    }
}

pub fn load_tsv(path: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

// ── scoring ─────────────────────────────────────────────────────────────────

pub struct Score {
    pub ours: usize,
    pub oracle: usize,
    pub overlap: usize,
    /// overlap / |oracle|, percent.
    pub recall: f64,
    /// overlap / |ours|, percent.
    pub precision: f64,
}

fn pct(overlap: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        overlap as f64 * 100.0 / total as f64
    }
}

pub fn score(ours: &BTreeSet<String>, oracle: &BTreeSet<String>) -> Score {
    let overlap = ours.intersection(oracle).count();
    Score {
        ours: ours.len(),
        oracle: oracle.len(),
        overlap,
        recall: pct(overlap, oracle.len()),
        precision: pct(overlap, ours.len()),
    }
}

// ── measurement ─────────────────────────────────────────────────────────────

pub struct Measurement {
    pub files: usize,
    /// Corpus files as root-relative paths, the enumeration that produced
    /// the rows; the rust projection scopes oracle rows against it.
    pub files_rel: BTreeSet<String>,
    /// Median of the 3 in-process runs.
    pub wall_ms: u128,
    /// Process-peak RSS after the runs (getrusage high-water mark).
    pub rss_mb: u64,
    pub forms: NormalForms,
}

fn request<'a>(files: &'a [PathBuf], corpus: &'a Corpus) -> ResolveRequest<'a> {
    // The diet_scip arms the CLI builds for `--family diet_scip` / `--resolve
    // --family call,type` (parse_arms in src/bin/extract.rs; diet_scip in
    // src/project.rs). ScipMode::Off is what makes the family diet.
    let checker = corpus.lang == "rust";
    ResolveRequest {
        paths: files,
        arms: ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: ScipMode::Off,
        // `project_root` stays None or a fresh cached index gets adopted and
        // the family is no longer diet. The checker carries its own root.
        project_root: None,
        scip_records: ScipRecords::all(),
        occurrence_text: false,
        rust_checker: checker.then_some(corpus.root.as_path()),
    }
}

/// 3 in-process runs, median wall, the last run's rows normalized. The rows
/// are a pure function of the file set, so any run's rows are the set; the
/// last one is kept so the earlier copies free before RSS is read.
pub fn measure(corpus: &Corpus) -> Measurement {
    let files = enumerate(corpus);
    assert!(
        files.len() >= 500,
        "ratchet {}: enumerated only {} files under {}; corpus rule broken?",
        corpus.lang,
        files.len(),
        corpus.root.display(),
    );
    println!(
        "ratchet {}: {} files under {}",
        corpus.lang,
        files.len(),
        corpus.root.display()
    );
    let mut walls = Vec::with_capacity(3);
    let mut facts = Vec::new();
    for run in 0..3 {
        let start = Instant::now();
        let out = resolve_project(&request(&files, corpus))
            .unwrap_or_else(|err| panic!("ratchet {}: resolve failed: {err}", corpus.lang));
        let wall_ms = start.elapsed().as_millis();
        assert!(
            wall_ms <= WALL_BUDGET_MS,
            "ratchet {} run {}: wall {wall_ms} ms over the {} ms per-call budget",
            corpus.lang,
            run + 1,
            WALL_BUDGET_MS,
        );
        println!("ratchet {}: run {} wall {wall_ms} ms", corpus.lang, run + 1);
        walls.push(wall_ms);
        facts = out;
    }
    walls.sort();
    Measurement {
        files: files.len(),
        files_rel: files
            .iter()
            .filter_map(|path| path.strip_prefix(&corpus.root).ok())
            .filter_map(|rel| rel.to_str().map(String::from))
            .collect(),
        wall_ms: walls[1],
        rss_mb: peak_rss_mb(),
        forms: normal_form(&corpus.root, &facts),
    }
}

/// `getrusage` high-water RSS in MB. ru_maxrss is bytes on darwin and
/// kilobytes on linux.
pub fn peak_rss_mb() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let code = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    assert_eq!(code, 0, "getrusage failed");
    let usage = unsafe { usage.assume_init() };
    #[cfg(target_os = "macos")]
    let bytes = usage.ru_maxrss as f64;
    #[cfg(not(target_os = "macos"))]
    let bytes = usage.ru_maxrss as f64 * 1024.0;
    (bytes / (1024.0 * 1024.0)).round() as u64
}

// ── RATCHET.tsv ─────────────────────────────────────────────────────────────

pub const RATCHET_HEADER: &str = "# extract ratchet: diet_scip (resolve call+types, 3 runs per corpus, median wall / process-peak rss) vs the committed oracle tsvs;\n\
     # recall = overlap/|oracle|, precision = overlap/|ours|, percent; check: 0.10 pt / wall +15% / rss +10% (ceilings at the worst of repeated runs); local-only (COMMON.md), never CI; bump: RATCHET_BUMP=1 improves floors/ceilings (walls/rss only by 10%+ margins), RATCHET_FORCE=1 rewrites.\n\
     lang\tfamily\toracle\trecall\tprecision\twall_ms\trss_mb\tmeasured_at_sha";

pub fn ratchet_path() -> PathBuf {
    Path::new(BENCH_DIR).join("RATCHET.tsv")
}

#[derive(Clone)]
pub struct RatchetRow {
    pub lang: String,
    pub family: String,
    pub oracle: String,
    pub recall: f64,
    pub precision: f64,
    pub wall_ms: u128,
    pub rss_mb: u64,
    pub sha: String,
}

/// The only subprocess in the ratchet: the receipt wants the measured-at sha,
/// and reading it via git is one call outside the measurement path.
pub fn measured_at_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn read_ratchet() -> Option<Vec<RatchetRow>> {
    let text = std::fs::read_to_string(ratchet_path()).ok()?;
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("lang\t") || line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        assert!(
            cols.len() == 8,
            "RATCHET.tsv row has {} columns, expected 8: {line}",
            cols.len()
        );
        rows.push(RatchetRow {
            lang: cols[0].to_string(),
            family: cols[1].to_string(),
            oracle: cols[2].to_string(),
            recall: cols[3].parse().unwrap_or_else(|_| panic!("RATCHET.tsv recall: {line}")),
            precision: cols[4].parse().unwrap_or_else(|_| panic!("RATCHET.tsv precision: {line}")),
            wall_ms: cols[5].parse().unwrap_or_else(|_| panic!("RATCHET.tsv wall_ms: {line}")),
            rss_mb: cols[6].parse().unwrap_or_else(|_| panic!("RATCHET.tsv rss_mb: {line}")),
            sha: cols[7].to_string(),
        });
    }
    Some(rows)
}

pub fn write_ratchet(rows: &[RatchetRow]) {
    let mut text = String::from(RATCHET_HEADER);
    text.push('\n');
    for row in rows {
        text.push_str(&format!(
            "{}\t{}\t{}\t{:.2}\t{:.2}\t{}\t{}\t{}\n",
            row.lang, row.family, row.oracle, row.recall, row.precision, row.wall_ms, row.rss_mb, row.sha
        ));
    }
    std::fs::write(ratchet_path(), text).expect("write RATCHET.tsv");
}

// ── the ratchet itself ──────────────────────────────────────────────────────

pub const RECALL_TOLERANCE_PT: f64 = 0.10;
pub const WALL_TOLERANCE_PCT: f64 = 15.0;
pub const RSS_TOLERANCE_PCT: f64 = 10.0;
/// A bump tightens a wall/rss ceiling only when the measurement is at least
/// this far below it, outside the run-to-run band.
pub const CEILING_TIGHTEN_MARGIN: f64 = 0.90;

/// One corpus: measure, print the table, then check (default) or bump
/// (`RATCHET_BUMP=1`). `RATCHET_FORCE=1` alongside bump rewrites every
/// measured row regardless of direction.
pub fn ratchet(lang: &str) {
    let corpus = corpus(lang);
    if !corpus.root.is_dir() {
        println!(
            "ratchet {}: absent (corpus root {} missing), skipped",
            corpus.lang,
            corpus.root.display()
        );
        return;
    }
    let measurement = measure(&corpus);
    let sha = measured_at_sha();
    let mut floors = read_ratchet().unwrap_or_default();
    let bump = std::env::var("RATCHET_BUMP").is_ok();
    let force = std::env::var("RATCHET_FORCE").is_ok();
    if floors.is_empty() && !bump {
        panic!(
            "ratchet: {} has no RATCHET.tsv rows; run once with RATCHET_BUMP=1 to plant the floors",
            corpus.lang
        );
    }

    println!(
        "\n{:<6} {:<8} {:<32} {:>7} {:>7} {:>7} {:>8} {:>9} {:>8} {:>7} verdict",
        "lang", "family", "oracle", "ours", "oracle", "overlap", "recall", "precision", "wall_ms", "rss_mb"
    );
    let mut failures = Vec::new();
    let mut improved = 0usize;
    let mut unchanged = 0usize;
    for (family, oracle_file) in corpus.oracles {
        let oracle_path = Path::new(BENCH_DIR).join(oracle_file);
        let row_key = |rows: &[RatchetRow]| {
            rows.iter()
                .position(|row| row.lang == corpus.lang && row.family == *family && row.oracle == *oracle_file)
        };
        if !oracle_path.is_file() {
            println!(
                "{:<6} {:<8} {:<32} absent ({} missing), skipped",
                corpus.lang, family, oracle_file, oracle_path.display()
            );
            continue;
        }
        let mut oracle_rows = load_tsv(&oracle_path);
        // The go call projection (GO-PARITY.REPORT.md): score our rows in the
        // oracle's own shape so recall and precision are comparable. The rust
        // call projection (RUST-PARITY.REPORT.md) scopes to the corpus file
        // list and mirrors closure rows on both sides. Every other family
        // and corpus scores raw.
        let mut ours = match GoProjection::per_oracle(oracle_file, &oracle_rows) {
            Some(projection) => go_project(
                family_rows(&measurement.forms, family),
                &measurement.forms.call_kinds,
                &projection,
            ),
            None => family_rows(&measurement.forms, family).clone(),
        };
        if let Some(projection) = RustProjection::per_oracle(oracle_file, &measurement.files_rel) {
            let (ours_projected, oracle_projected) =
                rust_project(&ours, &oracle_rows, &projection);
            ours = ours_projected;
            oracle_rows = oracle_projected;
        }
        let verdict = score(&ours, &oracle_rows);
        let floor = row_key(&floors).map(|index| floors[index].clone());
        let mut line_verdict = String::from("no-floor");
        if let Some(floor) = &floor {
            if verdict.recall < floor.recall - RECALL_TOLERANCE_PT {
                failures.push(format!(
                    "ratchet {} {} {}: recall {:.2} below floor {:.2} by {:.2} pt (tolerance {RECALL_TOLERANCE_PT})",
                    corpus.lang,
                    family,
                    oracle_file,
                    verdict.recall,
                    floor.recall,
                    floor.recall - verdict.recall
                ));
            }
            if verdict.precision < floor.precision - RECALL_TOLERANCE_PT {
                failures.push(format!(
                    "ratchet {} {} {}: precision {:.2} below floor {:.2} by {:.2} pt (tolerance {RECALL_TOLERANCE_PT})",
                    corpus.lang,
                    family,
                    oracle_file,
                    verdict.precision,
                    floor.precision,
                    floor.precision - verdict.precision
                ));
            }
            if measurement.wall_ms as f64 > floor.wall_ms as f64 * (1.0 + WALL_TOLERANCE_PCT / 100.0) {
                failures.push(format!(
                    "ratchet {} {} {}: wall {} ms above ceiling {} ms by {:.1}% (tolerance {WALL_TOLERANCE_PCT}%)",
                    corpus.lang,
                    family,
                    oracle_file,
                    measurement.wall_ms,
                    floor.wall_ms,
                    (measurement.wall_ms as f64 / floor.wall_ms as f64 - 1.0) * 100.0
                ));
            }
            if measurement.rss_mb as f64 > floor.rss_mb as f64 * (1.0 + RSS_TOLERANCE_PCT / 100.0) {
                failures.push(format!(
                    "ratchet {} {} {}: rss {} MB above ceiling {} MB by {:.1}% (tolerance {RSS_TOLERANCE_PCT}%)",
                    corpus.lang,
                    family,
                    oracle_file,
                    measurement.rss_mb,
                    floor.rss_mb,
                    (measurement.rss_mb as f64 / floor.rss_mb as f64 - 1.0) * 100.0
                ));
            }
            line_verdict = if failures.is_empty() { "ok" } else { "FAIL" }.to_string();
        }
        println!(
            "{:<6} {:<8} {:<32} {:>7} {:>7} {:>7} {:>8.2} {:>9.2} {:>8} {:>7} {}",
            corpus.lang,
            family,
            oracle_file,
            verdict.ours,
            verdict.oracle,
            verdict.overlap,
            verdict.recall,
            verdict.precision,
            measurement.wall_ms,
            measurement.rss_mb,
            line_verdict
        );

        if bump {
            let measured = RatchetRow {
                lang: corpus.lang.to_string(),
                family: family.to_string(),
                oracle: oracle_file.to_string(),
                recall: verdict.recall,
                precision: verdict.precision,
                wall_ms: measurement.wall_ms,
                rss_mb: measurement.rss_mb,
                sha: sha.clone(),
            };
            match row_key(&floors) {
                Some(index) => {
                    let floor = &floors[index];
                    // Wall and rss swing run to run (go rss 750-833 MB at one
                    // sha), so a bump tightens their ceilings only outside
                    // that noise band; otherwise every lucky run would drag
                    // the ceiling onto the optimistic end and the next normal
                    // run would go red. Recall/precision are stable to 0.01
                    // pt and move on any improvement.
                    let better = measured.recall > floor.recall
                        || measured.precision > floor.precision
                        || (measured.wall_ms as f64) < (floor.wall_ms as f64) * CEILING_TIGHTEN_MARGIN
                        || (measured.rss_mb as f64) < (floor.rss_mb as f64) * CEILING_TIGHTEN_MARGIN;
                    let worse = measured.recall < floor.recall
                        || measured.precision < floor.precision
                        || measured.wall_ms > floor.wall_ms
                        || measured.rss_mb > floor.rss_mb;
                    if better || (force && worse) {
                        println!(
                            "bump {} {} {}: recall {:.2}->{:.2} precision {:.2}->{:.2} wall {}->{} rss {}->{} ({})",
                            corpus.lang,
                            family,
                            oracle_file,
                            floor.recall, measured.recall,
                            floor.precision, measured.precision,
                            floor.wall_ms, measured.wall_ms,
                            floor.rss_mb, measured.rss_mb,
                            if force && worse { "forced" } else { "improved" }
                        );
                        floors[index] = measured;
                        improved += 1;
                    } else {
                        unchanged += 1;
                    }
                }
                None => {
                    println!(
                        "bump {} {} {}: new row (recall {:.2}, precision {:.2}, wall {}, rss {})",
                        corpus.lang, family, oracle_file, measured.recall, measured.precision, measured.wall_ms, measured.rss_mb
                    );
                    floors.push(measured);
                    improved += 1;
                }
            }
        }
    }
    if bump {
        floors.sort_by(|a, b| (&a.lang, &a.family, &a.oracle).cmp(&(&b.lang, &b.family, &b.oracle)));
        write_ratchet(&floors);
        println!(
            "ratchet {}: wrote {} ({improved} rows moved, {unchanged} held)",
            corpus.lang,
            ratchet_path().display()
        );
        return;
    }
    if !failures.is_empty() {
        for failure in &failures {
            println!("{failure}");
        }
        panic!(
            "ratchet {}: {} row(s) regressed against RATCHET.tsv",
            corpus.lang,
            failures.len()
        );
    }
    println!("ratchet {}: all rows hold", corpus.lang);
}
