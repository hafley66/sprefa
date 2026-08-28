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
    let output = move_output(fixture, extra);
    assert!(
        output.status.success(),
        "extract move {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn move_output(fixture: &Fixture, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg(fixture.root.join("lib/b.pl"))
        .arg(fixture.root.join("core/b.pl"))
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs")
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

// ── --verify: keep-if-pass, roll-back-if-fail ───────────────────────────────

/// rel -> bytes for every file under the root, so a rollback is judged
/// byte-identical, never end-state equality.
fn snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).expect("read file"));
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn verify_true_keeps_the_committed_move() {
    let fixture = fixture("verify_true");
    let output = move_output(&fixture, &["--commit", "--verify", "true"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("verify ok"), "{stdout}");
    assert!(!fixture.root.join("lib/b.pl").exists());
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("core/b.pl")).unwrap(),
        ":- module(b, [b_fact/1]).\n:- include('../lib/b_part.pl').\n"
    );
}

#[test]
fn verify_false_rolls_the_tree_back_byte_identical() {
    let fixture = helpers_fixture("verify_rollback");
    let before = snapshot(&fixture.root);

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg({
            let list = fixture.state.join("moves.tsv");
            std::fs::write(
                &list,
                HELPER_MOVES
                    .iter()
                    .map(|(old, new)| {
                        format!(
                            "{}\t{}\n",
                            fixture.root.join(old).display(),
                            fixture.root.join(new).display()
                        )
                    })
                    .collect::<String>(),
            )
            .unwrap();
            list
        })
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(["--commit", "--verify", "false"])
        .output()
        .expect("extract binary runs");

    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("verify failed (rc=1): rolled back"),
        "{stdout}"
    );
    let after = snapshot(&fixture.root);
    let differing: Vec<&String> = before
        .keys()
        .chain(after.keys())
        .filter(|rel| before.get(*rel) != after.get(*rel))
        .collect();
    assert!(
        differing.is_empty(),
        "tree not byte-identical after rollback: {differing:?}"
    );
    assert_eq!(before, after, "byte-identical tree after rollback");
    assert!(
        fixture.root.join("tests/helpers/one.mjs").is_file(),
        "the swept directory is back"
    );
    loads_clean_for_mjs(&fixture.root);
}

fn loads_clean_for_mjs(root: &Path) {
    assert!(root.join("tests/0_keeper.test.mjs").is_file());
}

/// SABOTAGE: without the both-flags check the dry run printed a plan and left,
/// and `touch marker` measured rc=0 with marker present on disk.
#[test]
fn verify_without_commit_is_an_error() {
    let fixture = fixture("verify_nocommit");
    let output = move_output(&fixture, &["--verify", "true"]);
    assert_eq!(output.status.code(), Some(2), "the flag pair errors");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--verify"), "{stderr}");
    assert!(stderr.contains("--commit"), "{stderr}");
}

#[test]
fn dry_run_never_runs_verify() {
    let fixture = fixture("verify_dryrun");
    let marker = fixture.root.join("marker");
    let output = move_output(&fixture, &["--verify", "touch marker"]);
    assert!(
        !output.status.success(),
        "a dry run refuses --verify: rc={}",
        output.status
    );
    assert!(!marker.exists(), "the dry run ran the command");
}

// ── --root repeatable: one MoveCx per root ──────────────────────────────────

const MULTI_ALPHA_A: &str =
    ":- module(a, [check/0]).\n:- use_module('lib/b').\n\ncheck :- b_fact(1).\n";
const MULTI_ALPHA_B: &str = ":- module(b, [b_fact/1]).\nb_fact(1).\n";
const MULTI_BETA_M: &str = ":- module(m, [go/0]).\n:- use_module('lib/c').\n\ngo :- c_fact(2).\n";
const MULTI_BETA_C: &str = ":- module(c, [c_fact/1]).\nc_fact(2).\n";

struct MultiFixture {
    base: PathBuf,
    alpha: PathBuf,
    beta: PathBuf,
    state: PathBuf,
}

/// Two sibling git roots, each a tiny prolog corpus, plus a rogue file outside
/// both for the under-no-root error.
fn multi_root_fixture(label: &str) -> MultiFixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let alpha = base.join("alpha");
    let beta = base.join("beta");
    for (root, files) in [
        (
            &alpha,
            [("a.pl", MULTI_ALPHA_A), ("lib/b.pl", MULTI_ALPHA_B)],
        ),
        (&beta, [("m.pl", MULTI_BETA_M), ("lib/c.pl", MULTI_BETA_C)]),
    ] {
        for (rel, body) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, body).unwrap();
        }
        git(&base, &["init", "-q", root.to_str().unwrap()]);
        git(
            root,
            &[
                "-c",
                "user.email=extract@move.test",
                "-c",
                "user.name=extract-move",
                "add",
                "-A",
            ],
        );
    }
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    MultiFixture {
        base,
        alpha,
        beta,
        state,
    }
}

fn multi_move_roots(
    fixture: &MultiFixture,
    roots: &[&Path],
    rows: &[(&Path, &str, &str)],
    extra: &[&str],
) -> std::process::Output {
    let list = fixture.state.join("moves.tsv");
    let body: String = rows
        .iter()
        .map(|(root, old, new)| {
            format!(
                "{}\t{}\n",
                root.join(old).display(),
                root.join(new).display()
            )
        })
        .collect();
    std::fs::write(&list, body).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_extract"));
    command.arg("move").arg("--list").arg(&list);
    for root in roots {
        command.arg("--root").arg(root);
    }
    command
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs")
}

fn stdout_lossy(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_lossy(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn two_roots_each_rewrite_their_own_importers() {
    let fixture = multi_root_fixture("multi_commit");
    let output = multi_move_roots(
        &fixture,
        &[&fixture.alpha, &fixture.beta],
        &[
            (&fixture.alpha, "lib/b.pl", "core/b.pl"),
            (&fixture.beta, "lib/c.pl", "core/c.pl"),
        ],
        &["--commit"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr_lossy(&output));
    let stdout = stdout_lossy(&output);
    assert!(
        stdout.contains("[root "),
        "multi-root output is prefixed:\n{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.alpha.join("a.pl")).unwrap(),
        ":- module(a, [check/0]).\n:- use_module('core/b').\n\ncheck :- b_fact(1).\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.beta.join("m.pl")).unwrap(),
        ":- module(m, [go/0]).\n:- use_module('core/c').\n\ngo :- c_fact(2).\n"
    );
    assert!(!fixture.alpha.join("lib/b.pl").exists());
    assert!(fixture.alpha.join("core/b.pl").is_file());
    assert!(!fixture.beta.join("lib/c.pl").exists());
    assert!(fixture.beta.join("core/c.pl").is_file());
}

/// A row under no `--root` is a named error before any stage runs; both roots
/// come out byte-identical.
#[test]
fn a_move_under_no_root_is_a_named_error_with_zero_edits() {
    let fixture = multi_root_fixture("multi_noroot");
    let rogue = fixture.base.join("rogue");
    std::fs::create_dir_all(&rogue).unwrap();
    std::fs::write(rogue.join("x.pl"), ":- module(x, []).\n").unwrap();
    let before_alpha = snapshot(&fixture.alpha);
    let before_beta = snapshot(&fixture.beta);

    let output = multi_move_roots(
        &fixture,
        &[&fixture.alpha, &fixture.beta],
        &[(&rogue, "x.pl", "y.pl")],
        &[],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr_lossy(&output));
    let stderr = stderr_lossy(&output);
    assert!(stderr.contains("is under none of the roots"), "{stderr}");
    assert!(stderr.contains("x.pl"), "{stderr}");
    assert_eq!(snapshot(&fixture.alpha), before_alpha, "zero edits");
    assert_eq!(snapshot(&fixture.beta), before_beta, "zero edits");
}

#[test]
fn verify_failure_rolls_back_every_root() {
    let fixture = multi_root_fixture("multi_rollback");
    let before_alpha = snapshot(&fixture.alpha);
    let before_beta = snapshot(&fixture.beta);

    let output = multi_move_roots(
        &fixture,
        &[&fixture.alpha, &fixture.beta],
        &[
            (&fixture.alpha, "lib/b.pl", "core/b.pl"),
            (&fixture.beta, "lib/c.pl", "core/c.pl"),
        ],
        &["--commit", "--verify", "false"],
    );
    assert_eq!(output.status.code(), Some(3), "{}", stderr_lossy(&output));
    let stdout = stdout_lossy(&output);
    assert!(
        stdout.contains("verify failed (rc=1): rolled back"),
        "{stdout}"
    );

    for (label, before, after) in [
        ("alpha", &before_alpha, &snapshot(&fixture.alpha)),
        ("beta", &before_beta, &snapshot(&fixture.beta)),
    ] {
        let differing: Vec<&String> = before
            .keys()
            .chain(after.keys())
            .filter(|rel| before.get(*rel) != after.get(*rel))
            .collect();
        assert!(
            differing.is_empty(),
            "{label} rolled back byte-identical: {differing:?}"
        );
    }
}

#[test]
fn no_root_flag_is_byte_identical_to_before() {
    let without = fixture("multi_norootflag_without");
    let with = fixture("multi_norootflag_with");

    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg(without.root.join("lib/b.pl"))
        .arg(without.root.join("core/b.pl"))
        .arg("--state")
        .arg(&without.state)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let derived = String::from_utf8(output.stdout).expect("stdout is UTF-8");

    assert_eq!(
        normalize(&derived, &without.root),
        normalize(&move_verb(&with, &[]), &with.root)
    );
}
