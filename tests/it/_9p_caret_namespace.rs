//! `(^User: (* ..)): (* ..)` end to end (AGENTS.md caret 3 row): the macro emits
//! `(: User (* ..))` and `(: User () (* ..))`; the body's rule reads `User` rows from
//! inside the namespace, an outside rule reads `User.greet` by dot.

use super::_9e_generic_rules::{render_row, ENGINES};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn compile(fixture: &str, engine: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/caret").join(fixture))
        .current_dir(root)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    serde_json::from_slice(&compiled.stdout)
        .unwrap_or_else(|_| panic!("{engine}: {}", String::from_utf8_lossy(&compiled.stderr)))
}

/// Rendered compiler rows whose text holds one of `needles`, sorted.
fn select(program: &Value, needles: &[&str]) -> Vec<String> {
    let names = &program["program"]["names"];
    let rendered: BTreeSet<String> = program["compiler_rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| render_row(row, names))
        .filter(|row| needles.iter().any(|needle| row.contains(needle)))
        .collect();
    rendered.into_iter().collect()
}

#[test]
fn caret_body_rule_and_dot_path_read_the_namespace() {
    for engine in ENGINES {
        let program = compile("0_namespace.dl7", engine);
        assert_eq!(program["diagnostics"], json!([]), "{engine}");
        let rows = select(&program, &["ada"]);
        // `User.greet` has no entry in `names`, so its owner node renders.
        let expected = vec![
            "User: const(\"ada\")",
            "greeting: const(\"hi ada\")",
            "owner(file, expansion_node(reader_node(44), infix_colon, 1, 18)): const(\"ada\"), const(\"hi ada\")",
        ];
        assert_eq!(rows, expected, "{engine}");
    }
}
