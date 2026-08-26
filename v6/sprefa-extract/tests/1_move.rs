//! `extract move` over a temp git repo: the plan, the applied tree, the shim.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: `swipl -g halt -l a.pl` alone measured rc=0 against a deliberately
//! broken `use_module('lib/nope')`, so it judges nothing on its own. Every load
//! check below therefore ALSO runs `swipl -g check -t halt -l a.pl`, which
//! measured rc=2 on that same broken tree and rc=0 on the intact one.

use std::path::{Path, PathBuf};
use std::process::Command;

const A_PL: &str = ":- module(a, [check/0]).\n:- use_module('lib/b').\n\ncheck :- b_fact(1).\n";
const B_PL: &str = ":- module(b, [b_fact/1]).\n:- include('b_part.pl').\n";
const B_PART_PL: &str = "b_fact(1).\n";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str) -> Fixture {
    fixture_of(
        label,
        &[
            ("a.pl", A_PL),
            ("lib/b.pl", B_PL),
            ("lib/b_part.pl", B_PART_PL),
        ],
    )
}

fn fixture_of(label: &str, files: &[(&str, &str)]) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_{label}_{}_{}",
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

fn move_verb(fixture: &Fixture, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg(fixture.root.join("lib/b.pl"))
        .arg(fixture.root.join("core/b.pl"))
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

/// The batch door over rows named relative to the fixture root, so a test states
/// its moves the way the tree spells them.
fn move_list(fixture: &Fixture, rows: &[(&str, &str)], extra: &[&str]) -> String {
    let list = fixture.state.join("moves.tsv");
    let body: String = rows
        .iter()
        .map(|(old, new)| {
            format!(
                "{}\t{}\n",
                fixture.root.join(old).display(),
                fixture.root.join(new).display()
            )
        })
        .collect();
    std::fs::write(&list, body).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move --list {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn kind_count(table: &str, kind: &str) -> usize {
    table
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter(|line| line.split_whitespace().next() == Some(kind))
        .count()
}

fn swipl(root: &Path, args: &[&str]) -> i32 {
    Command::new("swipl")
        .args(args)
        .current_dir(root)
        .output()
        .expect("swipl runs")
        .status
        .code()
        .unwrap_or(-1)
}

fn loads_clean(root: &Path) {
    assert_eq!(swipl(root, &["-g", "halt", "-l", "a.pl"]), 0, "swipl load");
    assert_eq!(
        swipl(root, &["-g", "check", "-t", "halt", "-l", "a.pl"]),
        0,
        "swipl reaches b_fact/1 through the rewritten path"
    );
}

#[test]
fn dry_run_plans_one_move_and_two_replaces_and_writes_nothing() {
    let fixture = fixture("dry");
    let table = move_verb(&fixture, &[]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 2, "table:\n{table}");
    assert_eq!(kind_count(&table, "create"), 0, "table:\n{table}");
    assert!(
        table.contains("+:- use_module('core/b')."),
        "a.pl's import is re-aimed:\n{table}"
    );
    assert!(
        table.contains("+:- include('../lib/b_part.pl')."),
        "b.pl's include still resolves from core/:\n{table}"
    );

    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
    assert!(fixture.root.join("lib/b.pl").is_file());
    assert!(!fixture.root.join("core/b.pl").exists());
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("a.pl")).unwrap(),
        A_PL
    );
}

#[test]
fn commit_rewrites_the_importer_and_the_include_and_swipl_loads() {
    let fixture = fixture("commit");
    let table = move_verb(&fixture, &["--commit"]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(kind_count(&table, "replace"), 2, "table:\n{table}");
    assert!(!fixture.root.join("lib/b.pl").exists());
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("a.pl")).unwrap(),
        ":- module(a, [check/0]).\n:- use_module('core/b').\n\ncheck :- b_fact(1).\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("core/b.pl")).unwrap(),
        ":- module(b, [b_fact/1]).\n:- include('../lib/b_part.pl').\n"
    );
    loads_clean(&fixture.root);
}

#[test]
fn shim_leaves_a_reexport_behind_and_swipl_still_loads() {
    let fixture = fixture("shim");
    let table = move_verb(&fixture, &["--commit", "--shim"]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(kind_count(&table, "create"), 1, "table:\n{table}");
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("lib/b.pl")).unwrap(),
        ":- module(b_shim, []).\n:- reexport('../core/b').\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("a.pl")).unwrap(),
        A_PL,
        "a shim run leaves every importer alone"
    );
    loads_clean(&fixture.root);
}

// The specifier rule and the fact gate, on the three shapes the hand-built walk
// used to reach through the prolog extractor's specifier kinds.
const WIDE_A_PL: &str = ":- module(a, [check/0]).\n:- use_module('lib/b', [b_fact/1]).\n:- use_module(library(lists)).\n\ncheck :- b_fact(1).\n";
const WIDE_C_PL: &str = ":- module(c, []).\n:- use_module('lib/b').\n";
const WIDE_SUB_B_PL: &str = ":- module(sub_b, []).\n";

fn wide_fixture(label: &str) -> Fixture {
    fixture_of(
        label,
        &[
            ("a.pl", WIDE_A_PL),
            ("lib/b.pl", B_PL),
            ("lib/b_part.pl", B_PART_PL),
            ("sub/c.pl", WIDE_C_PL),
            ("sub/lib/b.pl", WIDE_SUB_B_PL),
        ],
    )
}

#[test]
fn the_two_argument_form_is_re_aimed_and_a_library_alias_is_left_alone() {
    let fixture = wide_fixture("twoarg");
    let table = move_verb(&fixture, &["--commit"]);

    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("a.pl")).unwrap(),
        ":- module(a, [check/0]).\n:- use_module('core/b', [b_fact/1]).\n:- use_module(library(lists)).\n\ncheck :- b_fact(1).\n",
        "the import list rides along and `library(lists)` names no file"
    );
}

/// The batch door and the positional door plan the same thing for one prolog
/// move: same previews, same diffs, same stage count.
#[test]
fn a_one_row_list_plans_what_the_positional_form_plans() {
    let positional = fixture("listone_positional");
    let batch = fixture("listone_batch");

    let list = batch.state.join("moves.tsv");
    std::fs::write(
        &list,
        format!(
            "# one move, through the batch door\n\n{}\t{}\n",
            batch.root.join("lib/b.pl").display(),
            batch.root.join("core/b.pl").display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&batch.root)
        .arg("--state")
        .arg(&batch.state)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move --list exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let batched = String::from_utf8(output.stdout).expect("stdout is UTF-8");

    assert_eq!(
        normalize(&move_verb(&positional, &[]), &positional.root),
        normalize(&batched, &batch.root)
    );
}

/// The fixture root and the stage ids are per run; everything else is the plan.
fn normalize(table: &str, root: &Path) -> String {
    table
        .replace(&root.display().to_string(), "<root>")
        .lines()
        .map(|line| match line.strip_prefix("stage ") {
            Some(rest) => format!(
                "stage <id>{}",
                &rest[rest.find(' ').unwrap_or(rest.len())..]
            ),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_same_spec_text_elsewhere_naming_another_file_is_left_alone() {
    let fixture = wide_fixture("samename");
    let table = move_verb(&fixture, &["--commit"]);

    // `sub/c.pl` writes the SAME raw spec, `'lib/b'`, and it resolves to
    // `sub/lib/b.pl`. The gate is per file, never per spelling.
    assert_eq!(kind_count(&table, "replace"), 2, "table:\n{table}");
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("sub/c.pl")).unwrap(),
        WIDE_C_PL
    );
    assert!(fixture.root.join("sub/lib/b.pl").is_file());
    loads_clean(&fixture.root);
}

// ── the emptied-directory sweep ─────────────────────────────────────────────

const HELPER_ONE_MJS: &str = "export const one = 1\n";
const HELPER_TWO_MJS: &str = "export const two = 2\n";
const KEEPER_MJS: &str = "export const keeper = 3\n";

/// The grapht `tests/helpers` shape: a nested directory whose whole contents
/// move to a sibling, with the parent still holding a file of its own.
fn helpers_fixture(label: &str) -> Fixture {
    fixture_of(
        label,
        &[
            ("tests/helpers/one.mjs", HELPER_ONE_MJS),
            ("tests/helpers/two.mjs", HELPER_TWO_MJS),
            ("tests/0_keeper.test.mjs", KEEPER_MJS),
        ],
    )
}

const HELPER_MOVES: [(&str, &str); 2] = [
    (
        "tests/helpers/one.mjs",
        "tests/3_integration/helpers/one.mjs",
    ),
    (
        "tests/helpers/two.mjs",
        "tests/3_integration/helpers/two.mjs",
    ),
];

#[test]
fn a_directory_this_run_empties_is_removed_and_its_still_full_parent_is_not() {
    let fixture = helpers_fixture("rmdir_commit");
    let table = move_list(&fixture, &HELPER_MOVES, &["--commit"]);

    assert_eq!(kind_count(&table, "rmdir"), 1, "table:\n{table}");
    assert!(
        table.contains("rmdir tests/helpers"),
        "the sweep names the directory it removed:\n{table}"
    );
    assert!(
        !fixture.root.join("tests/helpers").exists(),
        "the emptied directory is gone"
    );
    assert!(
        fixture.root.join("tests/0_keeper.test.mjs").is_file(),
        "the parent still holds a file of its own"
    );
    assert!(fixture.root.join("tests").is_dir(), "so the parent stays");
    assert!(fixture
        .root
        .join("tests/3_integration/helpers/one.mjs")
        .is_file());
}

#[test]
fn a_dry_run_names_the_directory_it_would_remove_and_leaves_it_there() {
    let fixture = helpers_fixture("rmdir_dry");
    let table = move_list(&fixture, &HELPER_MOVES, &[]);

    assert_eq!(kind_count(&table, "rmdir"), 1, "table:\n{table}");
    assert!(
        table.contains("rmdir tests/helpers dry run, tree untouched"),
        "the dry run names what it would remove:\n{table}"
    );
    assert!(
        fixture.root.join("tests/helpers/one.mjs").is_file(),
        "a dry run writes nothing"
    );
    assert_eq!(git(&fixture.root, &["status", "--porcelain"]), "");
}

// ── the relative path constants a moved TS file writes ──────────────────────

/// A fixture tree copied off `tests/fixtures/<rel>`, so a TS corpus states
/// itself as files rather than as string constants.
fn fixture_tree(label: &str, rel: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(rel),
        &root,
    );
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

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

const PATH_MOVES: [(&str, &str); 3] = [
    (
        "tests/0_moved.test.ts",
        "tests/3_integration/0_moved.test.ts",
    ),
    (
        "tests/1_helpers.test.ts",
        "tests/3_integration/1_helpers.test.ts",
    ),
    (
        "tests/helpers/one.mjs",
        "tests/3_integration/helpers/one.mjs",
    ),
];

#[test]
fn a_moved_file_re_aims_every_relative_path_constant_it_writes() {
    let fixture = fixture_tree("ts_paths", "ts_move/paths");
    move_list(&fixture, &PATH_MOVES, &["--commit"]);
    let moved = read(&fixture.root, "tests/3_integration/0_moved.test.ts");

    for (before, after) in [
        (
            "new URL(\"../results/4_fixture_memory.json\"",
            "new URL(\"../../results/4_fixture_memory.json\"",
        ),
        (
            "new URL(\"../fixtures/sequence/\"",
            "new URL(\"../../fixtures/sequence/\"",
        ),
        (
            "resolve(import.meta.dirname, \"../fixtures/sequence\")",
            "resolve(import.meta.dirname, \"../../fixtures/sequence\")",
        ),
        (
            "fileURLToPath(import.meta.url)), \"..\")",
            "fileURLToPath(import.meta.url)), \"../..\")",
        ),
        ("new URL(\"../bin\"", "new URL(\"../../bin\""),
    ] {
        assert!(moved.contains(after), "{before} becomes {after}:\n{moved}");
    }
    assert!(
        moved.contains("const greeting = \"hello\""),
        "a string under no path callee is left alone:\n{moved}"
    );
    assert!(
        moved.contains("readFile(\"./b\")"),
        "`readFile` is not a path builder:\n{moved}"
    );
    assert!(
        moved.contains("source(\"grapht\", \"./15_sequenceGeometry.ts\")"),
        "a user function's argument is not a path constant:\n{moved}"
    );
}

/// @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
/// SABOTAGE: the first build of the class took every `join` argument, and the
/// grapht trial measured `issue.path.join(".")` rewritten to `join("..")` in
/// `src/0_bench/0_protocol.ts` and `6_record.ts`. A separator is not a path.
#[test]
fn an_array_separator_is_not_a_path_segment() {
    let fixture = fixture_tree("ts_paths_separator", "ts_move/paths");
    move_list(&fixture, &PATH_MOVES, &["--commit"]);
    let moved = read(&fixture.root, "tests/3_integration/0_moved.test.ts");

    for kept in [
        "issue.path.join(\".\")",
        "segments.join(\"..\")",
        "[\"a\", \"b\"].join(\"./\")",
    ] {
        assert!(moved.contains(kept), "{kept} is left alone:\n{moved}");
    }
}

/// Depth-delta arithmetic gets this wrong. The literal names a directory that
/// moved WITH the file, so resolving through the moves map is what holds it.
#[test]
fn a_literal_naming_a_co_moving_directory_comes_out_byte_identical() {
    let fixture = fixture_tree("ts_paths_comove", "ts_move/paths");
    let before = read(&fixture.root, "tests/1_helpers.test.ts");
    move_list(&fixture, &PATH_MOVES, &["--commit"]);

    assert_eq!(
        read(&fixture.root, "tests/3_integration/1_helpers.test.ts"),
        before,
        "`./helpers` and `./helpers/one.mjs` still name the same files"
    );
}

#[test]
fn an_unmoved_file_writing_the_same_constructs_is_left_alone() {
    let fixture = fixture_tree("ts_paths_unmoved", "ts_move/paths");
    let before = read(&fixture.root, "tests/2_unmoved.test.ts");
    move_list(&fixture, &PATH_MOVES, &["--commit"]);

    assert_eq!(read(&fixture.root, "tests/2_unmoved.test.ts"), before);
}
