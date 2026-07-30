//! Deprecated-alias coverage for the 2026-07-20 rename: `sg` -> `match_ast`,
//! `match` -> `match_line`. Both old names are kept as deprecated aliases
//! (never a hard break: this repo alone has 79+65 pre-rename call sites, plus
//! user-side `.dl` files this repo cannot see), and BOTH warn symmetrically —
//! `sg` is not "the good one that just gets a longer name"; it is equally
//! deprecated as a spelling. One mechanism (`BodyItem::{Match,Sg}.legacy_name`,
//! set in `parse/mod.rs`'s `body_item` dispatch, read in
//! `typecheck::normalize_body_item`) handles both entries, not a rename plus a
//! separate bolted-on deprecation.
//!
//! Each test below: the OLD name parses and runs, produces rows IDENTICAL to
//! the NEW name run against the same corpus, and emits the `deprecated-op-name`
//! warning naming the replacement. The canonical spelling stays warning-free.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("deprecated_op_names_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/x.rs"),
        "fn alpha_target() {}\nfn beta_other() {}\n",
    )
    .unwrap();
    dir
}

fn run(dir: &Path, prog: &str, db_name: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let db = dir.join(db_name);
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--no-daemon", "--db", db.to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Sorted line set, so a comparison is content-equal without depending on any
/// row-emission order the fixpoint happens not to guarantee.
fn sorted_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort_unstable();
    lines
}

/// `match(...)` (the pre-rename spelling) still parses and runs, produces rows
/// identical to `match_line(...)` over the same corpus, and warns naming
/// match_line — pointing at match_ast in turn for structured source code.
#[test]
fn legacy_match_runs_identically_to_match_line_and_warns() {
    let d = sandbox("match");
    let prog_old = concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "match(p, rev, /fn (?<name>[a-z_]+)/, l).\n",
        "? hit(p, l).\n",
    );
    let prog_new = prog_old.replace("match(", "match_line(");
    assert!(
        prog_new.contains("match_line("),
        "sanity: replace actually fired"
    );

    let (code_old, out_old, err_old) = run(&d, prog_old, "old.db");
    assert_eq!(code_old, 0, "legacy `match` still runs: {err_old}");
    assert!(
        err_old.contains("warn[deprecated-op-name]"),
        "warns: {err_old}"
    );
    assert!(
        err_old.contains("`match(...)` is deprecated"),
        "names itself: {err_old}"
    );
    assert!(
        err_old.contains("match_line"),
        "names the replacement: {err_old}"
    );
    assert!(
        err_old.contains("match_ast"),
        "also points at match_ast for source code: {err_old}"
    );

    let (code_new, out_new, err_new) = run(&d, &prog_new, "new.db");
    assert_eq!(code_new, 0, "{err_new}");
    assert!(
        !err_new.contains("deprecated-op-name"),
        "canonical spelling is clean: {err_new}"
    );

    assert!(!out_old.is_empty(), "the legacy run actually produced rows");
    assert_eq!(sorted_lines(&out_old), sorted_lines(&out_new),
        "match(...) and match_line(...) produce identical rows over the same corpus\nold:\n{out_old}\nnew:\n{out_new}");

    let _ = fs::remove_dir_all(&d);
}

/// `sg(...)` (the pre-rename spelling) still parses and runs, produces rows
/// identical to `match_ast(...)` over the same corpus, and warns naming
/// match_ast.
#[test]
fn legacy_sg_runs_identically_to_match_ast_and_warns() {
    let d = sandbox("sg");
    let prog_old = concat!(
        "rel hit(p: file, name: text, l: int).\n",
        "hit(p, NAME, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"fn $NAME() {}\", l).\n",
        "? hit(p, name, l).\n",
    );
    let prog_new = prog_old.replace("sg(", "match_ast(");
    assert!(
        prog_new.contains("match_ast("),
        "sanity: replace actually fired"
    );

    let (code_old, out_old, err_old) = run(&d, prog_old, "old.db");
    assert_eq!(code_old, 0, "legacy `sg` still runs: {err_old}");
    assert!(
        err_old.contains("warn[deprecated-op-name]"),
        "warns: {err_old}"
    );
    assert!(
        err_old.contains("`sg(...)` is deprecated"),
        "names itself: {err_old}"
    );
    assert!(
        err_old.contains("match_ast"),
        "names the replacement: {err_old}"
    );

    let (code_new, out_new, err_new) = run(&d, &prog_new, "new.db");
    assert_eq!(code_new, 0, "{err_new}");
    assert!(
        !err_new.contains("deprecated-op-name"),
        "canonical spelling is clean: {err_new}"
    );

    assert!(!out_old.is_empty(), "the legacy run actually produced rows");
    assert_eq!(sorted_lines(&out_old), sorted_lines(&out_new),
        "sg(...) and match_ast(...) produce identical rows over the same corpus\nold:\n{out_old}\nnew:\n{out_new}");

    let _ = fs::remove_dir_all(&d);
}

/// Both old names warn independently in the SAME program (one for each), and
/// neither warning is silently swallowed by the other — symmetric treatment,
/// not "sg is fine, only match is deprecated" or vice versa.
#[test]
fn both_legacy_names_warn_independently_in_one_program() {
    let d = sandbox("both");
    let prog = concat!(
        "rel line_hit(p: file, l: int).\n",
        "line_hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "match(p, rev, /fn/, l).\n",
        "rel ast_hit(p: file, name: text, l: int).\n",
        "ast_hit(p, NAME, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), ",
        "sg(p, rev, :rust, \"fn $NAME() {}\", l).\n",
        "? line_hit(p, l).\n",
        "? ast_hit(p, name, l).\n",
    );
    let (code, out, err) = run(&d, prog, "both.db");
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("`match(...)` is deprecated"),
        "match warns: {err}"
    );
    assert!(
        err.contains("`sg(...)` is deprecated"),
        "sg warns too: {err}"
    );
    assert!(!out.is_empty(), "both rules still produced rows: {out}");
    let _ = fs::remove_dir_all(&d);
}
