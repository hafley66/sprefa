//! Language-level named args on relation atoms: `rel(col: term)` in a body or
//! `?` atom. Positional args fill left-to-right, named args fill by declared
//! column, and any unmentioned column becomes a don't-care — so an author binds
//! the one column they care about without counting positional `_` placeholders.
//!
//! Resolution rides the rel's declared columns (`resolve_named_args` in the
//! frontend), so it works for a user `rel` decl and a built-in schema alike, and
//! survives a forward reference (the atom before its `rel` line).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("named_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--no-daemon"])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A body atom names one column (`city:`) and binds another (`name:`); the third
/// column (`age`) is neither, so it resolves to a don't-care. Only bob is in sf.
#[test]
fn named_args_bind_and_pin_columns_in_a_body_atom() {
    let d = sandbox("body");
    let (code, out, err) = run(&d, concat!(
        "rel person(name: text, age: int, city: text).\n",
        "rel sfp(n: text).\n",
        "person(\"ann\", 30, \"nyc\").\n",
        "person(\"bob\", 25, \"sf\").\n",
        "sfp(n) <- person(name: n, city: \"sf\").\n",
        "? sfp(n).\n",
    ));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("bob"), "sf person is bob:\n{out}");
    assert!(!out.contains("ann"), "ann is in nyc, not selected:\n{out}");
    let _ = fs::remove_dir_all(&d);
}

/// A `?` query pins a single column by name; the rest are don't-cares.
#[test]
fn named_args_work_in_a_query_head() {
    let d = sandbox("query");
    let (code, out, err) = run(&d, concat!(
        "rel person(name: text, age: int, city: text).\n",
        "person(\"ann\", 30, \"nyc\").\n",
        "person(\"bob\", 25, \"sf\").\n",
        "? person(city: \"nyc\").\n",
    ));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("ann"), "nyc person is ann:\n{out}");
    assert!(!out.contains("bob"), "bob is in sf:\n{out}");
    let _ = fs::remove_dir_all(&d);
}

/// An atom may name a column before its `rel` decl appears (forward reference):
/// the frontend collects every schema before resolving.
#[test]
fn named_args_resolve_before_the_rel_decl() {
    let d = sandbox("forward");
    let (code, out, err) = run(&d, concat!(
        "q(n) <- person(name: n, city: \"sf\").\n",
        "rel q(n: text).\n",
        "rel person(name: text, age: int, city: text).\n",
        "person(\"bob\", 25, \"sf\").\n",
        "? q(n).\n",
    ));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("bob"), "forward-referenced schema still resolves:\n{out}");
    let _ = fs::remove_dir_all(&d);
}

/// In named mode (any `col:` present), a bare identifier puns to its own column:
/// `name` == `name: name`. Only the explicit `city:` and the punned `name` bind;
/// `age` is unmentioned, so it's a don't-care.
#[test]
fn a_bare_name_puns_to_its_own_column() {
    let d = sandbox("pun");
    let (code, out, err) = run(&d, concat!(
        "rel person(name: text, age: int, city: text).\n",
        "rel adult(name: text).\n",
        "person(\"ann\", 30, \"nyc\").\n",
        "person(\"bob\", 25, \"sf\").\n",
        "adult(name) <- person(name, city: \"sf\").\n",
        "? adult(name).\n",
    ));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("bob"), "punned `name` binds the name column:\n{out}");
    assert!(!out.contains("ann"), "ann is nyc:\n{out}");
    let _ = fs::remove_dir_all(&d);
}

/// A bare non-identifier (a literal) in named mode is ambiguous and errors — a
/// pun needs a name to bind, so a literal must be written `col: value`.
#[test]
fn bare_literal_in_named_mode_is_an_error() {
    let d = sandbox("barelit");
    let (code, _out, err) = run(&d, concat!(
        "rel person(name: text, age: int).\n",
        "q(n) <- person(name: n, 5).\n",
        "? q(n).\n",
    ));
    assert_ne!(code, 0);
    assert!(err.contains("ambiguous"), "bare literal in named mode is flagged:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// A misspelled column name is a clear parse-time error, naming the real columns.
#[test]
fn unknown_named_column_is_a_clear_error() {
    let d = sandbox("unknown");
    let (code, _out, err) = run(&d, concat!(
        "rel person(name: text, age: int).\n",
        "q(n) <- person(nam: n).\n",
        "? q(n).\n",
    ));
    assert_ne!(code, 0);
    assert!(err.contains("unknown column `nam`"), "names the bad column:\n{err}");
    assert!(err.contains("name, age"), "lists the real columns:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// Setting the same column by a pun and an explicit named arg is rejected.
#[test]
fn double_set_column_is_an_error() {
    let d = sandbox("double");
    let (code, _out, err) = run(&d, concat!(
        "rel person(name: text, age: int).\n",
        // `name` puns to column `name`; `name: n` also targets it → collision.
        "q(x) <- person(name, name: x).\n",
        "? q(x).\n",
    ));
    assert_ne!(code, 0);
    assert!(err.contains("set twice"), "flags the collision:\n{err}");
    let _ = fs::remove_dir_all(&d);
}

/// Named args in a rule head resolve by column name, same as body/query atoms.
/// A column named by neither an explicit arg nor a pun pads to NULL.
#[test]
fn named_args_in_a_rule_head_resolve() {
    let d = sandbox("head");
    let (code, out, err) = run(&d, concat!(
        "rel person(name: text, age: int).\n",
        "person(name: \"x\", age: 1).\n",
        // out of column order, and a head that names only one column (age -> NULL)
        "person(age: 2, name: \"y\").\n",
        "person(name: \"z\").\n",
        "? person(a, b).\n",
    ));
    assert_eq!(code, 0, "named head args resolve:\n{err}");
    assert!(out.contains("x\t1"), "in-order named head:\n{out}");
    assert!(out.contains("y\t2"), "out-of-order named head:\n{out}");
    assert!(out.contains("z\t"), "partial head pads the rest to NULL:\n{out}");
    let _ = fs::remove_dir_all(&d);
}

/// A rule head can't mix named args with an aggregate call: `aggs` is parallel to
/// the positional terms only, so the two shapes are incompatible.
#[test]
fn named_args_with_aggregate_head_are_rejected() {
    let d = sandbox("head_agg");
    let (code, _out, err) = run(&d, concat!(
        "rel edge(f: text, t: text).\n",
        "edge(\"a\", \"b\").\n",
        "rel fan(f: text, n: int).\n",
        "fan(f: f, count(t)) <- edge(f, t).\n",
        "? fan(f, n).\n",
    ));
    assert_ne!(code, 0);
    assert!(err.contains("mix named args with an aggregate"), "clear rejection:\n{err}");
    let _ = fs::remove_dir_all(&d);
}
