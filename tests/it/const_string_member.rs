//! `const_string_member(file, object, member, value)` — every flat
//! string-valued member of a `const OBJ = { ... }` object literal. Own
//! builtin family (rides the oxc TS front-end; see
//! `Engine::refresh_const_string_member_rel`), TS/TSX/JS/JSX/MJS/CJS only in
//! v1. Computed keys and non-string values (including nested object/array
//! literals) are skipped, never guessed at.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `dl`'s text query output is `? rel => col\tcol...` then one indented row
/// per line, then a `  (N rows)` footer — pull out just the indented data
/// rows, tab-split.
fn data_rows(out: &str) -> Vec<Vec<String>> {
    out.lines()
        .filter(|l| l.starts_with("  ") && !l.trim_start().starts_with('('))
        .map(|l| l.trim().split('\t').map(|s| s.to_string()).collect())
        .collect()
}

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("const_string_member_{tag}"));
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

/// ACCEPT: a route lookup table's flat string members all show up, keyed by
/// the const's own binding name.
#[test]
fn route_table_members_are_discovered() {
    let dir = sandbox("routes");
    fs::write(dir.join("src/routes.ts"),
        "export const ROUTES = {\n\
         \x20 home: \"/\",\n\
         \x20 user: \"/u/:id\",\n\
         };\n").unwrap();
    let prog = format!("{SEEN_TS}? const_string_member(file, object, member, value).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");

    let mut rows = data_rows(&out);
    rows.sort_by(|a, b| a[2].cmp(&b[2]));
    assert_eq!(rows.len(), 2, "{out}");
    assert_eq!(rows[0], vec!["src/routes.ts", "ROUTES", "home", "/"]);
    assert_eq!(rows[1], vec!["src/routes.ts", "ROUTES", "user", "/u/:id"]);
}

/// ACCEPT (the two-line-join criterion, per the mapping-feedback plan):
/// resolving "which route string does key X map to" is exactly a two-line
/// dl join over `const_string_member`.
#[test]
fn resolving_a_route_key_is_a_two_line_join() {
    let dir = sandbox("two_line_join");
    fs::write(dir.join("src/routes.ts"),
        "export const ROUTES = { home: \"/\", user: \"/u/:id\" };\n").unwrap();
    let prog = format!(
        "{SEEN_TS}\
         rel route_for(routeKey: text, routePath: text).\n\
         route_for(routeKey, routePath) <- const_string_member(_, \"ROUTES\", routeKey, routePath).\n\
         ? route_for(\"user\", routePath).\n"
    );
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert_eq!(rows[0][0], "/u/:id");
}

/// ACCEPT: `export const` and `as const` both discover the same shape; a
/// computed key, a nested object value, and a non-string value are all
/// skipped rather than guessed at.
#[test]
fn export_and_as_const_wrappers_unwrap_and_non_string_members_skip() {
    let dir = sandbox("wrappers");
    let computed_key = "eventType";
    fs::write(dir.join("src/mixed.ts"), format!(
        "export const STATUS = {{\n\
         \x20 ok: \"OK\",\n\
         \x20 nested: {{ inner: \"skipped\" }},\n\
         \x20 count: 42,\n\
         \x20 [{computed_key}]: \"also skipped\",\n\
         }} as const;\n"
    )).unwrap();
    let prog = format!("{SEEN_TS}? const_string_member(file, object, member, value).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "stderr:\n{err}");
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 1, "{out}: only the flat string member survives");
    assert_eq!(rows[0], vec!["src/mixed.ts", "STATUS", "ok", "OK"]);
}

/// REJECT: `rel const_string_member(...)` is a reserved built-in name — the
/// engine bails with a named diagnostic, not a silent no-op.
#[test]
fn rel_decl_of_const_string_member_bails() {
    let dir = sandbox("reserved");
    let prog = "rel const_string_member(file: text, object: text, member: text, value: text).\n";
    let (code, _out, err) = run(&dir, prog);
    assert_ne!(code, 0, "reserved-name decl must fail");
    assert!(err.contains("built-in const-string-member relation") && err.contains("const_string_member"),
        "{err}");
}
