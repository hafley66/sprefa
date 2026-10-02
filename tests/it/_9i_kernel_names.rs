//! Compiler-generated goals call kernel and prelude relations by identity
//! (plans/v8/2026-10-02-kernel-by-identity.brief.md): a product named after one
//! leaves every other compiler row as it was.

use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::_9e_generic_rules::ENGINES;

/// Kernel and prelude relations generated goals call. `:` has no declaration
/// syntax, so it stays out.
const SHADOWS: [&str; 17] = [
    "tuple",
    "intern",
    "intern_snapshot",
    "node",
    "module",
    "product",
    "sum",
    "edge_snapshot",
    "edge_ref",
    "obligation",
    "discharged",
    "Extends",
    "checker_of",
    "Apply",
    "PartialCall",
    "Literal",
    "Curry",
];

fn compile(path: &Path, engine: &str) -> Value {
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(path)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{engine}: {}",
        String::from_utf8_lossy(&compiled.stdout)
    );
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]), "{engine}");
    program
}

/// The node a top-level declaration of `name` in `module` binds.
fn declared(rows: &[Value], module: &Value, name: &str) -> Value {
    let owner = json!({"f": "ref", "args": [module]});
    let label = json!({"f": "const", "args": [{"a": name}]});
    rows.iter()
        .find_map(|row| {
            let args = row["args"][1].as_array()?;
            let colon = row["args"][0] == json!({"f": "ref", "args": [{"f": "kernel", "args": [{"a": ":"}]}]});
            (colon && args[0] == owner && args[1] == label).then(|| args[2].clone())
        })
        .unwrap_or_else(|| panic!("no declaration of {name}"))
}

/// Compiler rows as text with the source path written `FILE`, minus every row
/// that mentions one of `skip`. The module's own `:` rows drop their ordinal: the
/// prelude aliases number after the program's declarations.
fn rows(program: &Value, path: &Path, skip: &[Value]) -> BTreeSet<String> {
    let skip: Vec<String> = skip.iter().map(Value::to_string).collect();
    let file = path.to_str().unwrap();
    let module = json!({"f": "ref", "args": [{"f": "module", "args": [{"f": "file", "args": [{"a": file}]}]}]});
    program["compiler_rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| !skip.iter().any(|node| row.to_string().contains(node.as_str())))
        .map(|row| {
            let mut row = row.clone();
            if row["args"][1][0] == module && row["args"][1].as_array().is_some_and(|args| args.len() == 4) {
                row["args"][1][3] = Value::Null;
            }
            row.to_string().replace(file, "FILE")
        })
        .collect()
}

fn scratch() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join(format!("target/kernel-names-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn products_named_after_kernel_and_prelude_relations_leave_builds_alone() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("fixtures/generic/10_kernel_names.dl7")).unwrap();
    let mut shadowed = source.clone();
    for name in SHADOWS {
        shadowed.push_str(&format!("\n({name}:\n   (* (shadow: int)))\n"));
    }
    let directory = scratch();
    let plain_path = directory.join("plain.dl7");
    let shadowed_path = directory.join("shadowed.dl7");
    std::fs::write(&plain_path, source).unwrap();
    std::fs::write(&shadowed_path, shadowed).unwrap();
    for engine in ENGINES {
        let plain = compile(&plain_path, engine);
        let with = compile(&shadowed_path, engine);
        let module = json!({"f": "module", "args": [{"f": "file", "args": [{"a": shadowed_path.to_str().unwrap()}]}]});
        let nodes: Vec<Value> = SHADOWS
            .iter()
            .map(|name| declared(with["compiler_rows"].as_array().unwrap(), &module, name))
            .collect();
        // The module's binding of each name moves from the prelude alias to the
        // declaration; those rows name it as `const(Name)`.
        let names: Vec<Value> =
            SHADOWS.iter().map(|name| json!({"f": "const", "args": [{"a": name}]})).collect();
        let expected = rows(&plain, &plain_path, &names);
        let mut skip = nodes;
        skip.extend(names);
        let got = rows(&with, &shadowed_path, &skip);
        assert!(expected.iter().any(|row| row.contains("\"application\"")), "{engine}");
        let missing: Vec<&String> = expected.difference(&got).collect();
        let extra: Vec<&String> = got.difference(&expected).collect();
        assert!(missing.is_empty() && extra.is_empty(), "{engine}\nmissing {missing:#?}\nextra {extra:#?}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}

/// Generated goals never build `name(Owner, Label)`: that resolves in the
/// caller's scope, where a program's declarations capture it. Source-written
/// names go through `forms::written_name`, the one constructor of `name/2`.
#[test]
fn only_written_name_constructs_a_name_term() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut level = vec![root];
    let mut sites = Vec::new();
    for _ in 0..SOURCE_DEPTH {
        let mut next = Vec::new();
        for directory in level {
            for entry in std::fs::read_dir(&directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    next.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (line, content) in text.lines().enumerate() {
                    if content.contains("compound(\"name\"") {
                        sites.push(format!("{}:{}", path.display(), line + 1));
                    }
                }
            }
        }
        level = next;
    }
    assert!(level.is_empty(), "src/ is deeper than {SOURCE_DEPTH}");
    assert_eq!(sites.len(), 1, "{sites:#?}");
    assert!(sites[0].contains("_2_lower/_1_forms.rs"), "{sites:#?}");
}

/// Directory nesting under `src/`.
const SOURCE_DEPTH: usize = 8;
