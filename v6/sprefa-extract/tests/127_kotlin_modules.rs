//! The kotlin module plane: `import` headers resolved through the supplied
//! files' own `package` headers (`src/lang/kotlin_modules.rs`) into
//! `resolved_import` rows, mirroring `62_go_module_plane.rs`.
//!
//! Fail-first receipt at e08866c82: `extract --resolve --family call` over
//! `tests/fixtures/kotlin_modules/**/*.kt` wrote 0 `resolved_import` rows
//! (`project.rs` `import_facts` chained ts, rust and go rows only).

use std::process::Command;

use serde_json::Value;

const FILES: &[&str] = &[
    "sample.kt",
    "project/model/Widget.kt",
    "project/model/Gadget.kt",
    "project/app/Main.kt",
];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    args.extend(
        FILES
            .iter()
            .map(|name| format!("tests/fixtures/kotlin_modules/{name}")),
    );
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
    path.rsplit('/').next().unwrap_or(path).to_string()
}

type Row = (String, String, String, String, String, String);

/// `(src stem, local, name, target stem, target_name, kind)` per `resolved_import`.
fn imports() -> Vec<Row> {
    let mut rows: Vec<Row> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_import")
        .map(|row| {
            (
                stem(&text(row, "src_path")),
                text(row, "local"),
                text(row, "name"),
                stem(&text(row, "target_path")),
                text(row, "target_name"),
                text(row, "kind"),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn row(src: &str, local: &str, name: &str, target: &str, target_name: &str, kind: &str) -> Row {
    (
        src.to_string(),
        local.to_string(),
        name.to_string(),
        target.to_string(),
        target_name.to_string(),
        kind.to_string(),
    )
}

/// `import com.acme.model.Widget`: a class, bound local to the file whose
/// `package` header declares `com.acme.model` and which declares `Widget`.
#[test]
fn a_class_import_binds_local_through_the_package_header() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "Main.kt",
            "",
            "com.acme.model.Widget",
            "Widget.kt",
            "",
            "module"
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "Main.kt",
            "Widget",
            "Widget",
            "Widget.kt",
            "Widget",
            "local"
        )),
        "{rows:?}"
    );
}

/// `import com.acme.model.makeWidget as build`: a top-level fun, the alias
/// is the local and the name stays the declared one.
#[test]
fn an_aliased_import_binds_by_the_alias() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "Main.kt",
            "build",
            "makeWidget",
            "Widget.kt",
            "makeWidget",
            "local"
        )),
        "{rows:?}"
    );
}

/// A typealias and a top-level val bind the same way a class does.
#[test]
fn a_typealias_and_a_top_level_val_bind_local() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "Main.kt",
            "WidgetId",
            "WidgetId",
            "Widget.kt",
            "WidgetId",
            "local"
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "Main.kt",
            "DEFAULT_WIDGET",
            "DEFAULT_WIDGET",
            "Widget.kt",
            "DEFAULT_WIDGET",
            "local"
        )),
        "{rows:?}"
    );
}

/// `import com.acme.model.*`: one module row and one star row per file
/// declaring the package.
#[test]
fn a_wildcard_import_mints_a_star_row_per_package_file() {
    let rows = imports();
    for target in ["Widget.kt", "Gadget.kt"] {
        assert!(
            rows.contains(&row(
                "Main.kt",
                "",
                "com.acme.model.*",
                target,
                "",
                "module"
            )),
            "{rows:?}"
        );
        assert!(
            rows.contains(&row("Main.kt", "*", "*", target, "", "star")),
            "{rows:?}"
        );
    }
}

/// `import com.acme.model.Missing` (no file declares it) and
/// `import java.util.List` (no supplied file declares the package) have no row;
/// `sample.kt`'s three imports are all outside the corpus.
#[test]
fn an_undeclared_name_and_an_external_package_bind_nothing() {
    let rows = imports();
    assert!(
        !rows.iter().any(|row| row.2.contains("Missing")),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.2.contains("java.util")),
        "{rows:?}"
    );
    assert!(!rows.iter().any(|row| row.0 == "sample.kt"), "{rows:?}");
}

/// COUNT: 4 named imports resolve (module + local each) and the wildcard
/// reaches 2 files (module + star each): 12 rows, all from `Main.kt`.
#[test]
fn row_count_matches_the_fixtures_written_headers() {
    let rows = imports();
    assert_eq!(rows.len(), 12, "{rows:?}");
    assert!(rows.iter().all(|row| row.0 == "Main.kt"), "{rows:?}");
}
