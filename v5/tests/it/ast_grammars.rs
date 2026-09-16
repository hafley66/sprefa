//! Raw `ast` op grammar coverage: each language dispatches through
//! engine::ts_lang to its tree-sitter grammar and binds a capture end to end.
//! python/go close the ast-vs-sg gap; bash/hcl/starlark/jsonnet are new.
//! Mirrors tests/kotlin.rs.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ast_grammars_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap(), "--query-json"])
        .output()
        .expect("run dl");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad json line `{l}`: {e}")))
        .collect()
}

/// Write one fixture, run one `ast(:lang, query)` rule, return the bound col-0
/// strings.
fn capture_names(
    tag: &str,
    file: &str,
    content: &str,
    glob: &str,
    lang: &str,
    query: &str,
) -> Vec<String> {
    let d = sandbox(tag);
    fs::write(d.join(file), content).unwrap();
    let prog = format!(
        r#"
rel hit(name: text, path: file, line: int).
hit(name, path, line) <- scan("WORK","{glob}",path,rev),
  ast(path, rev, :{lang}, "{query}", line).
? hit(name, path, line).
"#
    );
    let recs = run_json(&d, &prog);
    recs[0]["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| r[0].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn ast_python_function_defs() {
    let names = capture_names(
        "python",
        "src/a.py",
        "def alpha(x):\n    return x + 1\n\ndef beta():\n    pass\n",
        "src/**/*.py",
        "python",
        "(function_definition name: (identifier) @name)",
    );
    assert!(names.contains(&"alpha".to_string()), "found {names:?}");
    assert!(names.contains(&"beta".to_string()), "found {names:?}");
}

#[test]
fn ast_bash_function_defs() {
    let names = capture_names(
        "bash",
        "src/a.sh",
        "greet() {\n  echo hi\n}\n\nfunction farewell {\n  echo bye\n}\n",
        "src/**/*.sh",
        "bash",
        "(function_definition name: (word) @name)",
    );
    assert!(names.contains(&"greet".to_string()), "found {names:?}");
    assert!(names.contains(&"farewell".to_string()), "found {names:?}");
}

#[test]
fn ast_go_function_decls() {
    let names = capture_names(
        "go",
        "src/a.go",
        "package main\n\nfunc Alpha() int { return 1 }\n\nfunc Beta(x int) {}\n",
        "src/**/*.go",
        "go",
        "(function_declaration name: (identifier) @name)",
    );
    assert!(names.contains(&"Alpha".to_string()), "found {names:?}");
    assert!(names.contains(&"Beta".to_string()), "found {names:?}");
}

#[test]
fn ast_hcl_block_types() {
    let names = capture_names(
        "hcl", "src/main.tf",
        "resource \"aws_instance\" \"web\" {\n  ami = \"abc\"\n}\n\nvariable \"region\" {\n  default = \"us\"\n}\n",
        "src/**/*.tf", "hcl",
        "(block (identifier) @name)",
    );
    assert!(names.contains(&"resource".to_string()), "found {names:?}");
    assert!(names.contains(&"variable".to_string()), "found {names:?}");
}

#[test]
fn ast_starlark_function_defs() {
    let names = capture_names(
        "starlark",
        "src/build.bzl",
        "def my_rule(name):\n    pass\n\ndef other_rule():\n    pass\n",
        "src/**/*.bzl",
        "starlark",
        "(function_definition (identifier) @name)",
    );
    assert!(names.contains(&"my_rule".to_string()), "found {names:?}");
    assert!(names.contains(&"other_rule".to_string()), "found {names:?}");
}

#[test]
fn ast_jsonnet_function_params() {
    // jsonnet bindings: `local f(x) = ...` exposes the bound name.
    let names = capture_names(
        "jsonnet",
        "src/a.jsonnet",
        "local greet(name) = 'hi ' + name;\n{ msg: greet('world') }\n",
        "src/**/*.jsonnet",
        "jsonnet",
        "(bind (ident) @name)",
    );
    assert!(names.contains(&"greet".to_string()), "found {names:?}");
}

#[test]
fn ast_gotmpl_field_refs() {
    // go-template grammar is vendored + cc-built (no crates.io crate).
    let names = capture_names(
        "gotmpl",
        "src/page.tmpl",
        "Hello {{ .Name }}\n{{ if .Admin }}admin{{ end }}\n",
        "src/**/*.tmpl",
        "gotmpl",
        "(field (identifier) @name)",
    );
    assert!(names.contains(&"Name".to_string()), "found {names:?}");
    assert!(names.contains(&"Admin".to_string()), "found {names:?}");
}

#[test]
fn ast_dockerfile_from_images() {
    // dockerfile grammar is vendored + cc-built (crates.io crate pins ts 0.20).
    let names = capture_names(
        "dockerfile",
        "Dockerfile",
        "FROM alpine:3.18 AS build\nRUN echo hi\nFROM scratch\nCOPY . /app\n",
        "Dockerfile",
        "dockerfile",
        "(from_instruction (image_spec (image_name) @name))",
    );
    assert!(names.contains(&"alpine".to_string()), "found {names:?}");
    assert!(names.contains(&"scratch".to_string()), "found {names:?}");
}
