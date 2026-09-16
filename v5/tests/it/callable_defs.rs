//! call_def rows for the callable-lambda-ctor arc: lambdas (unbound closures),
//! constructors, nested named fns, and Rust trait method decls/defaults, over
//! the in-repo fixtures at tests/fixtures/callables/. Pins:
//!   - the TS/Kotlin/Python constructor rows (df-matching syms),
//!   - an unbound lambda row per language (deterministic `::closure::` sym),
//!   - byte-identical extraction across two cold runs (the determinism law),
//!   - every lambda sym also names a df closure scope (the df<->call join).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const FIXTURES: [&str; 5] = ["rust.rs", "ts.ts", "kotlin.kt", "go.go", "python.py"];

const SCAN: &str = r#"
rel src(path: file, rev: text).
src(path, rev) <- scan("WORK", "rust.rs", path, rev).
src(path, rev) <- scan("WORK", "ts.ts", path, rev).
src(path, rev) <- scan("WORK", "kotlin.kt", path, rev).
src(path, rev) <- scan("WORK", "go.go", path, rev).
src(path, rev) <- scan("WORK", "python.py", path, rev).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("callable_defs_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/callables");
    for name in FIXTURES {
        fs::copy(src.join(name), dir.join(name)).unwrap();
    }
    // Hermetic: an empty config so no ambient ~/.config/sprefa repos are ingested.
    fs::write(dir.join("empty.toml"), "").unwrap();
    dir
}

fn run(dir: &Path, prog: &str, db: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join(db).to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_CONFIG", dir.join("empty.toml"))
        .current_dir(dir)
        .output()
        .expect("run dl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn constructors_nested_and_trait_call_defs_emit() {
    let dir = sandbox("shapes");
    let out = run(
        &dir,
        &format!("{SCAN}\n? call_def(repo, sym, kind, file, line, end).\n"),
        "db",
    );

    // Constructors (kind method), sym IDENTICAL to what the df walkers mint.
    assert!(
        out.contains("ts.ts::method::Widget.constructor"),
        "TS ctor missing:\n{out}"
    );
    assert!(
        out.contains("kotlin.kt::method::Widget.<init>"),
        "kt primary ctor missing:\n{out}"
    );
    assert!(
        out.contains("kotlin.kt::method::Widget.<init>@"),
        "kt secondary ctor missing:\n{out}"
    );
    assert!(
        out.contains("python.py::method::Widget.__init__"),
        "py __init__ missing:\n{out}"
    );

    // Rust trait method DECLARATION (no body) + DEFAULT body, Method-owned by trait.
    assert!(
        out.contains("rust.rs::method::Shape.perimeter"),
        "rust trait decl missing:\n{out}"
    );
    assert!(
        out.contains("rust.rs::method::Shape.describe"),
        "rust trait default missing:\n{out}"
    );

    // Nested named fns (previously NOT EMITTED per the audit).
    assert!(
        out.contains("rust.rs::function::nested_helper"),
        "rust nested fn missing:\n{out}"
    );
    assert!(
        out.contains("ts.ts::function::tally"),
        "ts nested fn missing:\n{out}"
    );
    assert!(
        out.contains("kotlin.kt::function::nestedHelper"),
        "kt nested fn missing:\n{out}"
    );
    assert!(
        out.contains("python.py::function::nested_helper"),
        "py nested fn missing:\n{out}"
    );

    // An unbound lambda row (kind lambda, deterministic `::closure::` sym) per lang.
    for lang in FIXTURES {
        let has = out
            .lines()
            .any(|l| l.contains(lang) && l.contains("::closure::") && l.contains("\tlambda\t"));
        assert!(has, "no unbound lambda call_def for {lang}:\n{out}");
    }
}

#[test]
fn new_expression_resolves_to_ts_constructor() {
    let dir = sandbox("newexpr");
    // TS only: with all fixtures scanned, "Widget" is an ambiguous callee (the
    // TS ctor + both Kotlin ctors all carry the class name), so resolution
    // honestly bails. Scoped to ts.ts, "Widget" is unique and `return new
    // Widget(1)` in Widget.unit resolves to the ctor sym.
    let prog = "rel src(path: file, rev: text).\n\
                src(path, rev) <- scan(\"WORK\", \"ts.ts\", path, rev).\n\
                ? call_edge(caller, callee, kind).\n";
    let out = run(&dir, prog, "db");
    let resolved = out.lines().any(|l| {
        l.contains("ts.ts::method::Widget.unit") && l.contains("ts.ts::method::Widget.constructor")
    });
    assert!(resolved, "new Widget() did not resolve to the ctor:\n{out}");
}

#[test]
fn lambda_and_ctor_extraction_is_byte_identical() {
    let dir = sandbox("determ");
    let prog = format!("{SCAN}\n? call_def(repo, sym, kind, file, line, end).\n");
    let first = run(&dir, &prog, "db1");
    let second = run(&dir, &prog, "db2");
    let rows = |dump: &str| {
        let mut rows: Vec<&str> = dump.lines().filter(|l| l.contains("::")).collect();
        rows.sort_unstable();
        rows.join("\n")
    };
    let a = rows(&first);
    assert!(
        a.contains("::closure::"),
        "expected lambda rows in the dump:\n{first}"
    );
    assert_eq!(
        a,
        rows(&second),
        "call_def extraction is not byte-identical across two cold runs"
    );
}

#[test]
fn every_lambda_sym_names_a_df_closure_scope() {
    let dir = sandbox("dfjoin");
    // Strip the uniform `<repo>::` prefix off call_def syms (df_node syms carry
    // no repo prefix), then anti-join: a lambda sym that is neither a df closure
    // value node's `var` nor any lifted body's `fn` is an orphan. Expect none.
    let prog = format!(
        "{SCAN}
rel lam(sym: text).
lam(stripped) <- call_def(_, sym, \"lambda\", _, _, _), stripped = replace_re(sym, \"^[^:]+::\", \"\").
rel df_scope(sym: text).
df_scope(var) <- df_node(_, \"closure\", var, _, _, _, _).
df_scope(fnsym) <- df_node(_, _, _, fnsym, _, _, _).
rel orphan(sym: text).
orphan(sym) <- lam(sym), !df_scope(sym).
? orphan(sym).
"
    );
    let out = run(&dir, &prog, "db");
    let orphan_section: String = out
        .lines()
        .skip_while(|l| !l.contains("? orphan"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        orphan_section.contains("(0 rows)"),
        "some lambda call_def sym does not match a df closure scope:\n{out}"
    );
}
