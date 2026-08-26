//! What `extract move` is allowed to touch, counted: files the prescan gate
//! admits, rule scans per run, files the drain stages. Counts only, never a
//! wall clock, so the test says the same thing on a loaded machine.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: `carries_specifier` with the stem test dropped (the directive
//! needles alone, the pre-name-gate shape) measured 2 failed / 2 passed with
//! `parsed=4 skipped=2` against the asserted 2 and 4.
//! SABOTAGE: the drain run over the prescan's parsed set instead of the store's
//! named set measured 1 failed / 3 passed, 2 `move drain` lines against the 1
//! asserted, so `rule_scans_are_the_gated_files_plus_the_named_files` is what
//! pins the fact matcher as the thing that picks the drained files.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_extract");

/// Six prolog files, three gates. `a.pl` names the moved file, `b.pl` and
/// `d.pl` carry a directive naming something else, `e.pl` carries the moved
/// stem in a comment with no directive at all.
const FILES: [(&str, &str); 6] = [
    ("a.pl", ":- module(a, []).\n:- use_module('lib/target').\n"),
    ("b.pl", ":- module(b, []).\n:- use_module('lib/other').\n"),
    ("d.pl", ":- module(d, []).\n:- ensure_loaded('lib/other').\n"),
    ("e.pl", "% target lives in lib/target.pl\nnothing_here.\n"),
    ("lib/target.pl", ":- module(target, []).\n"),
    ("lib/other.pl", ":- module(other, []).\n"),
];

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_perf_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    for (rel, body) in FILES {
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

fn git(root: &Path, args: &[&str]) {
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
}

/// One dry run's debug trail. The move's spans carry the counts; the tree is
/// untouched either way.
fn spans(fixture: &Fixture) -> String {
    let output = Command::new(BIN)
        .arg("move")
        .arg(fixture.root.join("lib/target.pl"))
        .arg(fixture.root.join("core/target.pl"))
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .env("RUST_LOG", "extract=debug")
        .env_remove("DL_TRACE_SUMMARY")
        .env_remove("HAFLEY_LOG_FORMAT")
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stderr).expect("the debug trail is UTF-8")
}

/// The `key=value` a span line carries, as a number.
fn field(trail: &str, message: &str, key: &str) -> usize {
    let line = trail
        .lines()
        .find(|line| line.contains(message))
        .unwrap_or_else(|| panic!("no `{message}` line in:\n{trail}"));
    let token = line
        .split_whitespace()
        .find(|token| token.starts_with(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no `{key}=` on `{line}`"));
    token
        .trim_start_matches(&format!("{key}="))
        .parse()
        .unwrap_or_else(|_| panic!("`{token}` is not a count"))
}

#[test]
fn the_name_gate_admits_only_the_files_carrying_the_moved_stem() {
    let trail = spans(&fixture("gate"));
    assert_eq!(field(&trail, "move prescan", "corpus"), 6, "{trail}");
    assert_eq!(
        field(&trail, "move prescan", "parsed"),
        2,
        "`a.pl` names the moved file and `lib/target.pl` is the moved file: {trail}"
    );
    assert_eq!(
        field(&trail, "move prescan", "skipped"),
        4,
        "a directive naming another file, and the stem in a comment, buy no parse: {trail}"
    );
}

#[test]
fn rule_scans_are_the_gated_files_plus_the_named_files() {
    let trail = spans(&fixture("scans"));
    let gated = field(&trail, "move prescan", "parsed");
    let drained = trail
        .lines()
        .filter(|line| line.contains("move drain rel="))
        .count();
    assert_eq!(
        drained, 1,
        "the store names `a.pl` alone, so one file is scanned again: {trail}"
    );
    assert_eq!(
        gated + drained,
        3,
        "2 prescan scans + 1 fact-gated scan, never one per corpus file: {trail}"
    );
}

#[test]
fn every_named_file_stages_exactly_one_replace() {
    let trail = spans(&fixture("staged"));
    assert_eq!(field(&trail, "move drain done", "files"), 1, "{trail}");
    assert_eq!(
        field(&trail, "move drain done", "staged"),
        1,
        "one soopy Replace per named file, edits folded into it: {trail}"
    );
}

/// The rule's own count, off the committed file: argument ONE of the five
/// directives, and nothing else in the same call.
#[test]
fn the_bounded_rule_matches_argument_one_and_no_other_atom() {
    const RULE: &str = include_str!("../rules/move_specifier.yml");
    const SRC: &str = ":- module(a, []).\n:- use_module('lib/b').\n:- use_module('lib/c', [c/1]).\n:- use_module(library(lists)).\n:- include('parts/d.pl').\n:- ensure_loaded(plain).\n:- reexport('lib/e').\n:- consult('lib/f').\n:- other_call('lib/g').\n";

    let rule: ast_grep_config::RuleConfig<sprefa_extract::ExtractLang> =
        ast_grep_config::from_yaml_string(
            &format!("language: prolog\n{RULE}"),
            &ast_grep_config::GlobalRules::default(),
        )
        .expect("the committed rule decodes")
        .into_iter()
        .next()
        .expect("one rule");

    let root = ast_grep_core::AstGrep::new(SRC, sprefa_extract::ExtractLang::Prolog);
    let found: Vec<String> = root
        .root()
        .find_all(&rule.matcher)
        .map(|matched| matched.text().to_string())
        .collect();
    assert_eq!(
        found,
        vec![
            "'lib/b'",
            "'lib/c'",
            "'parts/d.pl'",
            "plain",
            "'lib/e'",
            "'lib/f'",
        ],
        "`library(lists)` names no file, `[c/1]` is argument two, `other_call` is not a load directive"
    );
}
