//! String functions `split(text, sep, idx)` and `replace(text, from, to)` in
//! rule heads and comparison sides. Same positioning rules as int arithmetic:
//! derived heads lower to SQL (`replace` native, `split` via the `sprf_split`
//! UDF in db.rs), source heads evaluate via `val_of`, body atom positions stay
//! binding-only. `idx` is 0-based; negative counts from the end (-1 = last).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("string_fns_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
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

/// Derived-head split: last path segment via negative idx (unary minus).
/// `src/lib.rs` -> `lib.rs`, `src/deep/mod.rs` -> `mod.rs`.
#[test]
fn split_derived_head_last_segment() {
    let d = sandbox("split_derived");
    let prog = r#"
rel f(path: text).
f("src/lib.rs").
f("src/deep/mod.rs").
f("top.rs").
rel stem(path: text, last: text).
stem(path, split(path, "/", -1)) <- f(path).
? stem(path, last).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("lib.rs\tlib.rs"), "last seg of src/lib.rs: {out}");
    assert!(out.contains("src/deep/mod.rs\tmod.rs"), "last seg of nested path: {out}");
    assert!(out.contains("top.rs\ttop.rs"), "no-separator passthrough: {out}");
}

/// Source-head split: split a captured fn name on `_`, first and last segments.
#[test]
fn split_source_head_segments() {
    let d = sandbox("split_source");
    fs::write(d.join("src/lib.rs"), "fn alpha_fn() {}\nfn beta_thing() {}\n").unwrap();
    let prog = r#"
rel seg(f: file, first: text, last: text).
seg(f, split(n, "_", 0), split(n, "_", -1)) <-
    scan("WORK", "src/*.rs", f, rev), match(f, rev, /fn (?<n>[a-z_]+)/, l).
? seg(f, first, last).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("alpha\tfn"), "alpha_fn split 0/-1: {out}");
    assert!(out.contains("beta\tthing"), "beta_thing split 0/-1: {out}");
}

/// Derived-head replace: underscores to hyphens (kebab). Uses native SQLite replace.
#[test]
fn replace_derived_head() {
    let d = sandbox("replace");
    let prog = r#"
rel n(w: text).
n("foo_bar").
n("a_b_c").
rel kebab(w: text, k: text).
kebab(w, replace(w, "_", "-")) <- n(w).
? kebab(w, k).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("foo_bar\tfoo-bar"), "single replace: {out}");
    assert!(out.contains("a_b_c\ta-b-c"), "replace is global (all occurrences): {out}");
}

/// split on a comparison side: filter to names whose last `.` segment is a known
/// extension. Function calls sit on the RHS of `=` (the LHS is a var); a leading
/// `split(...)` would parse as a body atom (relation "split"), same as any
/// `ident(args)` in body-leading position.
#[test]
fn split_in_comparison() {
    let d = sandbox("split_cmp");
    let prog = r#"
rel f(p: text).
f("a.rs").
f("b.md").
f("c.rs").
rel rust(p: text).
rust(p) <- f(p), ext = split(p, ".", -1), ext = "rs".
? rust(p).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("a.rs") && out.contains("c.rs"), "rust files: {out}");
    assert!(!out.contains("b.md"), "md filtered out: {out}");
}

/// Out-of-range idx drops the row (NULL semantics). `split("ab", " ", 0)` has
/// one part (idx 0 valid); idx 1 is out of range -> no row.
#[test]
fn split_out_of_range_drops_row() {
    let d = sandbox("oob");
    let prog = r#"
rel f(s: text).
f("one_part").
rel hit(s: text, seg: text).
hit(s, split(s, " ", 0)) <- f(s).
rel miss(s: text, seg: text).
miss(s, split(s, " ", 1)) <- f(s).
? hit(s, seg).
? miss(s, seg).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let hit = out.split("? hit =>").nth(1).unwrap().split('?').next().unwrap();
    assert!(hit.contains("one_part"), "idx 0 of one_part: {hit}");
    let miss = out.split("? miss =>").nth(1).unwrap();
    assert!(miss.contains("(0 rows)"), "idx 1 out of range -> 0 rows: {miss}");
}

/// A function call as an ARG of a body atom is a lower-time error (same rule
/// as arithmetic: body atoms bind, they don't compute). `g(split(...))` in body
/// position is rejected because the Call is a computed value, not a binding.
#[test]
fn call_in_body_atom_is_an_error() {
    let d = sandbox("body");
    let prog = r#"
rel f(s: text).
f("a/b").
rel g(x: text).
g("a").
rel bad(s: text).
bad(s) <- f(s), g(split(s, "/", 0)).
? bad(s).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "function call as a body-atom arg must fail");
    assert!(
        err.contains("function call only allowed in a rule head or comparison, not a body atom"),
        "got: {err}"
    );
}

/// `int(text)` in a derived head: a numeric string fills an int column. Mirrors
/// SQLite `CAST(.. AS INTEGER)` — leading int prefix, garbage -> 0.
#[test]
fn int_derived_head_casts_text() {
    let d = sandbox("int_derived");
    let prog = r#"
rel n(s: text).
n("42").
n("7x").
n("abc").
rel num(s: text, v: int).
num(s, int(s)) <- n(s).
? num(s, v).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("42\t42"), "plain int: {out}");
    assert!(out.contains("7x\t7"), "leading-int prefix: {out}");
    assert!(out.contains("abc\t0"), "non-numeric -> 0: {out}");
}

/// `int(...)` on a comparison side in a SOURCE rule (evaluated via `val_of`):
/// keep only matches whose captured number is positive. This is the papercut's
/// exact shape — a numeric capture compared numerically, not as text against "0".
#[test]
fn int_in_source_comparison() {
    let d = sandbox("int_cmp");
    fs::write(d.join("src/v.txt"), "count=42\ncount=0\ncount=7\n").unwrap();
    let prog = r#"
rel pos(f: file, c: text).
pos(f, c) <- scan("WORK", "src/*.txt", f, rev),
    match(f, rev, /count=(?<c>\d+)/, l), 0 < int(c).
? pos(f, c).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\t42") && out.contains("\t7"), "positive counts kept: {out}");
    assert!(!out.contains("\t0\n") && !out.contains("\t0\t"), "zero count filtered: {out}");
}

/// `int(..)` cannot fill a non-int column (it produces an int, not text).
#[test]
fn int_into_text_column_errors() {
    let d = sandbox("int_text");
    let prog = r#"
rel n(s: text).
n("5").
rel bad(s: text, v: text).
bad(s, int(s)) <- n(s).
? bad(s, v).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "int() into a text column must fail");
    assert!(err.contains("int") && err.contains("non-int column"), "got: {err}");
}

/// An unknown function name is rejected (whitelist is split/replace).
#[test]
fn unknown_function_errors() {
    let d = sandbox("unknown");
    let prog = r#"
rel f(s: text).
f("x").
rel bad(s: text, v: text).
bad(s, frobnicate(s, "/", 0)) <- f(s).
? bad(s, v).
"#;
    let (code, _out, err) = run(&d, prog);
    assert_ne!(code, 0, "unknown function must fail");
    assert!(
        err.contains("unknown") && err.contains("frobnicate"),
        "error names the unknown function: {err}"
    );
}

/// Case + first-char builtins: lower/upper/lcfirst/ucfirst.
#[test]
fn case_builtins() {
    let d = sandbox("case");
    let prog = r#"
rel n(w: text).
n("GetUser").
rel cased(w: text, lo: text, up: text, lcf: text, ucf: text).
cased(w, lower(w), upper(w), lcfirst(w), ucfirst(w)) <- n(w).
? cased(w, lo, up, lcf, ucf).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("getuser\tGETUSER\tgetUser\tGetUser"), "case fns: {out}");
}

/// trim + anchored strip_prefix/strip_suffix. Strip returns the input UNCHANGED
/// when the affix is absent (idempotent cleanup, not a filter).
#[test]
fn trim_and_strip_affix() {
    let d = sandbox("strip");
    let prog = r#"
rel n(w: text).
n("  hi  ").
n("useFoo").
n("bareName").
rel out(w: text, t: text, p: text, s: text).
out(w, trim(w), strip_prefix(w, "use"), strip_suffix(w, "Name")) <- n(w).
? out(w, t, p, s).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("useFoo\tuseFoo\tFoo\tuseFoo"), "strip_prefix present, suffix absent: {out}");
    assert!(out.contains("bareName\tbareName\tbareName\tbare"), "prefix absent, suffix present: {out}");
    assert!(out.contains("  hi  \thi"), "trim: {out}");
}

/// replace_re: regex replace-ALL strips the whole RTKQ affix family in one call.
/// `^Lazy|(Query|Mutation)$` matches both the leading `Lazy` and the trailing
/// suffix; replace_all removes both occurrences.
#[test]
fn replace_re_groups_and_family() {
    let d = sandbox("repre");
    let prog = r#"
rel n(w: text).
n("useGetUserQuery").
n("useUpdateUserMutation").
n("useLazyGetUserQuery").
rel stem(w: text, s: text).
stem(w, replace_re(strip_prefix(w, "use"), "^Lazy|(Query|Mutation)$", "")) <- n(w).
? stem(w, s).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("useGetUserQuery\tGetUser"), "Query suffix stripped: {out}");
    assert!(out.contains("useUpdateUserMutation\tUpdateUser"), "Mutation suffix stripped: {out}");
    assert!(out.contains("useLazyGetUserQuery\tGetUser"), "Lazy prefix + Query suffix stripped: {out}");
}

/// The headline composition: recover the RTKQ endpoint op name from a hook
/// identifier. `useGetUserQuery` -> `getUser`. Pure dl, no engine special case.
#[test]
fn rtkq_op_name_recovery() {
    let d = sandbox("rtkq");
    let prog = r#"
rel hook(h: text).
hook("useGetUserQuery").
hook("useUpdateUserMutation").
hook("useLazyListUsersQuery").
rel op(h: text, name: text).
op(h, lcfirst(replace_re(strip_prefix(h, "use"), "^Lazy|(Query|Mutation)$", ""))) <- hook(h).
? op(h, name).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("useGetUserQuery\tgetUser"), "getUser: {out}");
    assert!(out.contains("useUpdateUserMutation\tupdateUser"), "updateUser: {out}");
    assert!(out.contains("useLazyListUsersQuery\tlistUsers"), "listUsers: {out}");
}

/// Unary minus parses as a negative literal: `-1` in split idx, no parens
/// needed. Distinct from binary subtraction `a - 1` which still works.
#[test]
fn unary_minus_in_call_idx() {
    let d = sandbox("unary");
    let prog = r#"
rel f(s: text).
f("a.b.c").
rel last(s: text, v: text).
last(s, split(s, ".", -1)) <- f(s).
rel sub(s: text, v: text).
sub(s, split(s, ".", 0 - 1)) <- f(s).
? last(s, v).
? sub(s, v).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "{err}");
    let last = out.split("? last =>").nth(1).unwrap().split('?').next().unwrap();
    assert!(last.contains("\tc"), "unary -1 last seg: {last}");
    let sub = out.split("? sub =>").nth(1).unwrap();
    assert!(sub.contains("\tc"), "binary 0-1 last seg: {sub}");
}
