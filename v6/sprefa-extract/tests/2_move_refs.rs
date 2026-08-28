//! `extract move`'s manifest pass (arc 3) and `--text-refs` report (arc 4),
//! over `tests/fixtures/move_refs`: two moves, one package.json naming a
//! moved file's compiled image, a second package.json naming nothing moved,
//! and three text files (a bin script, a justfile, a markdown doc) each
//! naming an old path in one of the spellings the report scans for.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE (arc 3): `manifest_commit_rewrites_exact_bytes_and_leaves_pkg2_alone`
//! measured PASS -> FAIL with `write_manifest`'s trailing-newline guard
//! dropped: `left == right` failed, `right` (expected) carrying a trailing
//! `\n` the `left` (actual) output lacked.
//! SABOTAGE (arc 4): `text_refs_report_prints_golden_sorted_rows_and_touches_no_bytes`
//! measured PASS -> FAIL with `segment_pairs` proposing off `new_segments[0..]`
//! instead of `new_segments[dropped..]`: the PROTOCOL.md and justfile rows'
//! `proposed` side kept the dropped leading segments (`pkg/src/browser.ts`,
//! `pkg/tests/0_bench/1_scenarios.test.ts`) instead of matching the matched
//! side's own anchor depth.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_refs_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/move_refs");
    copy_tree(&source, &root);
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=extract@move.test",
            "-c",
            "user.name=extract-move",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        let dest = target.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &dest);
        } else {
            std::fs::copy(&path, &dest).expect("copy fixture file");
        }
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git stdout is UTF-8")
}

const MOVES: &str =
    "pkg/src/27_browser.ts\tpkg/src/browser.ts\npkg/tests/1_scenarios.test.ts\tpkg/tests/0_bench/1_scenarios.test.ts\n";

fn write_list(fixture: &Fixture) -> PathBuf {
    let path = fixture.state.join("moves.tsv");
    std::fs::write(&path, MOVES).unwrap();
    path
}

fn run_move(fixture: &Fixture, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(write_list(fixture))
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .current_dir(&fixture.root)
        .output()
        .expect("extract binary runs")
}

fn planned(fixture: &Fixture, extra: &[&str]) -> String {
    let output = run_move(fixture, extra);
    assert!(
        output.status.success(),
        "extract move {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn lines_with_prefix<'a>(output: &'a str, prefix: &str) -> Vec<&'a str> {
    output
        .lines()
        .filter(|line| line.starts_with(prefix))
        .collect()
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

const PKG2_ORIGINAL: &str = "{\n  \"name\": \"pkg2\",\n  \"main\": \"./dist/other.js\",\n  \"types\": \"./dist/other.d.ts\"\n}\n";

#[test]
fn manifest_dry_run_prints_receipt_and_writes_nothing() {
    let fixture = fixture("dry");
    let table = planned(&fixture, &[]);

    let receipts = lines_with_prefix(&table, "manifest ");
    assert_eq!(
        receipts,
        vec![
            "manifest pkg/package.json: exports[\"./browser\"].types ./dist/27_browser.d.ts -> ./dist/browser.d.ts",
            "manifest pkg/package.json: exports[\"./browser\"].import ./dist/27_browser.js -> ./dist/browser.js",
        ],
        "table:\n{table}"
    );
    assert_eq!(
        read(&fixture.root, "pkg/package.json"),
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/move_refs/pkg/package.json")
        )
        .unwrap(),
        "dry run must not touch the manifest"
    );
    assert_eq!(read(&fixture.root, "pkg2/package.json"), PKG2_ORIGINAL);
}

const PKG_REWRITTEN: &str = "{\n  \"name\": \"pkg\",\n  \"version\": \"0.1.0\",\n  \"main\": \"./dist/index.js\",\n  \"types\": \"./dist/index.d.ts\",\n  \"bin\": {\n    \"pkg-adapter\": \"./bin/pkg-adapter\"\n  },\n  \"exports\": {\n    \".\": {\n      \"types\": \"./dist/index.d.ts\",\n      \"import\": \"./dist/index.js\"\n    },\n    \"./browser\": {\n      \"types\": \"./dist/browser.d.ts\",\n      \"import\": \"./dist/browser.js\"\n    }\n  }\n}\n";

#[test]
fn manifest_commit_rewrites_exact_bytes_and_leaves_pkg2_alone() {
    let fixture = fixture("commit");
    let table = planned(&fixture, &["--commit"]);

    assert_eq!(
        lines_with_prefix(&table, "manifest ").len(),
        2,
        "table:\n{table}"
    );
    assert_eq!(read(&fixture.root, "pkg/package.json"), PKG_REWRITTEN);
    assert_eq!(
        read(&fixture.root, "pkg2/package.json"),
        PKG2_ORIGINAL,
        "a package.json naming no moved file is never opened for writing"
    );
}

#[test]
fn text_refs_absent_prints_nothing() {
    let fixture = fixture("absent");
    let table = planned(&fixture, &[]);

    assert!(
        lines_with_prefix(&table, "text-ref ").is_empty(),
        "table:\n{table}"
    );
}

#[test]
fn text_refs_report_prints_golden_sorted_rows_and_touches_no_bytes() {
    let fixture = fixture("report");
    let table = planned(&fixture, &["--text-refs"]);

    let rows = lines_with_prefix(&table, "text-ref ");
    assert_eq!(
        rows,
        vec![
            "text-ref pkg/PROTOCOL.md:3 src/27_browser.ts -> src/browser.ts",
            "text-ref pkg/bin/pkg-adapter:2 ../dist/27_browser.js -> ../dist/browser.js",
            "text-ref pkg/justfile:2 tests/1_scenarios.test.ts -> tests/0_bench/1_scenarios.test.ts",
        ],
        "table:\n{table}"
    );

    for rel in [
        "pkg/PROTOCOL.md",
        "pkg/bin/pkg-adapter",
        "pkg/justfile",
        "pkg/README.md",
        "pkg/package.json",
        "pkg2/package.json",
    ] {
        assert_eq!(
            read(&fixture.root, rel),
            std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/move_refs")
                    .join(rel)
            )
            .unwrap(),
            "{rel} must be byte-identical after a report-only run"
        );
    }
}
