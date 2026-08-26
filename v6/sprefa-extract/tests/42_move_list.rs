//! `extract move --list <tsv>`: many moves in one plan, one Replace per
//! importer however many moves touch it, and the guard that refuses a Replace
//! whose file changed between plan and commit.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: `a_replace_whose_file_changed_under_it_is_refused` measured PASS
//! -> FAIL with `bind_action` re-reading `expected` off disk (the shape before
//! this arc): the stale stage was accepted and the edits would have landed at
//! offsets the file no longer had.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str, files: &[(&str, &str)]) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_list_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    for (rel, body) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }
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

fn write_list(fixture: &Fixture, body: &str) -> PathBuf {
    let path = fixture.state.join("moves.tsv");
    std::fs::write(&path, body).unwrap();
    path
}

fn move_list(fixture: &Fixture, list: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .current_dir(&fixture.root)
        .output()
        .expect("extract binary runs")
}

fn planned(fixture: &Fixture, list: &Path, extra: &[&str]) -> String {
    let output = move_list(fixture, list, extra);
    assert!(
        output.status.success(),
        "extract move --list {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn refused(fixture: &Fixture, list: &Path) -> String {
    let output = move_list(fixture, list, &[]);
    assert!(!output.status.success(), "the batch was accepted");
    String::from_utf8(output.stderr).expect("stderr is UTF-8")
}

fn kind_count(table: &str, kind: &str) -> usize {
    table
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter(|line| line.split_whitespace().next() == Some(kind))
        .count()
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

const HUB_TS: &str = "import { a } from './a';\nimport { b } from './b';\nimport { c } from './c';\n\nexport const hub = [a, b, c];\n";
const ONLY_A_TS: &str = "import { a } from './a';\n\nexport const onlyA = a;\n";

fn three_move_fixture(label: &str) -> Fixture {
    fixture(
        label,
        &[
            ("src/a.ts", "export const a = 'a';\n"),
            ("src/b.ts", "export const b = 'b';\n"),
            ("src/c.ts", "export const c = 'c';\n"),
            ("src/hub.ts", HUB_TS),
            ("src/onlyA.ts", ONLY_A_TS),
        ],
    )
}

const THREE_MOVES: &str = "# three files leave src/ for lib/\n\nsrc/a.ts\tlib/a.ts\nsrc/b.ts\tlib/b.ts\nsrc/c.ts\tlib/c.ts\n";

#[test]
fn three_moves_touching_one_importer_are_three_edits_in_one_replace() {
    let fixture = three_move_fixture("shared");
    let list = write_list(&fixture, THREE_MOVES);
    let table = planned(&fixture, &list, &["--commit"]);

    assert_eq!(kind_count(&table, "move"), 3, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 2, "table:\n{table}");
    // Two stages, whatever the move count: every Replace in the first, every
    // Move in the second (soopy takes one operation per source file).
    assert_eq!(table.matches("committed").count(), 2, "table:\n{table}");

    assert_eq!(
        read(&fixture.root, "src/hub.ts"),
        "import { a } from '../lib/a';\nimport { b } from '../lib/b';\nimport { c } from '../lib/c';\n\nexport const hub = [a, b, c];\n"
    );
    assert_eq!(
        read(&fixture.root, "src/onlyA.ts"),
        "import { a } from '../lib/a';\n\nexport const onlyA = a;\n"
    );
    for rel in ["lib/a.ts", "lib/b.ts", "lib/c.ts"] {
        assert!(fixture.root.join(rel).is_file(), "{rel} arrived");
    }
    assert!(!fixture.root.join("src/a.ts").exists());
}

#[test]
fn a_batch_dry_run_prints_one_plan_and_writes_nothing() {
    let fixture = three_move_fixture("dry");
    let list = write_list(&fixture, THREE_MOVES);
    let table = planned(&fixture, &list, &[]);

    assert_eq!(kind_count(&table, "move"), 3, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 2, "table:\n{table}");
    assert_eq!(table.matches("dry run, tree untouched").count(), 2);
    assert_eq!(
        table.lines().filter(|line| line.starts_with("plan ")).count(),
        3,
        "one plan line per move:\n{table}"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    assert_eq!(read(&fixture.root, "src/hub.ts"), HUB_TS);
}

const MIXED_A_PL: &str = ":- module(a, [check/0]).\n:- use_module('lib/b').\n\ncheck :- true.\n";

#[test]
fn a_mixed_list_picks_an_arm_per_row_by_extension() {
    let fixture = fixture(
        "mixed",
        &[
            ("a.pl", MIXED_A_PL),
            ("lib/b.pl", ":- module(b, []).\n"),
            ("src/x.ts", "export const x = 'x';\n"),
            ("src/user.ts", "import { x } from './x';\n\nexport const user = x;\n"),
        ],
    );
    let list = write_list(&fixture, "lib/b.pl\tcore/b.pl\nsrc/x.ts\tsrc/deep/x.ts\n");
    let table = planned(&fixture, &list, &["--commit"]);

    assert_eq!(kind_count(&table, "move"), 2, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 2, "table:\n{table}");
    assert_eq!(
        read(&fixture.root, "a.pl"),
        ":- module(a, [check/0]).\n:- use_module('core/b').\n\ncheck :- true.\n"
    );
    assert_eq!(
        read(&fixture.root, "src/user.ts"),
        "import { x } from './deep/x';\n\nexport const user = x;\n"
    );
}

/// A row aiming at a file another row is vacating is refused too: validation
/// runs before any stage, so that file is still on disk when it is checked.
#[test]
fn a_destination_that_another_row_still_reads_from_ends_the_run() {
    let fixture = three_move_fixture("collide");
    let list = write_list(&fixture, "src/a.ts\tsrc/b2.ts\nsrc/b.ts\tsrc/a.ts\n");
    let message = refused(&fixture, &list);

    assert!(
        message.contains("move destination already exists")
            && message.ends_with("src/a.ts\n"),
        "stderr:\n{message}"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
}

#[test]
fn two_rows_writing_one_destination_end_the_run() {
    let fixture = three_move_fixture("dupdest");
    let list = write_list(&fixture, "src/a.ts\tlib/x.ts\nsrc/b.ts\tlib/x.ts\n");
    let message = refused(&fixture, &list);

    assert!(
        message.contains("destination of two moves"),
        "stderr:\n{message}"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
}

#[test]
fn a_row_with_no_tab_ends_the_run_instead_of_being_skipped() {
    let fixture = three_move_fixture("notab");
    let list = write_list(&fixture, "src/a.ts lib/a.ts\n");
    let message = refused(&fixture, &list);

    assert!(message.contains(":1: a move list row is"), "stderr:\n{message}");
}

#[test]
fn the_list_and_the_positional_form_are_exclusive() {
    let fixture = three_move_fixture("both");
    let list = write_list(&fixture, THREE_MOVES);
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg(fixture.root.join("src/a.ts"))
        .arg(fixture.root.join("lib/a.ts"))
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .output()
        .expect("extract binary runs");

    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(message.contains("--list carries the moves"), "stderr:\n{message}");
}

/// The guard `bind_action` carries: a Replace states the bytes its offsets were
/// cut against, so an edit landing under it between plan and stage is refused.
#[test]
fn a_replace_whose_file_changed_under_it_is_refused() {
    let fixture = fixture("stale", &[("one.ts", "export const one = 1;\n")]);
    let root = &fixture.root;
    let identity = soopy::SourceRoot::open_directory(root)
        .expect("open root")
        .directory()
        .identity
        .clone();
    let source = sprefa_extract::directory_source(&identity, "one.ts");
    let planned = soopy::ContentId::blake3(b"export const one = 1;\n");
    let edit = soopy::TextEdit {
        range: soopy::ActionSpan {
            source: source.clone(),
            start: 19,
            end: 20,
        },
        replacement: b"2".to_vec(),
        producer: soopy::ActionProducer::unordered("extract-move-test"),
    };
    let action = sprefa_extract::replace_action(source, planned, vec![edit]);

    assert!(
        stage(root, &identity, &action).is_ok(),
        "the plan stages while the file still holds the bytes it was cut against"
    );
    std::fs::write(root.join("one.ts"), "export const one = 12345;\n").unwrap();
    assert!(
        stage(root, &identity, &action).is_err(),
        "a Replace bound against stale bytes has to be refused"
    );
}

fn stage(
    root: &Path,
    identity: &soopy::DirectoryId,
    action: &soopy::SourceAction,
) -> Result<(), String> {
    let mut source_root = soopy::SourceRoot::open_directory(root).map_err(|e| e.to_string())?;
    let bound = sprefa_extract::bind_action(root, identity, action)?;
    let request = soopy::StageRequest::new(
        soopy::SourceRootId::Directory {
            directory: identity.clone(),
        },
        vec![bound],
    );
    let mut store = soopy::InMemoryStageStore::new();
    soopy::stage_mutations(&mut source_root, &request, &mut store)
        .map(|_| ())
        .map_err(|refusal| refusal.to_string())
}
