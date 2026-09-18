//! `sprefa-extract`, now living in `hafley-rs`, through its real binary into
//! `dl8 compile --tsi`. Three runs over `fixtures/extract/corpus`: the TSI
//! envelope, `--family diet_scip`, `--family scip`. The first run's JSONL is
//! the stream `dl8` loads, and the compiler rows it produces are compared
//! against `fixtures/extract/expected_tsi_rows.json` on the rows that name the
//! corpus.
//!
//! FAIL-FIRST RECEIPT: drop `--witness` from `tsi_stream` and the same run
//! prints `resolved_import` records and no `protocol` record, so
//! `tsi_envelope_carries_the_corpus_type_graph` fails at "the stream opens
//! with no protocol record" and `dl8_compile_consumes_the_extracted_stream`
//! finds no compiler row naming the corpus.
//!
//! `--family tsi` does not exist: `tsi` is the envelope `--witness` wraps a
//! run in (`crates/sprefa-extract/src/bin/extract/help.rs:193-212` lists the
//! family names; `extract.rs:207-216` is the flag), and the relation rows the
//! loader reads come from `--family type`. Several paths in one run need
//! `--resolve` (`extract.rs:645-660`).

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Over this, a build is a defect to report rather than a budget to keep.
const BUILD_BUDGET: Duration = Duration::from_secs(60);

fn v8_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn sprefa_root() -> PathBuf {
    v8_root()
        .parent()
        .expect("v8 has a parent directory")
        .to_path_buf()
}

fn fixture_root() -> PathBuf {
    v8_root().join("fixtures/extract")
}

/// The corpus paths in one order, so two runs name the same files. Relative to
/// the v8 root, because `resolved_import` records carry the path as given and
/// an absolute one would pin this checkout into the stream.
fn corpus_paths() -> Vec<PathBuf> {
    [
        "fixtures/extract/corpus/ts/records.ts",
        "fixtures/extract/corpus/ts/format.ts",
        "fixtures/extract/corpus/ts/report.ts",
        "fixtures/extract/corpus/rust/shapes.rs",
        "fixtures/extract/corpus/rust/report.rs",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("dl8-extract-{}-{name}", std::process::id()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("scratch parent");
    }
    path
}

/// `crates/sprefa-extract` resolves its own lockfile and is `exclude`d from the
/// hafley-rs workspace, so its manifest is the one to build, and `-p` against
/// the root manifest would not find the package.
/// Canonical: `hafley-rs` is a sibling symlink, and cargo walks the path as
/// given, so the uncanonicalized form lands in sprefa's workspace instead of
/// hafley-rs's and fails with "believes it's in a workspace when it's not".
fn crate_manifest() -> PathBuf {
    let linked = sprefa_root().join("hafley-rs/crates/sprefa-extract/Cargo.toml");
    std::fs::canonicalize(&linked).unwrap_or(linked)
}

/// Where a built `extract` can sit. `$CARGO_TARGET_DIR` comes first because a
/// lane sets it, and then neither target directory ever fills. The workspace
/// one is second: the crate was a member until hafley-rs excluded it.
fn built_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(shared) = std::env::var_os("CARGO_TARGET_DIR") {
        out.push(PathBuf::from(shared).join("debug/extract"));
    }
    out.push(sprefa_root().join("hafley-rs/target/debug/extract"));
    out.push(sprefa_root().join("hafley-rs/crates/sprefa-extract/target/debug/extract"));
    out
}

/// `$SPREFA_EXTRACT_BIN`, then a built binary, then one build. The sibling
/// link is gitignored, so a tree without it names that.
fn extract_bin() -> PathBuf {
    if let Some(named) = std::env::var_os("SPREFA_EXTRACT_BIN") {
        let path = PathBuf::from(named);
        assert!(
            path.exists(),
            "SPREFA_EXTRACT_BIN names {}, which does not exist",
            path.display()
        );
        return path;
    }
    if let Some(built) = built_candidates().into_iter().find(|path| path.exists()) {
        return built;
    }
    let manifest = crate_manifest();
    assert!(
        manifest.exists(),
        "no sprefa-extract crate at {}; link hafley-rs or set SPREFA_EXTRACT_BIN",
        manifest.display()
    );
    let start = Instant::now();
    let output = Command::new("cargo")
        .args([
            "build",
            "--features",
            "cli",
            "--bin",
            "extract",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cargo build");
    let elapsed = start.elapsed();
    assert!(
        output.status.success(),
        "building extract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < BUILD_BUDGET,
        "building extract took {} ms, over the {} ms budget",
        elapsed.as_millis(),
        BUILD_BUDGET.as_millis()
    );
    built_candidates()
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("cargo build left no extract in {:?}", built_candidates()))
}

struct Run {
    stdout: String,
    stderr: String,
    millis: u128,
}

fn extract_run(args: &[&str], paths: &[PathBuf]) -> Run {
    let mut command = Command::new(extract_bin());
    command.current_dir(v8_root()).args(args).args(paths);
    let start = Instant::now();
    let output = command.output().expect("extract runs");
    let millis = start.elapsed().as_millis();
    assert!(
        output.status.success(),
        "extract {args:?} exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    Run {
        stdout: String::from_utf8(output.stdout).expect("extract writes utf8"),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        millis,
    }
}

fn records(stream: &str) -> Vec<Value> {
    stream
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("extract writes one JSON object per line"))
        .collect()
}

fn record_kind(row: &Value, kind: &str) -> bool {
    row.get("record").and_then(Value::as_str) == Some(kind)
}

fn relation_of(row: &Value) -> Option<&str> {
    if !record_kind(row, "fact") {
        return None;
    }
    row.get("relation").and_then(Value::as_str)
}

/// The names the corpus declares. A compiler row carrying one of these came
/// from the extracted stream and not from the prelude.
const CORPUS_NAMES: [&str; 7] = [
    "UserRecord",
    "TeamRecord",
    "formatUser",
    "formatTeam",
    "reportTeam",
    "format_user",
    "format_team",
];

fn names_corpus(row: &Value) -> bool {
    let text = serde_json::to_string(row).expect("a compiler row re-encodes");
    CORPUS_NAMES
        .iter()
        .any(|name| text.contains(&format!("\"{name}\"")))
}

fn tsi_stream() -> (PathBuf, Run) {
    let run = extract_run(
        &["--witness", "--resolve", "--family", "type"],
        &corpus_paths(),
    );
    let path = scratch("corpus.tsi.jsonl");
    std::fs::write(&path, &run.stdout).expect("scratch stream");
    (path, run)
}

#[test]
fn tsi_envelope_carries_the_corpus_type_graph() {
    let (_, run) = tsi_stream();
    let rows = records(&run.stdout);
    assert!(
        rows.first().is_some_and(|row| record_kind(row, "protocol")),
        "the stream opens with no protocol record"
    );
    assert!(
        rows.iter().any(|row| record_kind(row, "run")),
        "the stream carries no run record, so the loader has no owner"
    );
    let facts = rows.iter().filter_map(relation_of).count();
    assert!(facts > 0, "--family type emitted no relation rows");
    for relation in ["tsi.type", "tsi.product", "tsi.edge", "tsi.name"] {
        assert!(
            rows.iter().any(|row| relation_of(row) == Some(relation)),
            "no {relation} row over the corpus"
        );
    }
    assert!(
        rows.iter()
            .filter(|row| relation_of(row) == Some("tsi.name"))
            .any(names_corpus),
        "no tsi.name row names a corpus type"
    );
    assert!(
        rows.iter().any(|row| record_kind(row, "witness")),
        "the envelope carries no witness"
    );
    println!("tsi: {facts} relation rows, {} ms", run.millis);
}

#[test]
fn diet_scip_resolves_across_corpus_files() {
    let run = extract_run(&["--family", "diet_scip"], &corpus_paths());
    let rows = records(&run.stdout);
    assert!(!rows.is_empty(), "--family diet_scip emitted no rows");
    for kind in ["resolved_edge", "resolved_type_edge", "resolved_import"] {
        assert!(
            rows.iter().any(|row| record_kind(row, kind)),
            "--family diet_scip emitted no {kind} row over the corpus: {:?}",
            rows.iter()
                .filter_map(|row| row.get("record"))
                .collect::<Vec<_>>()
        );
    }
    let cross_file = rows.iter().any(|row| {
        let ends = [
            ("caller_path", "callee_path"),
            ("owner_path", "target_path"),
            ("src_path", "target_path"),
        ];
        ends.iter().any(|(from, to)| {
            let from = row.get(from).and_then(Value::as_str);
            let to = row.get(to).and_then(Value::as_str);
            matches!((from, to), (Some(from), Some(to)) if from != to)
        })
    });
    assert!(
        cross_file,
        "every diet_scip edge stayed inside one file; the corpus imports across files"
    );
    println!("diet_scip: {} rows, {} ms", rows.len(), run.millis);
}

#[test]
fn scip_indexes_the_typescript_corpus() {
    if which("scip-typescript").is_none() {
        println!("ignored: scip-typescript is not on PATH, so --family scip has no indexer to run");
        return;
    }
    // The index is a build artifact; the fixture tree stays committable.
    let cache = scratch("scip-cache");
    std::fs::create_dir_all(&cache).expect("scip cache directory");
    let run = extract_run(
        &[
            "--family",
            "scip",
            "--scip-cache",
            cache.to_str().expect("utf8 cache path"),
        ],
        &[PathBuf::from("fixtures/extract/corpus/ts")],
    );
    let rows = records(&run.stdout);
    let skipped: Vec<&Value> = rows
        .iter()
        .filter(|row| record_kind(row, "scip_skip"))
        .collect();
    assert!(
        skipped.is_empty(),
        "--family scip skipped the corpus: {skipped:?}; stderr: {}",
        run.stderr
    );
    for kind in ["scip_def", "scip_ref", "scip_name"] {
        assert!(
            rows.iter().any(|row| record_kind(row, kind)),
            "no {kind} row over the TypeScript corpus"
        );
    }
    assert!(
        rows.iter()
            .filter(|row| record_kind(row, "scip_name"))
            .any(names_corpus),
        "no scip_name row names a corpus symbol"
    );
    println!("scip: {} rows, {} ms", rows.len(), run.millis);
}

#[test]
fn dl8_compile_consumes_the_extracted_stream() {
    let (stream, _) = tsi_stream();
    let program = fixture_root().join("main.dl7");
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(&program)
        .arg("--project")
        .arg(fixture_root())
        .arg("--tsi")
        .arg(&stream)
        .output()
        .expect("dl8 runs");
    assert!(
        output.status.success(),
        "dl8 compile --tsi exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled: Value =
        serde_json::from_slice(&output.stdout).expect("dl8 writes JSON on stdout");
    let diagnostics = compiled["diagnostics"]
        .as_array()
        .expect("diagnostics is an array");
    assert!(diagnostics.is_empty(), "dl8 reported {diagnostics:?}");
    let rows = compiled["compiler_rows"]
        .as_array()
        .expect("compiler_rows is an array");
    let named: Vec<Value> = rows
        .iter()
        .filter(|row| names_corpus(row))
        .cloned()
        .collect();
    assert!(
        !named.is_empty(),
        "no compiler row names a corpus type; the TSI stream did not reach the compiler"
    );
    let expected_path = fixture_root().join("expected_tsi_rows.json");
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&expected_path).expect("expected rows are committed"),
    )
    .expect("expected rows are JSON");
    assert_eq!(
        Value::Array(named.clone()),
        expected,
        "the rows naming the corpus differ from {}",
        expected_path.display()
    );
    println!(
        "dl8: {} compiler rows, {} name the corpus",
        rows.len(),
        named.len()
    );
}

/// `Command::new` on a missing program fails at spawn, so the scip case asks
/// first and says why it stood down.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}
