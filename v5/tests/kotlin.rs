//! Kotlin language support: the `ast` op dispatches :kotlin to the
//! tree-sitter-kotlin-sg grammar and the `sg` op to SupportLang::Kotlin.
//! Both ops extract from .kt files end to end.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kotlin_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .output().expect("run dl");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

const KT: &str = "\
fun alpha(x: Int): Int {
    return x + 1
}

class Greeter(val name: String) {
    fun greet() {
        println(\"hi $name\")
    }
}
";

#[test]
fn ast_op_extracts_kotlin_functions() {
    let d = sandbox("ast");
    fs::write(d.join("src/a.kt"), KT).unwrap();
    let prog = r#"
rel fn_def(name: text, path: file, line: int).
fn_def(name, path, line) <- scan("WORK","src/**/*.kt",path,rev),
  ast(path, rev, :kotlin, "(function_declaration (simple_identifier) @name)", line).
? fn_def(name, path, line).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    let names: Vec<&str> = rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert!(names.contains(&"alpha"), "found {names:?}");
    assert!(names.contains(&"greet"), "found {names:?}");
}

#[test]
fn module_graph_resolves_kotlin_imports() {
    let d = sandbox("modgraph");
    // package decl deliberately disagrees with the directory layout
    fs::create_dir_all(d.join("src/weird")).unwrap();
    fs::write(d.join("src/weird/Util.kt"), "package com.lib\n\nclass Util\nfun helper() = 1\n").unwrap();
    fs::write(d.join("src/Main.kt"),
        "package com.app\n\nimport com.lib.Util\nimport com.lib.helper\nimport java.io.File\nimport com.lib.Nope\n").unwrap();
    // scan pulls the .kt files into the engine's file set; the module graph
    // refresh then runs the Kotlin resolver over them.
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.kt", path, rev), match(path, rev, /./, line).
? module_edge(src, dst).
? module_unresolved(file, spec, reason, line).
"#;
    let recs = run_json(&d, prog);
    let edges = recs[0]["rows"].as_array().expect("rows");
    let resolved: Vec<(&str, &str)> = edges.iter()
        .map(|r| (r[0].as_str().unwrap(), r[1].as_str().unwrap())).collect();
    // Util + helper both resolve to the declaration's file; java.io is External
    // (absent), com.lib.Nope lands in module_unresolved.
    assert_eq!(resolved, vec![("src/Main.kt", "src/weird/Util.kt")], "deduped edge: {resolved:?}");
    let unresolved = recs[1]["rows"].as_array().expect("rows");
    assert_eq!(unresolved.len(), 1, "{unresolved:?}");
    assert_eq!(unresolved[0][1].as_str().unwrap(), "com.lib.Nope");
}

#[test]
fn type_edge_covers_kotlin_files() {
    let d = sandbox("typeedge");
    fs::write(d.join("src/Model.kt"), "\
package com.app

interface Pricing
abstract class Repo(val store: Store) : Base(), Pricing
class Store
open class Base
enum class Color { RED }
").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.kt", path, rev), match(path, rev, /./, line).
? type_edge(from, to, kind).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    let edges: Vec<(&str, &str, &str)> = rows.iter()
        .map(|r| (r[0].as_str().unwrap(), r[1].as_str().unwrap(), r[2].as_str().unwrap()))
        .collect();
    assert!(edges.contains(&("Repo", "Store", "field")), "{edges:?}");
    assert!(edges.contains(&("Repo", "Base", "impl")), "{edges:?}");
    assert!(edges.contains(&("Repo", "Pricing", "impl")), "{edges:?}");
    assert!(edges.contains(&("Color", "Color::RED", "variant")), "{edges:?}");
}

#[test]
fn sg_op_matches_kotlin_patterns() {
    let d = sandbox("sg");
    fs::write(d.join("src/a.kt"), KT).unwrap();
    let prog = r#"
rel hit(X: text, path: file, line: int).
hit(X, path, line) <- scan("WORK","src/**/*.kt",path,rev),
  sg(path, rev, :kotlin, "println($X)", line).
? hit(X, path, line).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "rows: {rows:?}");
    assert_eq!(rows[0][0].as_str().unwrap(), "\"hi $name\"");
    assert_eq!(rows[0][2].as_i64().unwrap(), 7, "1-based match line");
}
