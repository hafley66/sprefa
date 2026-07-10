//! S3 body-level binds + S4 text `+` concat (2026-07-09 arc).
//!   S3: `callee = replace(callee_q, ".", "::")` in a DERIVED body binds the
//!       computed value for later atoms, negations, and the head; the RHS may
//!       consume any body-atom var or an earlier bind. Source rules keep the
//!       head-inline refusal (no second evaluator).
//!   S4: `+` is overloaded — int + int stays addition, text + text lowers to
//!       SQLite `||` (was a silent numeric-coercion `0`), mixed is a typecheck
//!       error naming the interp/int() fix.
//! Same sandbox harness as tests/it/facts.rs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("body_binds_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A bind feeds a later join atom AND a negation, through a CHAINED second
/// bind (the second consumes the first). The join keeps only the known callee;
/// the negation drops the blocked one.
#[test]
fn bind_feeds_join_negation_and_chains() {
    let dir = sandbox("chain");
    let prog = concat!(
        "rel raw_edge(caller: text, callee_q: text).\n",
        "raw_edge(\"main\", \"pkg.util.helper()\").\n",
        "raw_edge(\"main\", \"pkg.util.banned()\").\n",
        "raw_edge(\"main\", \"pkg.util.unknown()\").\n",
        "rel known_fn(name: text).\n",
        "known_fn(\"pkg::util::helper\").\n",
        "known_fn(\"pkg::util::banned\").\n",
        "rel blocked(name: text).\n",
        "blocked(\"pkg::util::banned\").\n",
        "rel resolved(caller: text, callee: text).\n",
        "resolved(caller, callee) <-\n",
        "  raw_edge(caller, callee_q),\n",
        "  stripped = replace(callee_q, \"()\", \"\"),\n",
        "  callee = replace(stripped, \".\", \"::\"),\n",
        "  known_fn(callee),\n",
        "  !blocked(callee).\n",
        "? resolved(caller, callee).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "bind chain must run:\n{err}");
    assert!(out.contains("pkg::util::helper"), "known + unblocked callee kept:\n{out}");
    assert!(!out.contains("banned"), "negation on the bind var drops the blocked row:\n{out}");
    assert!(!out.contains("unknown"), "join on the bind var drops the unknown row:\n{out}");
    assert!(out.contains("(1 rows)"), "exactly one resolved edge:\n{out}");
}

/// Text `+` in the head AND in a body bind: URL building from variables, the
/// exact S4 complaint shape. Was `0` (SQLite numeric coercion) before the fork.
#[test]
fn text_concat_in_head_and_bind() {
    let dir = sandbox("concat");
    let prog = concat!(
        "rel base_url(host: text).\n",
        "base_url(\"api.example.com\").\n",
        "rel endpoint(url: text, label: text).\n",
        "endpoint(url, \"svc: \" + host) <- base_url(host), url = \"https://\" + host + \"/v1\".\n",
        "? endpoint(url, label).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "text concat must run:\n{err}");
    assert!(out.contains("https://api.example.com/v1"), "bind concat:\n{out}");
    assert!(out.contains("svc: api.example.com"), "head concat:\n{out}");
}

/// int + int regression: still SQL addition, not concat.
#[test]
fn int_plus_int_stays_addition() {
    let dir = sandbox("intadd");
    let prog = concat!(
        "rel hit(line: int).\n",
        "hit(4).\n",
        "rel next_line(value: int).\n",
        "next_line(line + 1) <- hit(line).\n",
        "? next_line(value).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("5"), "4 + 1 = 5, not \"41\":\n{out}");
    assert!(!out.contains("41"), "int + must never concat:\n{out}");
}

/// Mixed int/text `+` is a typecheck error naming the fix, not a silent 0.
#[test]
fn mixed_plus_is_a_typecheck_error() {
    let dir = sandbox("mixed");
    let prog = concat!(
        "rel item(name: text, count: int).\n",
        "item(\"widget\", 3).\n",
        "rel label(text_out: text).\n",
        "label(name + count) <- item(name, count).\n",
        "? label(text_out).\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "mixed + must fail:\n{all}");
    assert!(all.contains("plus-mismatch"), "diag code expected:\n{all}");
    assert!(all.contains("interpolation") || all.contains("int(.."), "message names the fix:\n{all}");
}

/// A bind in a SOURCE rule body stays refused (no second evaluator), and the
/// message points at BOTH escape hatches: head-inline for source rules, body
/// binds for derived rules.
#[test]
fn source_rule_bind_refused_with_pointed_message() {
    let dir = sandbox("source");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), "fn alpha() {}\n").unwrap();
    let prog = concat!(
        "rel exts(path: file, ext: text).\n",
        "exts(path, ext) <- scan(\"WORK\", \"src/*.rs\", path, rev),\n",
        "  match(path, rev, /fn /, line),\n",
        "  ext = split(path, \".\", -1).\n",
        "? exts(path, ext).\n");
    let (code, _out, err) = run(&dir, prog);
    assert_ne!(code, 0, "source-rule bind must refuse:\n{err}");
    assert!(err.contains("put the expression in the rule head"), "head-inline fix named:\n{err}");
    assert!(err.contains("derived-rule bodies only"), "derived-rule alternative named:\n{err}");
}

/// The realistic case: the call_edge_bare-style suffix strip written head-inline
/// and as a body bind produce IDENTICAL row sets (same lowering, one evaluator).
#[test]
fn suffix_strip_bind_matches_head_inline() {
    let dir = sandbox("parity");
    let prog = concat!(
        "rel raw_edge(caller: text, callee_q: text).\n",
        "raw_edge(\"a::f\", \"pkg::helper()\").\n",
        "raw_edge(\"a::g\", \"pkg::nested::helper()\").\n",
        "raw_edge(\"b::h\", \"other::thing\").\n",
        "rel bare_inline(caller: text, callee: text).\n",
        "bare_inline(caller, replace(callee_q, \"()\", \"\")) <- raw_edge(caller, callee_q).\n",
        "rel bare_bound(caller: text, callee: text).\n",
        "bare_bound(caller, callee) <- raw_edge(caller, callee_q), callee = replace(callee_q, \"()\", \"\").\n",
        "? bare_inline(caller, callee).\n",
        "? bare_bound(caller, callee).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "{err}");
    let block = |rel: &str| -> Vec<String> {
        out.split(&format!("? {rel} =>")).nth(1).unwrap()
            .lines().skip(1)
            .map(|line| line.trim().to_string())
            .take_while(|line| !line.is_empty() && !line.starts_with('?'))
            .collect()
    };
    let (mut inline_rows, mut bound_rows) = (block("bare_inline"), block("bare_bound"));
    inline_rows.sort();
    bound_rows.sort();
    assert_eq!(inline_rows, bound_rows, "head-inline and body-bind rows must be identical:\n{out}");
    assert!(inline_rows.iter().any(|row| row.contains("pkg::helper")), "{out}");
}
