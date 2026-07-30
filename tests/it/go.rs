//! Go language support (the syntactic "diet SCIP" tier): GoTypes (type_entity/
//! type_edge/type_sig/call_*/df_*/doc_comment) + GoResolver (module_edge via
//! go.mod + one-package-per-directory). Mirrors tests/it/kotlin.rs's shape.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("go_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("pkg/store")).unwrap();
    dir
}

/// Empty config via `$SPREFA_CONFIG`, so an ambient
/// `~/.config/sprefa/config.toml` never joins the sandbox's file set (same
/// hermeticity rationale as `resolver_import_narrowing.rs::empty_config`).
fn empty_config(dir: &PathBuf) -> PathBuf {
    let cfg = dir.join("empty.toml");
    fs::write(&cfg, "# no repos\n").unwrap();
    cfg
}

fn run_json(dir: &PathBuf, prog: &str) -> Vec<serde_json::Value> {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let cfg = empty_config(dir);
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--no-daemon",
            "--db",
            dir.join("db").to_str().unwrap(),
            "--query-json",
        ])
        .env("SPREFA_CONFIG", cfg)
        .current_dir(dir)
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

const STORE_GO: &str = "\
package store

// Store holds pricing data.
type Store struct {
\tHost string
\tPort int
\tMeta Metadata
}

// Metadata carries extra store attributes.
type Metadata struct {
\tRegion string
}

// Pricing describes something priced.
type Pricing interface {
\tPrice() int
}

// NewStore builds a Store from a host name.
func NewStore(host string) Store {
\treturn Store{Host: host, Port: 8080}
}

// Name returns the display name.
func (s *Store) Name() string {
\treturn s.Host
}
";

#[test]
fn ast_op_extracts_go_functions() {
    let d = sandbox("ast");
    fs::write(d.join("pkg/store/store.go"), STORE_GO).unwrap();
    let prog = r#"
rel fn_def(name: text, path: file, line: int).
fn_def(name, path, line) <- scan("WORK","pkg/**/*.go",path,rev),
  ast(path, rev, :go, "(function_declaration name: (identifier) @name)", line).
? fn_def(name, path, line).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    let names: Vec<&str> = rows.iter().map(|r| r[0].as_str().unwrap()).collect();
    assert!(names.contains(&"NewStore"), "found {names:?}");
}

#[test]
fn sg_op_matches_go_patterns() {
    let d = sandbox("sg");
    fs::write(d.join("pkg/store/store.go"), STORE_GO).unwrap();
    let prog = r#"
rel hit(path: file, line: int).
hit(path, line) <- scan("WORK","pkg/**/*.go",path,rev),
  sg(path, rev, :go, "package store", line).
? hit(path, line).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "rows: {rows:?}");
    assert_eq!(rows[0][1].as_i64().unwrap(), 1, "1-based match line");
}

#[test]
fn type_graph_covers_go_struct_interface_and_methods() {
    let d = sandbox("typegraph");
    fs::write(d.join("pkg/store/store.go"), STORE_GO).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "pkg/**/*.go", path, rev), match(path, rev, /./, line).
? type_entity(sym, name, kind, parent, file, line).
? type_edge(from, to, kind, _).
? doc_comment(repo, sym, line, text).
"#;
    let recs = run_json(&d, prog);
    let entities = recs[0]["rows"].as_array().expect("rows");
    let kinds: Vec<(&str, &str)> = entities
        .iter()
        .map(|r| (r[1].as_str().unwrap(), r[2].as_str().unwrap()))
        .collect();
    assert!(kinds.contains(&("Store", "struct")), "{kinds:?}");
    assert!(kinds.contains(&("Pricing", "interface")), "{kinds:?}");
    assert!(kinds.contains(&("NewStore", "function")), "{kinds:?}");
    assert!(kinds.contains(&("Name", "method")), "{kinds:?}");
    // the Name method's parent joins Store's own entity sym.
    let store_sym = entities
        .iter()
        .find(|r| r[1].as_str() == Some("Store"))
        .unwrap()[0]
        .as_str()
        .unwrap()
        .to_string();
    let name_parent = entities
        .iter()
        .find(|r| r[1].as_str() == Some("Name"))
        .unwrap()[3]
        .as_str()
        .unwrap();
    assert_eq!(name_parent, store_sym);

    let edges = recs[1]["rows"].as_array().expect("rows");
    let field_edges: Vec<(&str, &str, &str)> = edges
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap(),
                r[1].as_str().unwrap(),
                r[2].as_str().unwrap(),
            )
        })
        .collect();
    // Host/Port are builtin-typed (string/int), no ref; Meta is a named type.
    assert!(
        field_edges.contains(&("Store", "Metadata", "field")),
        "{field_edges:?}"
    );

    let docs = recs[2]["rows"].as_array().expect("rows");
    assert!(
        docs.iter()
            .any(|r| r[3].as_str().unwrap().contains("Store holds pricing data.")),
        "{docs:?}"
    );
}

#[test]
fn call_graph_resolves_go_calls() {
    let d = sandbox("callgraph");
    fs::write(d.join("pkg/store/store.go"), STORE_GO).unwrap();
    fs::write(
        d.join("main.go"),
        "\
package main

import \"example.com/app/pkg/store\"

func main() {
\ts := store.NewStore(\"local\")
\t_ = s.Name()
}
",
    )
    .unwrap();
    fs::write(d.join("go.mod"), "module example.com/app\n\ngo 1.22\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.go", path, rev), match(path, rev, /./, line).
? call_edge(caller, callee).
? module_edge(src, dst).
"#;
    let recs = run_json(&d, prog);
    let calls = recs[0]["rows"].as_array().expect("rows");
    let call_pairs: Vec<(&str, &str)> = calls
        .iter()
        .map(|r| (r[0].as_str().unwrap(), r[1].as_str().unwrap()))
        .collect();
    assert!(
        call_pairs
            .iter()
            .any(|(_, callee)| callee.contains("NewStore")),
        "{call_pairs:?}"
    );

    let modules = recs[1]["rows"].as_array().expect("rows");
    let mod_pairs: Vec<(&str, &str)> = modules
        .iter()
        .map(|r| (r[0].as_str().unwrap(), r[1].as_str().unwrap()))
        .collect();
    assert!(
        mod_pairs.contains(&("main.go", "pkg/store/store.go")),
        "{mod_pairs:?}"
    );
}

#[test]
fn module_graph_resolves_aliased_go_import() {
    let d = sandbox("modbinding");
    fs::write(d.join("pkg/store/store.go"), STORE_GO).unwrap();
    fs::write(
        d.join("main.go"),
        "\
package main

import s \"example.com/app/pkg/store\"

func main() {
\t_ = s.NewStore(\"local\")
}
",
    )
    .unwrap();
    fs::write(d.join("go.mod"), "module example.com/app\n\ngo 1.22\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.go", path, rev), match(path, rev, /./, line).
? module_binding_resolved(file, local, source, dst).
"#;
    let recs = run_json(&d, prog);
    let rows = recs[0]["rows"].as_array().expect("rows");
    let bindings: Vec<(&str, &str, &str, &str)> = rows
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap(),
                r[1].as_str().unwrap(),
                r[2].as_str().unwrap(),
                r[3].as_str().unwrap(),
            )
        })
        .collect();
    assert!(
        bindings.contains(&("main.go", "s", "store", "pkg/store/store.go")),
        "{bindings:?}"
    );
}
