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
