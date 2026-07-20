//! Python diet-tier extraction end to end: `PyTypes` (type_entity/type_edge/
//! call_*/df_*/doc_comment, index-free) + `PyResolver` (module_binding_resolved) over a
//! small fixture — a class with methods + docstrings, a `from`-import alias,
//! a capitalized constructor call, and a list comprehension. Mirrors
//! tests/it/kotlin.rs's shape.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("python_{tag}"));
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

const MAIN_PY: &str = "\
\"\"\"Petshop module.\"\"\"


class Animal:
    \"\"\"A pet.\"\"\"

    def __init__(self, name):
        self.name = name

    def speak(self):
        return greet(self.name)


def greet(name):
    return \"hi \" + name


def build_zoo(names):
    animals = [Animal(n) for n in names]
    return animals
";

const HELPER_PY: &str = "\
from main import Animal as Pet


def make_pet(name):
    return Pet(name)
";

#[test]
fn type_entity_and_doc_comment_cover_class_method_and_docstring() {
    let d = sandbox("entities");
    fs::write(d.join("src/main.py"), MAIN_PY).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.py", path, rev), match(path, rev, /./, line).
? type_entity(repo, sym, name, kind, parent, file, line).
? doc_comment(repo, sym, line, text).
"#;
    let recs = run_json(&d, prog);
    let entities = recs[0]["rows"].as_array().expect("rows");
    let by_name = |name: &str| -> Vec<&serde_json::Value> {
        entities.iter().filter(|r| r[2].as_str() == Some(name)).collect()
    };
    let animal = by_name("Animal");
    assert_eq!(animal.len(), 1, "{entities:?}");
    assert_eq!(animal[0][3].as_str().unwrap(), "class");
    assert!(animal[0][4].is_null() || animal[0][4].as_str() == Some(""), "class has no parent: {:?}", animal[0]);
    let speak = by_name("speak");
    assert_eq!(speak.len(), 1, "{entities:?}");
    assert_eq!(speak[0][3].as_str().unwrap(), "method");
    let animal_sym = animal[0][1].as_str().unwrap();
    assert_eq!(speak[0][4].as_str().unwrap(), animal_sym, "method's parent joins the class sym");
    let greet = by_name("greet");
    assert_eq!(greet.len(), 1, "{entities:?}");
    assert_eq!(greet[0][3].as_str().unwrap(), "function");

    let docs = recs[1]["rows"].as_array().expect("rows");
    assert!(docs.iter().any(|d| d[1].as_str() == Some(animal_sym) && d[3].as_str() == Some("A pet.")), "{docs:?}");
}

#[test]
fn module_binding_resolved_captures_from_import_alias() {
    let d = sandbox("binding");
    fs::write(d.join("src/main.py"), MAIN_PY).unwrap();
    fs::write(d.join("src/helper.py"), HELPER_PY).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.py", path, rev), match(path, rev, /./, line).
? module_binding_resolved(file, local, source, dst).
? module_edge(src, dst).
"#;
    let recs = run_json(&d, prog);
    let bindings = recs[0]["rows"].as_array().expect("rows");
    assert!(
        bindings.iter().any(|r| r[0].as_str() == Some("src/helper.py")
            && r[1].as_str() == Some("Pet")
            && r[2].as_str() == Some("Animal")
            && r[3].as_str() == Some("src/main.py")),
        "{bindings:?}"
    );
    let edges = recs[1]["rows"].as_array().expect("rows");
    assert!(
        edges.iter().any(|r| r[0].as_str() == Some("src/helper.py") && r[1].as_str() == Some("src/main.py")),
        "{edges:?}"
    );
}

#[test]
fn call_edge_resolves_and_ctor_df_node_present_with_comprehension_loop() {
    let d = sandbox("calls_flow");
    fs::write(d.join("src/main.py"), MAIN_PY).unwrap();
    let prog = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.py", path, rev), match(path, rev, /./, line).
? call_edge(caller, callee, kind).
? df_node(id, kind, var, fn, file, line, _).
? loop_over(file, start, end, var, collection, fn_sym).
"#;
    let recs = run_json(&d, prog);
    let edges = recs[0]["rows"].as_array().expect("rows");
    // `speak` calls the free function `greet`, unique in this small file set.
    assert!(
        edges.iter().any(|r| r[0].as_str().is_some_and(|s| s.contains("speak")) && r[1].as_str().is_some_and(|s| s.contains("greet"))),
        "{edges:?}"
    );

    let nodes = recs[1]["rows"].as_array().expect("rows");
    // `Animal(n)` inside the comprehension mints a `new` df_node named Animal.
    assert!(
        nodes.iter().any(|r| r[1].as_str() == Some("new") && r[2].as_str() == Some("Animal")),
        "{nodes:?}"
    );

    let loops = recs[2]["rows"].as_array().expect("rows");
    // the list comprehension records its own loop span with loop var `n`.
    assert!(loops.iter().any(|r| r[3].as_str() == Some("n")), "{loops:?}");
}
