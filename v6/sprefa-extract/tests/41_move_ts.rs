//! `extract move` on the TS arm, over `tests/fixtures/ts_move`: one moved file,
//! five importers spelling it five ways, and the moved file's own re-aims.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: asserting only the moved file's arrival measured green against a
//! build whose `ts_replacement` returned `None` for every alias, so the alias
//! row is asserted by its exact text AND re-resolved through `TsResolver` on the
//! committed tree, which measured a failure on that same build.

use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_extract::{ts_specifiers, TsResolver};

const OLD: &str = "src/entry/index.ts";
const NEW: &str = "src/deep/entry/index.ts";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_ts_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_move");
    copy_tree(&source, &root);
    std::fs::rename(root.join("vendor"), root.join("node_modules")).expect("stage node_modules");
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
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
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

fn move_verb(fixture: &Fixture, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg(fixture.root.join(OLD))
        .arg(fixture.root.join(NEW))
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

fn kind_count(table: &str, kind: &str) -> usize {
    table
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter(|line| line.split_whitespace().next() == Some(kind))
        .count()
}

/// The spelling each importer writes after the move, one row per style.
const REWRITTEN: [(&str, &str); 5] = [
    ("src/importers/relative.ts", "'../deep/entry/index.ts'"),
    ("src/importers/extensionless.ts", "'../deep/entry/index'"),
    ("src/importers/directory.ts", "'../deep/entry'"),
    ("src/importers/emitted.ts", "'../deep/entry/index.js'"),
    ("src/importers/alias.ts", "'@app/deep/entry'"),
];

#[test]
fn every_importer_style_is_rewritten_and_still_resolves_to_the_moved_file() {
    let fixture = fixture("styles");
    let table = move_verb(&fixture, &["--commit"]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 6, "table:\n{table}");
    assert!(!fixture.root.join(OLD).exists());

    for (rel, spec) in REWRITTEN {
        let body = read(&fixture.root, rel);
        assert!(body.contains(spec), "{rel} writes {spec}:\n{body}");
    }

    let resolver = TsResolver::new(&fixture.root).expect("build resolver");
    let arrived = fixture.root.join(NEW).canonicalize().expect("moved file");
    for (rel, _) in REWRITTEN {
        let from = fixture.root.join(rel);
        let body = read(&fixture.root, rel);
        let rows = ts_specifiers(rel, &body).expect("importer parses");
        assert_eq!(rows.len(), 1, "{rel} writes one spec");
        assert_eq!(
            resolver.resolve(&from, &rows[0].module),
            Some(arrived.clone()),
            "{rel} still reaches the moved file through {}",
            rows[0].module
        );
    }
}

#[test]
fn the_moved_file_re_aims_its_relative_import_and_leaves_the_package_alone() {
    let fixture = fixture("selfaim");
    move_verb(&fixture, &["--commit"]);

    let body = read(&fixture.root, NEW);
    assert!(
        body.contains("from '../../b'"),
        "the sibling import climbs out of the new directory:\n{body}"
    );
    assert!(
        body.contains("from 'pkg-exports'"),
        "a package name is anchored to the root, not the importer:\n{body}"
    );

    let resolver = TsResolver::new(&fixture.root).expect("build resolver");
    let from = fixture.root.join(NEW);
    let expected = fixture.root.join("src/b.ts").canonicalize().unwrap();
    assert_eq!(resolver.resolve(&from, "../../b"), Some(expected));
}

#[test]
fn a_dry_run_prints_the_plan_and_writes_nothing() {
    let fixture = fixture("dry");
    let before = read(&fixture.root, "src/importers/alias.ts");
    let table = move_verb(&fixture, &[]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 6, "table:\n{table}");
    assert!(
        table.contains("+import { entry } from '@app/deep/entry';"),
        "the alias preview is in the plan:\n{table}"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    assert!(fixture.root.join(OLD).is_file());
    assert!(!fixture.root.join(NEW).exists());
    assert_eq!(read(&fixture.root, "src/importers/alias.ts"), before);
}

/// `src/index.ts` writes eleven specs and none of them names the moved file, so
/// the whole file is left alone: the gate is per target, never per spelling.
#[test]
fn a_file_naming_no_moved_target_is_untouched() {
    let fixture = fixture("untouched");
    let before = read(&fixture.root, "src/index.ts");
    move_verb(&fixture, &["--commit"]);
    assert_eq!(read(&fixture.root, "src/index.ts"), before);
}
