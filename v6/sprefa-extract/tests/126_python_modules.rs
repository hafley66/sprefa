//! The python module plane: import statements resolved against the supplied
//! file set (`src/lang/python/_2_modules.rs`) into `resolved_import` rows,
//! mirroring `62_go_module_plane.rs`.
//!
//! Fail-first receipt at e08866c82: `extract --resolve --family call` over
//! `tests/fixtures/python_modules/**/*.py` wrote 0 `resolved_import` rows
//! (`project.rs` `import_facts` chained ts, rust and go rows only).

use std::process::Command;

use serde_json::Value;

const FILES: &[&str] = &[
    "main.py",
    "app/__init__.py",
    "app/core.py",
    "app/helpers.py",
    "app/sub/__init__.py",
    "app/sub/leaf.py",
    "app/sub/sibling.py",
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
            .map(|name| format!("tests/fixtures/python_modules/{name}")),
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

/// `src_path` and `target_path` relative to the fixture directory.
fn rel(path: &str) -> String {
    path.split("python_modules/")
        .nth(1)
        .unwrap_or(path)
        .to_string()
}

type Row = (String, String, String, String, String, String, u64);

/// `(src, local, name, target, target_name, kind, hops)` per `resolved_import`.
fn imports() -> Vec<Row> {
    let mut rows: Vec<Row> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_import")
        .map(|row| {
            (
                rel(&text(row, "src_path")),
                text(row, "local"),
                text(row, "name"),
                rel(&text(row, "target_path")),
                text(row, "target_name"),
                text(row, "kind"),
                row["hops"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn row(
    src: &str,
    local: &str,
    name: &str,
    target: &str,
    target_name: &str,
    kind: &str,
    hops: u64,
) -> Row {
    (
        src.to_string(),
        local.to_string(),
        name.to_string(),
        target.to_string(),
        target_name.to_string(),
        kind.to_string(),
        hops,
    )
}

/// `import app` / `import app.core`: the package's `__init__.py` and the
/// module file, one `module` file edge and one `namespace` binding each.
#[test]
fn a_plain_import_binds_the_dotted_name_as_a_namespace() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "main.py",
            "",
            "app",
            "app/__init__.py",
            "",
            "module",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "main.py",
            "app",
            "*",
            "app/__init__.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "main.py",
            "",
            "app.core",
            "app/core.py",
            "",
            "module",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "main.py",
            "app.core",
            "*",
            "app/core.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
}

/// `import app.helpers as helpers_alias`: the alias is the local.
#[test]
fn an_aliased_module_import_binds_by_the_alias() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "main.py",
            "helpers_alias",
            "*",
            "app/helpers.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
}

/// `from app import run`: `app/__init__.py` re-exports `run` through
/// `from .core import run`, so the row reaches `core.py` at kind=indirect.
#[test]
fn a_package_re_export_binds_indirect_to_the_declaring_module() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "main.py",
            "run",
            "run",
            "app/core.py",
            "run",
            "indirect",
            2
        )),
        "{rows:?}"
    );
}

/// `from app import helper`: reached through `from .helpers import *` in the
/// package, kind=star.
#[test]
fn a_star_re_export_binds_star_to_the_declaring_module() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "main.py",
            "helper",
            "helper",
            "app/helpers.py",
            "helper",
            "star",
            2
        )),
        "{rows:?}"
    );
}

/// `from app.sub import leaf`: `sub/__init__.py` declares nothing, so the
/// name is the package's submodule, a namespace row with no target name.
#[test]
fn a_submodule_named_in_a_from_import_binds_as_a_namespace() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "main.py",
            "leaf",
            "leaf",
            "app/sub/leaf.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
}

/// `from app.core import missing`: the module row stands, the binding has
/// no row; `import os` has neither.
#[test]
fn an_undeclared_name_and_an_external_module_bind_nothing() {
    let rows = imports();
    assert!(!rows.iter().any(|row| row.1 == "missing"), "{rows:?}");
    assert!(!rows.iter().any(|row| row.2 == "os"), "{rows:?}");
    assert_eq!(
        rows.iter()
            .filter(|row| row.0 == "main.py" && row.2 == "app.core" && row.5 == "module")
            .count(),
        2,
        "{rows:?}"
    );
}

/// `from app.helpers import *` in `main.py`: one star row, local `*`.
#[test]
fn a_star_import_mints_one_star_row() {
    let rows = imports();
    assert!(
        rows.contains(&row("main.py", "*", "*", "app/helpers.py", "", "star", 1)),
        "{rows:?}"
    );
}

/// Every relative form in `app/sub/leaf.py`: `from .. import core` (the
/// parent package's submodule), `from ..core import Engine as Eng` (a local
/// declaration, aliased), `from . import sibling`, `from .sibling import sib`.
#[test]
fn relative_imports_walk_directories_from_the_importing_file() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "",
            "..",
            "app/__init__.py",
            "",
            "module",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "core",
            "core",
            "app/core.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "",
            "..core",
            "app/core.py",
            "",
            "module",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "Eng",
            "Engine",
            "app/core.py",
            "Engine",
            "local",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "",
            ".",
            "app/sub/__init__.py",
            "",
            "module",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "sibling",
            "sibling",
            "app/sub/sibling.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "sib",
            "sib",
            "app/sub/sibling.py",
            "sib",
            "local",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/sub/leaf.py",
            "h",
            "*",
            "app/helpers.py",
            "",
            "namespace",
            1
        )),
        "{rows:?}"
    );
}

/// The package's own re-exports are rows too: `from .core import run` binds
/// local, `from .helpers import *` is a star row.
#[test]
fn the_package_init_writes_its_own_rows() {
    let rows = imports();
    assert!(
        rows.contains(&row(
            "app/__init__.py",
            "run",
            "run",
            "app/core.py",
            "run",
            "local",
            1
        )),
        "{rows:?}"
    );
    assert!(
        rows.contains(&row(
            "app/__init__.py",
            "*",
            "*",
            "app/helpers.py",
            "",
            "star",
            1
        )),
        "{rows:?}"
    );
}

/// COUNT: `main.py` writes 9 clauses, 8 module rows (`os` is external) and
/// 7 bindings (`missing` declines); `leaf.py` 5 and 5; `__init__.py` 2 and 2.
#[test]
fn row_count_matches_the_fixtures_written_clauses() {
    let rows = imports();
    let count = |src: &str, kind: &str| {
        rows.iter()
            .filter(|row| row.0 == src && (row.5 == "module") == (kind == "module"))
            .count()
    };
    assert_eq!(count("main.py", "module"), 8, "{rows:?}");
    assert_eq!(count("main.py", "binding"), 7, "{rows:?}");
    assert_eq!(count("app/sub/leaf.py", "module"), 5, "{rows:?}");
    assert_eq!(count("app/sub/leaf.py", "binding"), 5, "{rows:?}");
    assert_eq!(count("app/__init__.py", "module"), 2, "{rows:?}");
    assert_eq!(count("app/__init__.py", "binding"), 2, "{rows:?}");
    assert_eq!(rows.len(), 29, "{rows:?}");
}
