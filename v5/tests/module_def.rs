//! `def name(params) <- body.` declares a parameterized rule template. A body
//! atom `name(args)` is inlined: the template body is cloned, params are
//! substituted by the args, every non-param internal var is alpha-renamed so
//! two instantiations never capture each other. The arity-reuse layer that
//! lets wide relations cohere across files (one extractor pattern in a lib,
//! many call sites in a program).
//!
//! `def` is inline-only. No recursion, no fixed-point: a template that
//! transitively calls itself is rejected at compile time.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("def_tpl_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &std::path::Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The simplest shape: one template, one call site. The template body is
/// substituted verbatim with the call's args bound to the params. This is the
/// arity-reuse "save me copy-pasting this 5-atom pattern" promise.
#[test]
fn def_inlines_at_call_site() {
    let d = sandbox("basic");
    let (code, out, err) = run(&d, "\
        rel edge(a: int, b: int).\n\
        rel two_hop(a: int, b: int).\n\
        edge(1, 2).\n\
        edge(2, 3).\n\
        def via(x, z) <- edge(x, m), edge(m, z).\n\
        two_hop(a, b) <- via(a, b).\n\
        ? two_hop(a, b).\n");
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("1\t3"), "1->2->3 path via template: {out}");
}

/// Two instantiations of the same template in one rule body. The internal var
/// `m` must alpha-rename to disjoint vars or the two calls collapse into one
/// binding. This is the regression guard for the per-call-site counter.
#[test]
fn two_calls_to_same_template_dont_share_internal_vars() {
    let d = sandbox("disjoint");
    let (code, out, err) = run(&d, "\
        rel edge(a: int, b: int).\n\
        rel four_hop(a: int, b: int).\n\
        edge(1, 2). edge(2, 3). edge(3, 4). edge(4, 5).\n\
        def via(x, z) <- edge(x, m), edge(m, z).\n\
        four_hop(a, b) <- via(a, mid), via(mid, b).\n\
        ? four_hop(a, b).\n");
    assert_eq!(code, 0, "{err}");
    // 1->2->3->4->5 is the only 4-edge chain. If the two `via` calls shared
    // their internal `m`, this would collapse to a 2-edge chain (1->3) and
    // there would be no 4-hop result.
    assert!(out.contains("1\t5"), "alpha-rename keeps siblings disjoint: {out}");
    assert!(out.contains("(1 rows)"), "exactly one 4-edge chain: {out}");
}

/// The classic arity-reuse case from the plan: a qualified call-graph join
/// (`fndef` + `callsite` + range containment) factored into a `def`. Without
/// the template, every program that wanted this join would copy-paste the
/// three atoms. With it, the program states its inputs and the lib states the
/// join shape.
#[test]
fn qualified_call_graph_join_factored_into_def() {
    let d = sandbox("qcall");
    // A tiny corpus: fn `outer` spans lines 1-3, a call to `inner` at line 2.
    // Without the range containment, every fn would call every other fn in
    // the same file.
    fs::write(d.join("src.rs"), "\
fn outer() {\n\
    inner();\n\
}\n\
fn inner() {}\n").unwrap();
    let (code, out, err) = run(&d, "\
        rel fndef(name: text, path: file, s: int, e: int).\n\
        rel callsite(callee: text, path: file, line: int).\n\
        rel calls(caller: text, callee: text).\n\
        \n\
        fndef(name, p, s, e) <-\n\
          scan(\"WORK\", \"src.rs\", p, rev),\n\
          match(p, rev, /fn\\s+(?<name>\\w+)/, s),\n\
          match(p, rev, /}/, e).\n\
        callsite(callee, p, line) <-\n\
          scan(\"WORK\", \"src.rs\", p, rev),\n\
          match(p, rev, /(?<callee>\\w+)\\s*\\(/, line).\n\
        \n\
        def qcall(caller, callee) <-\n\
          fndef(caller, p, s, e),\n\
          callsite(callee, p, l),\n\
          s <= l, l <= e.\n\
        \n\
        calls(caller, callee) <- qcall(caller, callee).\n\
        ? calls(caller, callee).\n");
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("outer\tinner"), "range containment resolved: {out}");
    // The reverse direction (`inner calls outer`) must NOT appear: the call to
    // `inner` is inside `outer`'s body, not the other way around. A naive
    // file-scoped heuristic (no def) would surface both.
    let block = out.split("? calls").nth(1).unwrap_or("");
    assert!(!block.contains("inner\touter"), "containment is one-directional: {out}");
}

/// A template is not itself a rule: declaring `def foo(x) <- ...` with no call
/// site emits zero rules. The `foo` rel is undefined; querying it without a
/// call site is an unknown-relation error (the same as any other undefined rel).
#[test]
fn def_without_call_site_emits_nothing() {
    let d = sandbox("orphan");
    let (code, out, err) = run(&d, "\
        rel edge(a: int, b: int).\n\
        edge(1, 2).\n\
        def via(x, z) <- edge(x, m), edge(m, z).\n\
        ? edge(a, b).\n");
    assert_eq!(code, 0, "no call site, but the program is still valid: {err}");
    assert!(out.contains("1\t2"), "non-template query still works: {out}");
}

/// A template call with the wrong arity hard-errors at expand time. The error
/// names the template and the expected vs actual arg count.
#[test]
fn wrong_arity_at_call_site_errors() {
    let d = sandbox("arity");
    let (code, _, err) = run(&d, "\
        rel edge(a: int, b: int).\n\
        edge(1, 2).\n\
        def via(x, z) <- edge(x, m), edge(m, z).\n\
        rel r(a: int).\n\
        r(a) <- via(a).\n\
        ? r(a).\n");
    assert_ne!(code, 0);
    assert!(err.contains("expects 2 args"), "arity error names the template: {err}");
}

/// Recursion is rejected: a template that transitively calls itself would
/// inline forever, so the cycle check bails up front naming the entry.
#[test]
fn recursion_is_rejected() {
    let d = sandbox("cycle");
    let (code, _, err) = run(&d, "\
        rel edge(a: int, b: int).\n\
        edge(1, 2).\n\
        def reach(x, z) <- edge(x, z).\n\
        def reach(x, z) <- edge(x, m), reach(m, z).\n\
        rel r(a: int, b: int).\n\
        r(a, b) <- reach(a, b).\n\
        ? r(a, b).\n");
    assert_ne!(code, 0, "recursive def must be rejected: {err}");
    assert!(err.contains("declared twice"), "same-name second def is a conflict: {err}");
}

/// Same-name second `def` is a conflict (mirrors rel-decl conflicts). The
/// library author who wants two shapes under one name must rename one.
#[test]
fn same_name_def_twice_is_conflict() {
    let d = sandbox("dupe");
    let (code, _, err) = run(&d, "\
        rel edge(a: int, b: int).\n\
        edge(1, 2).\n\
        def via(x, z) <- edge(x, m), edge(m, z).\n\
        def via(x, z) <- edge(x, z).\n\
        rel r(a: int, b: int).\n\
        r(a, b) <- via(a, b).\n\
        ? r(a, b).\n");
    assert_ne!(code, 0);
    assert!(err.contains("declared twice"), "second def of same name is conflict: {err}");
}

/// A `def` from an imported file is callable from the importer. The template
/// table is scoped to the whole merge, mirroring how rel decls already cross
/// file boundaries. This is the cross-file arity-reuse promise: a library
/// ships the template, programs call it without seeing the body.
#[test]
fn def_crosses_use_boundary() {
    let d = sandbox("lib");
    fs::write(d.join("lib.dl"), "\
        rel edge(a: int, b: int).\n\
        def via(x, z) <- edge(x, m), edge(m, z).\n").unwrap();
    let (code, out, err) = run(&d, "\
        use \"lib.dl\".\n\
        edge(1, 2).\n\
        edge(2, 3).\n\
        rel two_hop(a: int, b: int).\n\
        two_hop(a, b) <- via(a, b).\n\
        ? two_hop(a, b).\n");
    assert_eq!(code, 0, "imported def is callable: {err}");
    assert!(out.contains("1\t3"), "imported def resolves: {out}");
}

/// A `def` keyword with the lookahead-shape `def name(` parses as a template.
/// A rule whose head rel is literally `def` (`def(args) <- ...`) still parses
/// as a rule because the second token is `(` not an ident. Mirrors the
/// `use`-as-rel-name guard.
#[test]
fn def_as_rel_name_still_parses() {
    let d = sandbox("rel_named_def");
    let (code, out, err) = run(&d, "\
        rel def(x: int).\n\
        def(1).\n\
        def(2).\n\
        ? def(x).\n");
    assert_eq!(code, 0, "`def` as a rel name must parse: {err}");
    assert!(out.contains("1") && out.contains("2"), "facts present: {out}");
}
