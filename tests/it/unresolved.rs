//! `unresolved(file, line, reason, detail)` — an edge that COULD exist but
//! whose target is computed at runtime, as opposed to `module_unresolved`'s
//! "no edge exists at all" case. Own builtin family (rides the oxc TS
//! front-end; see `Engine::refresh_unresolved_rel`), TS/TSX/JS/JSX/MJS/CJS
//! only in v1. Closed v1 reason vocabulary: `dynamic-import`,
//! `computed-member-call`, `spread-call-args`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `dl`'s text query output is `? rel => col\tcol...` then one data row per
/// line (no indent — column 0 is the first cell), then a `  (N rows)`
/// footer — pull out just the data rows, tab-split.
fn data_rows(out: &str) -> Vec<Vec<String>> {
    out.lines()
        .filter(|l| !l.is_empty() && !l.starts_with("? ") && !l.trim_start().starts_with('('))
        .map(|l| l.trim().split('\t').map(|s| s.to_string()).collect())
        .collect()
}

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("unresolved_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

const SEEN_TS: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.ts", path, rev).
"#;

/// ACCEPT: a template-built dynamic `import()` specifier is flagged
/// `dynamic-import`, with the exact interpolated source text as `detail`.
#[test]
fn dynamic_import_expr_is_flagged() {
    let dir = sandbox("dynamic_import");
    fs::write(dir.join("src/loader.ts"),
        "export async function loadPlugin(pluginName: string) {\n\
         \x20 return import(`./plugins/${pluginName}`);\n\
         }\n").unwrap();
    let prog = format!("{SEEN_TS}? unresolved(file, line, reason, detail).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0][0], "src/loader.ts");
    assert_eq!(rows[0][2], "dynamic-import");
    assert_eq!(rows[0][3], "`./plugins/${pluginName}`");
}

/// ACCEPT: a `require(expr)` call with a non-literal argument is also
/// `dynamic-import` (the CJS twin of the ES `import(expr)` case).
#[test]
fn require_with_computed_argument_is_flagged() {
    let dir = sandbox("require_dynamic");
    fs::write(dir.join("src/loader.js"),
        "function loadModule(moduleName) {\n\
         \x20 return require(moduleName);\n\
         }\n").unwrap();
    let prog = format!(
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.js\", path, rev).\n\
         ? unresolved(file, line, reason, detail).\n"
    );
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0][2], "dynamic-import");
    assert_eq!(rows[0][3], "moduleName");
}

/// ACCEPT: `require("literal")` is a plain static dependency, NOT flagged —
/// only a computed argument counts.
#[test]
fn require_with_string_literal_is_not_flagged() {
    let dir = sandbox("require_static");
    fs::write(dir.join("src/loader.js"),
        "const fs = require(\"fs\");\n").unwrap();
    let prog = format!(
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.js\", path, rev).\n\
         ? unresolved(file, line, reason, detail).\n"
    );
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    assert_eq!(data_rows(&out).len(), 0, "{out}");
}

/// ACCEPT: `handlers[eventType]()` — a computed-member call site — is
/// flagged `computed-member-call`, with the exact callee expression text.
#[test]
fn computed_member_call_is_flagged() {
    let dir = sandbox("computed_member");
    fs::write(dir.join("src/dispatch.ts"),
        "export function dispatch(eventType: string, handlers: Record<string, () => void>) {\n\
         \x20 handlers[eventType]();\n\
         }\n").unwrap();
    let prog = format!("{SEEN_TS}? unresolved(file, line, reason, detail).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0][2], "computed-member-call");
    assert_eq!(rows[0][3], "handlers[eventType]");
}

/// ACCEPT: `notify(...listeners)` — a spread call argument — is flagged
/// `spread-call-args`, with the exact spread expression text (`...` incl.).
#[test]
fn spread_call_argument_is_flagged() {
    let dir = sandbox("spread_args");
    fs::write(dir.join("src/notify.ts"),
        "declare function notify(...args: unknown[]): void;\n\
         export function broadcast(listeners: unknown[]) {\n\
         \x20 notify(...listeners);\n\
         }\n").unwrap();
    let prog = format!("{SEEN_TS}? unresolved(file, line, reason, detail).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0][2], "spread-call-args");
    assert_eq!(rows[0][3], "...listeners");
}

/// ACCEPT: a plain statically-resolvable call site (bare identifier callee,
/// no spread args) produces zero `unresolved` rows.
#[test]
fn plain_call_is_not_flagged() {
    let dir = sandbox("plain_call");
    fs::write(dir.join("src/plain.ts"),
        "function greet(name: string) { return name; }\n\
         export function run() { return greet(\"world\"); }\n").unwrap();
    let prog = format!("{SEEN_TS}? unresolved(file, line, reason, detail).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    assert_eq!(data_rows(&out).len(), 0, "{out}");
}

/// REJECT: `rel unresolved(...)` is a reserved built-in name — the engine
/// bails with a named diagnostic, not a silent no-op.
#[test]
fn rel_decl_of_unresolved_bails() {
    let dir = sandbox("reserved");
    let prog = "rel unresolved(file: text, line: int, reason: text, detail: text).\n";
    let (code, _out, err) = run(&dir, prog);
    assert_ne!(code, 0, "reserved-name decl must fail");
    assert!(err.contains("reserved-name") && err.contains("relation `unresolved`")
        && err.contains("pick another name"), "{err}");
}
