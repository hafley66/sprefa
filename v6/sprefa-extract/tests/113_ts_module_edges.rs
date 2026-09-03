//! The ts file-to-file module edge: one `resolved_import` row of
//! kind=module per specifier that names a corpus file, beside the binding rows.
//! A module edge is the file the specifier names; a binding that walked past
//! it through a re-export chain (`hops >= 2`) seats a name and is not one.
//!
//! FAIL-FIRST RECEIPT (1b2464c9bdf62a99073e93a66a935e8014e53f0d): no `module`
//! kind existed. `consumer.ts` emitted ONE `resolved_import` row, the `alpha`
//! binding at `alpha.ts` (kind=star, hops=2), so every file edge below read
//! `[]`: the barrel it imports from, the side-effect import, `import x =
//! require`, `export * from`, `export { x } from` and `import()` had no row at
//! all, and the one row it had named a file the source never imports. Over
//! TypeScript 5.9 `src/**` that shape scored 50.57 recall / 32.85 precision
//! against madge (RATCHET.tsv, ts5 module rows at that sha).
//!
//! Fixtures: `tests/fixtures/ts_module/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/ts_module";

const ALL: &[&str] = &[
    "consumer",
    "barrel",
    "alpha",
    "side_effect",
    "cjs_helper",
    "star_target",
    "beta",
    "lazy",
];

fn run(files: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    args.extend(files.iter().map(|name| format!("{DIR}/{name}.ts")));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

fn stem(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".ts")
        .to_string()
}

/// `(specifier, target stem)` per kind=module row written by `src`, sorted.
fn module_rows(rows: &[Value], src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = rows
        .iter()
        .filter(|row| row["record"] == "resolved_import" && row["kind"] == "module")
        .filter(|row| stem(&text(row, "src_path")) == src)
        .map(|row| (text(row, "name"), stem(&text(row, "target_path"))))
        .collect();
    out.sort();
    out
}

/// Every literal specifier form is a module row, and each names the file the
/// specifier resolves to (`.js` written for a `.ts`, extensionless `require`).
#[test]
fn every_literal_specifier_form_is_a_module_row() {
    let rows = run(ALL);
    assert_eq!(
        module_rows(&rows, "consumer"),
        [
            ("./barrel.js".to_string(), "barrel".to_string()),
            ("./beta.js".to_string(), "beta".to_string()),
            ("./cjs_helper".to_string(), "cjs_helper".to_string()),
            ("./lazy.js".to_string(), "lazy".to_string()),
            ("./side_effect.js".to_string(), "side_effect".to_string()),
            ("./star_target.js".to_string(), "star_target".to_string()),
        ]
    );
    assert_eq!(
        module_rows(&rows, "barrel"),
        [("./alpha.js".to_string(), "alpha".to_string())]
    );
}

/// The binding row through the barrel still seats `alpha` in `alpha.ts` at
/// two hops; the module rows never name that file from `consumer`, since the
/// source imports the barrel and nothing else.
#[test]
fn a_barrel_binding_keeps_its_seat_and_is_not_a_module_edge() {
    let rows = run(ALL);
    let alpha: Vec<(String, String, u64)> = rows
        .iter()
        .filter(|row| row["record"] == "resolved_import" && row["local"] == "alpha")
        .map(|row| {
            (
                stem(&text(row, "target_path")),
                text(row, "kind"),
                row["hops"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    assert_eq!(alpha, [("alpha".to_string(), "star".to_string(), 2)]);
    assert!(
        module_rows(&rows, "consumer")
            .iter()
            .all(|(_, target)| target != "alpha"),
        "a module row named the seat of a re-exported binding"
    );
}

/// A module row carries no binding: `hops` is 1, `local` is empty, and
/// `target_name` is null, so a consumer joining on names skips it.
#[test]
fn a_module_row_binds_no_name() {
    let rows = run(ALL);
    let shapes: Vec<(u64, String, bool)> = rows
        .iter()
        .filter(|row| row["record"] == "resolved_import" && row["kind"] == "module")
        .map(|row| {
            (
                row["hops"].as_u64().unwrap_or(0),
                text(row, "local"),
                row["target_name"].is_null(),
            )
        })
        .collect();
    assert_eq!(shapes.len(), 7);
    assert!(shapes.iter().all(|shape| *shape == (1, String::new(), true)));
}
