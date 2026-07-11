//! `import_binding`/`import_binding_rev`: EVERY local binding an import
//! introduces, parsed off the import AST (Rust `use`, TS/JS `import`, Kotlin
//! `import`), for EVERY resolution kind — including `External` (a library
//! import), which `module_binding` deliberately skips (it only fires on a
//! resolved, non-self `File` target; see `ModuleRef::bindings` in
//! src/modgraph.rs). That gap is exactly what a mapping query needs: "which
//! library does this local name come from" almost always resolves External.
//!
//! Acceptance gate (plans/2026-07-10-mapping-feedback-import-binding.md item
//! 2): resolving which library a local name binds to must be a TWO-LINE dl
//! join. `acceptance_two_line_join_resolves_local_name_to_library` below is
//! that exact join, run against a real fixture.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("import_binding_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .current_dir(dir)
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

/// Find a row in `rows` (each a JSON array) matching every given (col index,
/// expected string) pair.
fn find_row<'a>(rows: &'a [serde_json::Value], cols: &[(usize, &str)]) -> Option<&'a serde_json::Value> {
    rows.iter().find(|row| {
        cols.iter().all(|(i, expected)| row[*i].as_str() == Some(*expected))
    })
}

// ── Rust: use..as, plain use, and pub use (reexport) ─────────────────────────

const RUST_LIB: &str = "\
mod pkg;
pub use pkg::Widget as ReexportedWidget;
use pkg::helper as helper_fn;
use pkg::Other;
use external_crate::Thing as ExternalAlias;
";

const RUST_PKG: &str = "\
pub struct Widget;
pub fn helper() -> i64 { 1 }
pub struct Other;
";

#[test]
fn rust_use_as_and_pub_use_bindings() {
    let d = sandbox("rust");
    fs::write(d.join("src/lib.rs"), RUST_LIB).unwrap();
    fs::write(d.join("src/pkg.rs"), RUST_PKG).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /./, line).
? import_binding(file, local_name, source_module, imported_name, kind).
? module_binding(file, local, source, dst).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");

    // `pub use pkg::Widget as ReexportedWidget;` — a re-export, not a plain use.
    let reexport = find_row(rows, &[(0, "src/lib.rs"), (1, "ReexportedWidget")])
        .unwrap_or_else(|| panic!("no ReexportedWidget row: {rows:?}"));
    assert_eq!(reexport[2].as_str().unwrap(), "pkg::Widget");
    assert_eq!(reexport[3].as_str().unwrap(), "Widget");
    assert_eq!(reexport[4].as_str().unwrap(), "reexport");

    // `use pkg::helper as helper_fn;` — aliased, plain named.
    let aliased = find_row(rows, &[(0, "src/lib.rs"), (1, "helper_fn")])
        .unwrap_or_else(|| panic!("no helper_fn row: {rows:?}"));
    assert_eq!(aliased[2].as_str().unwrap(), "pkg::helper");
    assert_eq!(aliased[3].as_str().unwrap(), "helper");
    assert_eq!(aliased[4].as_str().unwrap(), "named");

    // `use pkg::Other;` — unaliased: local_name == imported_name.
    let plain = find_row(rows, &[(0, "src/lib.rs"), (1, "Other")])
        .unwrap_or_else(|| panic!("no Other row: {rows:?}"));
    assert_eq!(plain[2].as_str().unwrap(), "pkg::Other");
    assert_eq!(plain[3].as_str().unwrap(), "Other");
    assert_eq!(plain[4].as_str().unwrap(), "named");

    // `use external_crate::Thing as ExternalAlias;` — resolves External (no
    // such crate in this fileset): the case module_binding CANNOT see.
    let external = find_row(rows, &[(0, "src/lib.rs"), (1, "ExternalAlias")])
        .unwrap_or_else(|| panic!("no ExternalAlias row: {rows:?}"));
    assert_eq!(external[2].as_str().unwrap(), "external_crate::Thing");
    assert_eq!(external[3].as_str().unwrap(), "Thing");
    assert_eq!(external[4].as_str().unwrap(), "named");

    let legacy_bindings = recs[1]["rows"].as_array().expect("rows");
    assert!(
        !legacy_bindings.iter().any(|r| r[1].as_str() == Some("ExternalAlias")),
        "module_binding must NOT carry the external alias (File-target only): {legacy_bindings:?}"
    );
}

// ── TS/JS: named, default, namespace, side-effect ────────────────────────────

const TS_APP: &str = "\
import myAlias from \"left-pad\";
import { readFile } from \"fs\";
import { existsSync as fileExists } from \"fs\";
import * as pathlib from \"path\";
import \"./polyfill\";
";

const JS_POLYFILL: &str = "\
import { helper } from \"./helper.js\";
helper();
";

const JS_HELPER: &str = "\
export function helper() {}
";

#[test]
fn ts_named_default_namespace_side_effect_bindings() {
    let d = sandbox("ts");
    fs::write(d.join("src/app.ts"), TS_APP).unwrap();
    fs::write(d.join("src/polyfill.js"), JS_POLYFILL).unwrap();
    fs::write(d.join("src/helper.js"), JS_HELPER).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.ts", path, rev), match(path, rev, /./, line).
seen(path) <- scan("WORK", "src/**/*.js", path, rev), match(path, rev, /./, line).
? import_binding(file, local_name, source_module, imported_name, kind).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");

    let default_row = find_row(rows, &[(0, "src/app.ts"), (1, "myAlias")])
        .unwrap_or_else(|| panic!("no default-import row: {rows:?}"));
    assert_eq!(default_row[2].as_str().unwrap(), "left-pad");
    assert_eq!(default_row[3].as_str().unwrap(), "default");
    assert_eq!(default_row[4].as_str().unwrap(), "default");

    let named_plain = find_row(rows, &[(0, "src/app.ts"), (1, "readFile")])
        .unwrap_or_else(|| panic!("no plain named-import row: {rows:?}"));
    assert_eq!(named_plain[2].as_str().unwrap(), "fs");
    assert_eq!(named_plain[3].as_str().unwrap(), "readFile");
    assert_eq!(named_plain[4].as_str().unwrap(), "named");

    let named_aliased = find_row(rows, &[(0, "src/app.ts"), (1, "fileExists")])
        .unwrap_or_else(|| panic!("no aliased named-import row: {rows:?}"));
    assert_eq!(named_aliased[2].as_str().unwrap(), "fs");
    assert_eq!(named_aliased[3].as_str().unwrap(), "existsSync");
    assert_eq!(named_aliased[4].as_str().unwrap(), "named");

    let namespace_row = find_row(rows, &[(0, "src/app.ts"), (1, "pathlib")])
        .unwrap_or_else(|| panic!("no namespace-import row: {rows:?}"));
    assert_eq!(namespace_row[2].as_str().unwrap(), "path");
    assert_eq!(namespace_row[3].as_str().unwrap(), "*");
    assert_eq!(namespace_row[4].as_str().unwrap(), "namespace");

    let side_effect_row = find_row(rows, &[(0, "src/app.ts"), (2, "./polyfill"), (4, "side_effect")])
        .unwrap_or_else(|| panic!("no side-effect-import row: {rows:?}"));
    assert_eq!(side_effect_row[1].as_str().unwrap(), "");
    assert_eq!(side_effect_row[3].as_str().unwrap(), "");

    // .js is parsed too: polyfill.js's own named import of helper.js.
    let js_named = find_row(rows, &[(0, "src/polyfill.js"), (1, "helper")])
        .unwrap_or_else(|| panic!("no .js named-import row: {rows:?}"));
    assert_eq!(js_named[2].as_str().unwrap(), "./helper.js");
    assert_eq!(js_named[3].as_str().unwrap(), "helper");
    assert_eq!(js_named[4].as_str().unwrap(), "named");
}

// ── Kotlin: import..as ───────────────────────────────────────────────────────

const KT_MAIN: &str = "\
package com.app

import com.app.util.Helper as HelperAlias
import com.app.util.Widget
import com.external.Thing as ExtAlias

fun main() {}
";

const KT_UTIL: &str = "\
package com.app.util

class Helper
class Widget
";

#[test]
fn kotlin_import_as_bindings() {
    let d = sandbox("kotlin");
    fs::create_dir_all(d.join("src/main/kotlin/com/app/util")).unwrap();
    fs::write(d.join("src/main/kotlin/com/app/Main.kt"), KT_MAIN).unwrap();
    fs::write(d.join("src/main/kotlin/com/app/util/Util.kt"), KT_UTIL).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.kt", path, rev), match(path, rev, /./, line).
? import_binding(file, local_name, source_module, imported_name, kind).
? module_binding(file, local, source, dst).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    let file = "src/main/kotlin/com/app/Main.kt";

    let aliased = find_row(rows, &[(0, file), (1, "HelperAlias")])
        .unwrap_or_else(|| panic!("no HelperAlias row: {rows:?}"));
    assert_eq!(aliased[2].as_str().unwrap(), "com.app.util.Helper");
    assert_eq!(aliased[3].as_str().unwrap(), "Helper");
    assert_eq!(aliased[4].as_str().unwrap(), "named");

    let plain = find_row(rows, &[(0, file), (1, "Widget")])
        .unwrap_or_else(|| panic!("no Widget row: {rows:?}"));
    assert_eq!(plain[2].as_str().unwrap(), "com.app.util.Widget");
    assert_eq!(plain[3].as_str().unwrap(), "Widget");
    assert_eq!(plain[4].as_str().unwrap(), "named");

    // External resolution (no such package in this fileset): module_binding
    // cannot see this one, import_binding does.
    let external = find_row(rows, &[(0, file), (1, "ExtAlias")])
        .unwrap_or_else(|| panic!("no ExtAlias row: {rows:?}"));
    assert_eq!(external[2].as_str().unwrap(), "com.external.Thing");
    assert_eq!(external[3].as_str().unwrap(), "Thing");
    assert_eq!(external[4].as_str().unwrap(), "named");

    let legacy_bindings = recs[1]["rows"].as_array().expect("rows");
    assert!(
        !legacy_bindings.iter().any(|r| r[1].as_str() == Some("ExtAlias")),
        "module_binding must NOT carry the external alias: {legacy_bindings:?}"
    );
}

// ── Acceptance gate: the two-line join ───────────────────────────────────────

/// The exact join from plans/2026-07-10-mapping-feedback-import-binding.md
/// item 2: "resolving which library any local name binds to must be a
/// TWO-LINE join". `binds_lib` is that join; the point query then answers
/// "myAlias comes from which library" in one shot, over a real fixture
/// (a TS default import of an npm-shaped specifier that resolves External).
#[test]
fn acceptance_two_line_join_resolves_local_name_to_library() {
    let d = sandbox("acceptance");
    fs::write(d.join("src/app.ts"), "import myAlias from \"left-pad\";\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.ts", path, rev), match(path, rev, /./, line).

rel binds_lib(local_name: text, source_module: text).
binds_lib(local_name, source_module) <- import_binding(file, local_name, source_module, _, _).

? binds_lib("myAlias", lib).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "exactly one library binds myAlias: {rows:?}");
    // The point-bound literal ("myAlias") drops out of the projection; the
    // single unbound var (`lib`) is the only output column.
    assert_eq!(rows[0][0].as_str().unwrap(), "left-pad");
}
