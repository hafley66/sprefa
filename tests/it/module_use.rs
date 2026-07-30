//! Module surface: `use "path".` includes another `.dl` file. Canonical-path
//! dedup makes diamond imports load once; same-name same-cols rel decls merge;
//! conflicting cols hard-error. The frontend (`frontend::load_program`) owns
//! the only place sugar disappears; with no `use` in any source file the path
//! is byte-for-byte identical to the prior parse-only pipeline.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("use_mod_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &std::path::Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A `use "dep.dl".` splices the dep file's items: its rels are declared, its
/// rules derive, the program's own query reaches into them. With no `use` the
/// same program errors out on the unknown rel, proving the splice happened.
#[test]
fn use_splices_items_into_scope() {
    let d = sandbox("basic");
    fs::write(
        d.join("dep.dl"),
        "\
        rel greeting(who: text).\n\
        greeting(\"world\").\n\
        greeting(\"module\").\n",
    )
    .unwrap();
    let (code, out, err) = run(&d, "use \"dep.dl\".\n? greeting(who).\n");
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("world"), "imported fact present: {out}");
    assert!(out.contains("module"), "imported fact present: {out}");
    assert!(out.contains("(2 rows)"), "exactly the two facts: {out}");
}

/// A diamond: A uses B and C, both of which use D. D loads once; its rel
/// declaration dedups so a same-name same-cols re-declaration is a no-op (no
/// "table already exists" error from the lower).
#[test]
fn diamond_import_loads_each_module_once() {
    let d = sandbox("diamond");
    fs::write(
        d.join("d.dl"),
        "rel shared(x: int).\nshared(1).\nshared(2).\n",
    )
    .unwrap();
    fs::write(
        d.join("b.dl"),
        "use \"d.dl\".\nrel b_only(x: int).\nb_only(10).\n",
    )
    .unwrap();
    fs::write(
        d.join("c.dl"),
        "use \"d.dl\".\nrel c_only(x: int).\nc_only(20).\n",
    )
    .unwrap();
    let (code, out, err) = run(
        &d,
        "\
        use \"b.dl\".\n\
        use \"c.dl\".\n\
        ? shared(x).\n\
        ? b_only(x).\n\
        ? c_only(x).\n",
    );
    assert_eq!(code, 0, "diamond import must succeed: {err}");
    // The shared rel's facts survive once; b_only and c_only are distinct.
    let sh = out.split("? shared").nth(1).unwrap_or("");
    assert!(
        sh.contains("(2 rows)"),
        "shared dedups to two rows (loaded once): {out}"
    );
    assert!(out.contains("10"), "b_only fact present: {out}");
    assert!(out.contains("20"), "c_only fact present: {out}");
}

/// A same-name same-cols rel declaration across the program and an imported
/// module dedups. This is the contract a library author relies on: the same
/// `rel foo(x: int).` line can appear in two files without error.
#[test]
fn same_name_same_cols_dedup() {
    let d = sandbox("dedup_ok");
    fs::write(d.join("lib.dl"), "rel fact(x: int).\nfact(100).\n").unwrap();
    let (code, out, err) = run(
        &d,
        "\
        use \"lib.dl\".\n\
        rel fact(x: int).\n\
        fact(1).\n\
        ? fact(x).\n",
    );
    assert_eq!(code, 0, "same-name same-cols must dedup: {err}");
    assert!(
        out.contains("100") && out.contains("1"),
        "both facts present: {out}"
    );
    assert!(
        out.contains("(2 rows)"),
        "two facts total after dedup: {out}"
    );
}

/// A rel name that conflicts on cols (different name OR different type) hard
/// errors. The import story's worst failure mode is a silent shadow; this is
/// the loud guard against it.
#[test]
fn conflicting_cols_hard_error() {
    let d = sandbox("dedup_conflict");
    fs::write(d.join("lib.dl"), "rel fact(x: int).\nfact(1).\n").unwrap();
    let (code, out, err) = run(
        &d,
        "\
        use \"lib.dl\".\n\
        rel fact(x: text).\n\
        fact(\"hello\").\n\
        ? fact(x).\n",
    );
    assert_ne!(code, 0, "conflicting cols must hard-error: {out}");
    assert!(
        err.contains("declared twice"),
        "error names the conflict: {err}"
    );
}

/// A `use` of a missing file DEGRADES: it emits a `use-unresolved` diagnostic
/// (the import is skipped, the rest of the program still loads) rather than
/// bailing the whole load — which, in the LSP, would kill the server. The exit
/// is still non-zero (an Error-severity diag) and the message lists the roots
/// tried, so a typo does not silently resolve to an empty program.
#[test]
fn missing_module_yells_and_degrades() {
    let d = sandbox("missing");
    let (code, out, err) = run(&d, "use \"nope.dl\".\nrel t(x: int).\nt(1).\n? t(x).\n");
    assert_ne!(code, 0, "missing module is still an error: {out}");
    assert!(
        err.contains("use-unresolved"),
        "diagnostic carries the code: {err}"
    );
    assert!(
        err.contains("cannot resolve") && err.contains("nope.dl"),
        "message names the failed import: {err}"
    );
    assert!(
        err.contains("not found"),
        "message lists the roots tried: {err}"
    );
}

/// `use` requires a string literal. The lookahead is `use` followed by
/// `Tok::Str`; anything else (`use foo.`, `use 5.`) is rejected at parse time.
/// `rel use(name: text).` still parses as a rel because the second token is
/// `(`, not a string.
#[test]
fn use_requires_string_literal() {
    let d = sandbox("syntax");
    let (code, out, err) = run(&d, "use foo.\nrel t(x: int).\nt(1).\n? t(x).\n");
    assert_ne!(code, 0, "non-string use must be a parse error: {out}");
    // The error names the offending token; the program is rejected, not silently
    // parsed as a rel named `use` with the trailing ident.
    assert!(
        err.contains("in "),
        "parse error carries file context: {err}"
    );
}

/// `rel use(name: text).` is a legal rel name; the lookahead distinguishes it
/// from the import keyword (a Str after `use`). This guards the existing
/// `examples/callgraph.dl` shape, where `use` is a rel carrying call-site
/// names, from regressing when the import keyword was added.
#[test]
fn use_as_rel_name_still_parses() {
    let d = sandbox("rel_named_use");
    let (code, out, err) = run(
        &d,
        "\
        rel use(name: text).\n\
        use(\"lex\").\n\
        ? use(n).\n",
    );
    assert_eq!(code, 0, "`use` as a rel name must parse: {err}");
    assert!(out.contains("lex"), "fact present: {out}");
}

/// `use "std/<example>.dl"` resolves an EMBEDDED example from the binary when
/// the file is absent on disk. Real std libs win on name clash; examples fill
/// in the std/ namespace. The gh-checkout example has
/// `checkout(slug, "", "0") <- repo(slug, root, url).` plus a demo
/// `? checkout_done(...)` query — the rule splices in, the demo `?` is stripped
/// (library vs demo), so only the parent's own `?` query fires.
#[test]
fn use_std_resolves_embedded_example_and_strips_demo_query() {
    let d = sandbox("std_example");
    // No source tree + no examples/ on disk: the only way `use` resolves is the
    // embedded copy in the binary. SPREFA_CONFIG=empty so `repo` is just the
    // self `--root` repo (one row), which the spliced rule sees.
    let (code, out, err) = run(
        &d,
        "\
        use \"std/gh-checkout.dl\".\n\
        ? checkout(slug, branch, pr).\n",
    );
    assert_eq!(code, 0, "embedded example `use` resolved: {err}");
    // The example's rule spliced in: its `checkout(slug, "", "0") <- repo(...)`
    // produced a row for the self repo.
    assert!(
        out.contains("? checkout"),
        "the spliced-in rule produced a checkout row: {out}"
    );
    // The example's demo `? checkout_done(...)` query was STRIPPED (library
    // splice). Only the parent's `? checkout(...)` query should fire — exactly
    // one `? ...` section in stdout.
    let query_count = out.lines().filter(|l| l.starts_with("? ")).count();
    assert_eq!(
        query_count, 1,
        "demo query stripped on splice (1 = parent only): {out}"
    );
}

/// A bare `use "gh-checkout.dl"` (no `std/` prefix) does NOT resolve to the
/// embedded example. The `std/` prefix is the library namespace; bare names
/// would silently shadow user files. Reject with a `use-unresolved` squiggle.
#[test]
fn use_bare_name_does_not_resolve_embedded_example() {
    let d = sandbox("bare_rejected");
    let (code, _out, err) = run(
        &d,
        "\
        use \"gh-checkout.dl\".\n\
        ? checkout(slug, branch, pr).\n",
    );
    assert_ne!(code, 0, "a bare example name must not resolve silently");
    assert!(
        err.contains("use-unresolved") && err.contains("gh-checkout.dl"),
        "bare name rejected with a use-unresolved squiggle naming the path: {err}"
    );
}
