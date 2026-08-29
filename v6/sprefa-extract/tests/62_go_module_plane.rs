//! The go module plane: import specs + exported identifiers resolved through
//! a directory index (`src/lang/go_modules.rs`), so `Resolve<CallF>` /
//! `Resolve<TypeF>` bind a `pkg.Name` selector through the target package's
//! REAL name instead of a corpus-wide name guess. Mirrors
//! `57_rust_module_plane.rs`.
//!
//! Fixtures: `tests/fixtures/go_modules/module_a`, `module_b` (a two-module
//! workspace: `go_module_of` must not confuse the two when both are fed to
//! one `--resolve` run).

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde_json::Value;

const FILES: &[&str] = &[
    "module_a/pkgutil2/widget.go",
    "module_a/pkgutil3/widget2.go",
    "module_a/blankpkg/blank.go",
    "module_a/vendorlike/yaml.v3/yaml.go",
    "module_a/main.go",
    "module_a/shadow.go",
    "module_b/pkgutil/util.go",
    "module_b/main.go",
];

fn run(families: &str) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        families.to_string(),
    ];
    args.extend(
        FILES
            .iter()
            .map(|name| format!("tests/fixtures/go_modules/{name}")),
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

/// `(caller, callee file stem, callee, kind)` per `resolved_edge`, sorted.
fn call_edges(families: &str) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = run(families)
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                stem(&text(row, "callee_path")),
                text(row, "callee_name"),
                text(row, "kind"),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `(owner name, target file stem, target name, kind)` per `resolved_type_edge`.
fn type_edges(families: &str) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = run(families)
        .iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            (
                text(row, "owner_name"),
                stem(&text(row, "target_path")),
                text(row, "target_name"),
                text(row, "kind"),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `(src stem, local, target stem, target_name, kind)` per `resolved_import`.
fn imports(families: &str) -> Vec<(String, String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String, String)> = run(families)
        .iter()
        .filter(|row| row["record"] == "resolved_import")
        .map(|row| {
            (
                stem(&text(row, "src_path")),
                text(row, "local"),
                stem(&text(row, "target_path")),
                text(row, "target_name"),
                text(row, "kind"),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `(path stem, reason, detail)` per `unresolved`.
fn unresolved(families: &str) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run(families)
        .iter()
        .filter(|row| row["record"] == "unresolved")
        .map(|row| {
            (
                stem(&text(row, "path")),
                text(row, "reason"),
                text(row, "detail"),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `import alias "example.com/b/pkgutil"` binds the call THROUGH the alias,
/// and the `resolved_import` row's `local` is the alias itself.
#[test]
fn an_aliased_import_binds_by_the_alias() {
    assert!(call_edges("call").contains(&(
        "UseAlias".to_string(),
        "util.go".to_string(),
        "Helper".to_string(),
        "import_resolve".to_string(),
    )));
    assert!(imports("call").contains(&(
        "main.go".to_string(),
        "alias".to_string(),
        "pkgutil".to_string(),
        "util".to_string(),
        "local".to_string(),
    )));
}

/// `alias.helper()`: `helper` is unexported (lower-case), invisible outside
/// its own package, so no `resolved_edge` binds it at all.
#[test]
fn an_unexported_name_binds_no_edge() {
    assert!(!call_edges("call")
        .iter()
        .any(|(caller, ..)| caller == "UseUnexported"));
}

/// `import "example.com/a/vendorlike/yaml.v3"`: the directory's OWN package
/// clause says `package yaml`, so the `resolved_import` row's `local` is
/// `yaml`, never `yaml.v3` (the import path's last segment).
#[test]
fn an_unaliased_import_binds_by_the_real_package_name_not_the_last_segment() {
    assert!(imports("call").contains(&(
        "main.go".to_string(),
        "yaml".to_string(),
        "yaml.v3".to_string(),
        "yaml".to_string(),
        "local".to_string(),
    )));
}

/// `type Wrapper struct { N yaml.Node }`: the SAME path-vs-package-name gap,
/// proven on the type plane where the qualifier text survives phase 1 intact
/// (`yaml.Node` is written into the candidate row verbatim).
#[test]
fn a_qualified_type_ref_binds_through_the_real_package_name() {
    assert!(type_edges("type").contains(&(
        "Wrapper".to_string(),
        "yaml.go".to_string(),
        "Node".to_string(),
        "field".to_string(),
    )));
}

/// `import . "example.com/a/pkgutil2"` in `main.go`: the row is `kind=namespace`
/// with no member name, one row per file that writes the dot import.
#[test]
fn a_dot_import_mints_a_namespace_row_per_file() {
    let rows = imports("call");
    assert!(rows.contains(&(
        "main.go".to_string(),
        ".".to_string(),
        "pkgutil2".to_string(),
        "putil2".to_string(),
        "namespace".to_string(),
    )));
    assert!(rows.contains(&(
        "shadow.go".to_string(),
        ".".to_string(),
        "pkgutil2".to_string(),
        "putil2".to_string(),
        "namespace".to_string(),
    )));
}

/// `Widget()` bare in `main.go`: BOTH pkgutil2 and pkgutil3 export `Widget`
/// (corpus-wide ambiguous), so only `main.go`'s OWN dot import of pkgutil2
/// disambiguates the call.
#[test]
fn a_bare_name_binds_through_its_files_own_dot_import() {
    assert!(call_edges("call").contains(&(
        "UseDot".to_string(),
        "widget.go".to_string(),
        "Widget".to_string(),
        "import_resolve".to_string(),
    )));
}

/// `shadow.go` declares its OWN `Widget`, dot-imports pkgutil2 too: the
/// same-file declaration wins, never the dot import (`name_resolve`, not
/// `import_resolve`), and the edge lands in `shadow.go` itself.
#[test]
fn a_local_declaration_shadows_a_dot_imported_name() {
    assert!(call_edges("call").contains(&(
        "UseShadowed".to_string(),
        "shadow.go".to_string(),
        "Widget".to_string(),
        "name_resolve".to_string(),
    )));
}

/// `_ "example.com/a/blankpkg"`: a blank import binds nothing, so it mints no
/// `resolved_import` row and no `unresolved` row.
#[test]
fn a_blank_import_mints_no_row_at_all() {
    assert!(!imports("call")
        .iter()
        .any(|(_, _, target, ..)| target == "blankpkg"));
    assert!(!unresolved("call")
        .iter()
        .any(|(_, _, detail)| detail.contains("blankpkg")));
}

/// `import "github.com/pkg/errors"`: outside `example.com/a` entirely, so no
/// `resolved_import` row exists, and the import SPEC itself drops with
/// reason `external` (independent of whether any call site references it).
#[test]
fn an_external_import_drops_with_reason_external() {
    assert!(!imports("call")
        .iter()
        .any(|(_, _, _, target_name, _)| target_name == "errors"));
    assert!(unresolved("call").contains(&(
        "main.go".to_string(),
        "external".to_string(),
        "github.com/pkg/errors".to_string(),
    )));
}

/// One `resolved_import` row per resolvable, non-blank import spec written:
/// `main.go` (dot pkgutil2, yaml.v3), `shadow.go` (dot pkgutil2), `main.go`
/// (module_b, alias). The blank and external specs mint none.
#[test]
fn import_count_matches_the_fixtures_written_bindings() {
    let count = imports("call").len();
    assert_eq!(count, 4, "imports: {:?}", imports("call"));
}

// ── the plane's cost ────────────────────────────────────────────────────────

/// `(record-filtered rows)` of one `--resolve` run over EXPLICIT paths (the
/// twin-package fixtures get their own file list, so `FILES` stays the
/// two-module workspace set the older tests pin).
fn run_paths(paths: &[String]) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

/// module_c: two directories BOTH declaring `package debug` (the name-collision
/// shape), one of them with a `Helper` the other lacks, plus a `_test.go`
/// sibling package beside a primary package, plus two caller files.
fn twin_module(dir: &Path) -> Vec<String> {
    let dir = dir.join("twin");
    for rel in ["debug", "other", "poison", "caller"] {
        std::fs::create_dir_all(dir.join(rel)).expect("package dir");
    }
    std::fs::write(dir.join("go.mod"), "module example.com/c\n\ngo 1.22\n").expect("go.mod");
    std::fs::write(
        dir.join("debug/debug.go"),
        "package debug\n\n// Helper lives in the OTHER package named debug.\n",
    )
    .expect("debug file");
    std::fs::write(
        dir.join("other/debug.go"),
        "package debug\n\nfunc Helper() int { return 1 }\n",
    )
    .expect("other debug file");
    // The _test sibling is written FIRST: the directory's primary package name
    // must win the dir's binding whatever order the files arrive in.
    std::fs::write(
        dir.join("poison/alpha_test.go"),
        "package alpha_test\n\nimport \"testing\"\n\nfunc TestA(t *testing.T) { _ = t }\n",
    )
    .expect("test file");
    std::fs::write(
        dir.join("poison/alpha.go"),
        "package alpha\n\nfunc Pick() int { return 1 }\n",
    )
    .expect("poison file");
    std::fs::write(
        dir.join("caller/caller.go"),
        "package caller\n\nimport (\n\t\"example.com/c/debug\"\n\tp \"example.com/c/poison\"\n\tq \"example.com/c/other\"\n)\n\nfunc Use() int {\n\treturn debug.Helper() + q.Helper() + p.Pick()\n}\n",
    )
    .expect("caller file");
    [
        dir.join("poison/alpha_test.go"),
        dir.join("poison/alpha.go"),
        dir.join("debug/debug.go"),
        dir.join("other/debug.go"),
        dir.join("caller/caller.go"),
    ]
    .iter()
    .map(|p| p.to_string_lossy().into_owned())
    .collect()
}

fn absolute(paths: &[String]) -> Vec<String> {
    let root = env!("CARGO_MANIFEST_DIR");
    paths
        .iter()
        .map(|p| {
            if Path::new(p).is_absolute() {
                p.clone()
            } else {
                format!("{root}/{p}")
            }
        })
        .collect()
}

/// `(local, target stem, target_name)` per `resolved_import`, raw order lost.
fn import_rows(rows: &[Value]) -> Vec<(String, String, String, String)> {
    let mut out: Vec<(String, String, String, String)> = rows
        .iter()
        .filter(|row| row["record"] == "resolved_import")
        .map(|row| {
            (
                stem(&text(row, "src_path")),
                text(row, "local"),
                stem(&text(row, "target_path")),
                text(row, "target_name"),
            )
        })
        .collect();
    out.sort();
    out
}

/// A caller run ALONE (its imported package's files absent from this
/// invocation) still writes the `resolved_import` row: the directory an
/// in-module import names is computable from the module alone, and the row
/// must not depend on which files share the process.
#[test]
fn an_import_emits_its_row_without_the_target_package_in_the_run() {
    let dir = std::env::temp_dir().join("sprefa-extract-62-twin");
    let _ = std::fs::remove_dir_all(&dir);
    let all = twin_module(&dir);
    let caller = absolute(&all)
        .into_iter()
        .find(|p| p.ends_with("caller/caller.go"))
        .expect("caller path");
    let rows = import_rows(&run_paths(&[caller]));
    let poison = rows
        .iter()
        .find(|(.., target_stem, _)| target_stem == "poison");
    assert!(
        poison.is_some(),
        "the poison import lost its row when its package was outside the invocation: {rows:?}"
    );
}

/// Two directories both declaring `package debug` must bind independently: the
/// plain import binds `debug`, the OTHER package named debug binds through its
/// own directory, and neither row takes the other's name.
#[test]
fn two_packages_sharing_a_name_bind_by_directory_not_by_name() {
    let dir = std::env::temp_dir().join("sprefa-extract-62-twin-name");
    let _ = std::fs::remove_dir_all(&dir);
    let all = twin_module(&dir);
    let rows = import_rows(&run_paths(&all));
    // Both imports resolve to a package whose REAL name is debug: the plain
    // one into debug/, the aliased one into other/. Each keeps its own local.
    assert!(
        rows.contains(&(
            "caller.go".to_string(),
            "debug".to_string(),
            "debug".to_string(),
            "debug".to_string()
        )),
        "the plain debug import binds: {rows:?}"
    );
    assert!(
        rows.contains(&(
            "caller.go".to_string(),
            "q".to_string(),
            "other".to_string(),
            "debug".to_string()
        )),
        "the second package named debug binds through its own directory: {rows:?}"
    );
}

/// `poison/alpha_test.go` declares `package alpha_test` in the same directory
/// as `alpha.go`'s `package alpha`: the dir's PRIMARY package name (no
/// `_test` suffix) wins, whatever order the files arrive in.
#[test]
fn a_test_sibling_never_poisons_the_dir_package_name() {
    let dir = std::env::temp_dir().join("sprefa-extract-62-twin-poison");
    let _ = std::fs::remove_dir_all(&dir);
    let all = twin_module(&dir);
    let rows = import_rows(&run_paths(&all));
    let row = rows
        .iter()
        .find(|(.., target_stem, _)| target_stem == "poison")
        .unwrap_or_else(|| panic!("poison import row: {rows:?}"));
    assert_eq!(
        row.3, "alpha",
        "the dir's package name is poisoned: {rows:?}"
    );
}

/// `debug.Helper` names a package whose files sit in the invocation but which
/// declares no `Helper`: the import leg declines, and the v5-shaped corpus
/// name-match is the LAST leg (unique corpus `Helper` wins, kind
/// `name_resolve`), never a silent nothing.
#[test]
fn an_import_qualified_site_falls_back_to_the_name_match() {
    let all = twin_module(&std::env::temp_dir().join("sprefa-extract-62-twin-fallback"));
    let rows = run_paths(&all);
    let edges: Vec<(String, String, String, String)> = rows
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                stem(&text(row, "callee_path")),
                text(row, "callee_name"),
                text(row, "kind"),
            )
        })
        .collect();
    let hit = edges
        .iter()
        .find(|(caller, callee_file, callee, _)| {
            caller == "Use" && callee_file == "debug.go" && callee == "Helper"
        })
        .unwrap_or_else(|| panic!("no edge to the unique corpus Helper: {edges:?}"));
    assert_eq!(
        hit.3, "name_resolve",
        "the fallback leg is the name match: {edges:?}"
    );
}

const RATIO_BUDGET: f64 = 2.5;

/// `n` leaf files in one package, `n` callers in a second package each
/// reaching one leaf through a plain (unaliased) import: the shape that
/// makes the plane work hardest, one directory-scoped lookup per call.
fn qualified_corpus(dir: &Path, n: usize) -> Vec<String> {
    let dir = dir.join(format!("n{n}"));
    std::fs::create_dir_all(dir.join("leaf")).expect("leaf dir");
    std::fs::create_dir_all(dir.join("caller")).expect("caller dir");
    std::fs::write(dir.join("go.mod"), "module example.com/gen\n\ngo 1.22\n").expect("go.mod");
    let mut leaf = String::from("package leaf\n\n");
    for index in 0..n {
        leaf.push_str(&format!("func Pick{index}() int {{ return {index} }}\n"));
    }
    let leaf_path = dir.join("leaf/leaf.go");
    std::fs::write(&leaf_path, leaf).expect("leaf file");
    let mut paths = vec![leaf_path.to_string_lossy().into_owned()];
    for index in 0..n {
        let path = dir.join(format!("caller/f{index}.go"));
        std::fs::write(
            &path,
            format!(
                "package caller\n\nimport \"example.com/gen/leaf\"\n\nfunc call{index}() int {{ return leaf.Pick{index}() }}\n"
            ),
        )
        .expect("caller file");
        paths.push(path.to_string_lossy().into_owned());
    }
    paths
}

fn resolve_wall(paths: &[String]) -> f64 {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg("--family")
        .arg("call")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    start.elapsed().as_secs_f64()
}

/// COUNT test on cost: doubling the corpus must not more than 2.5x the wall.
/// A per-call-site directory rescan (rather than the per-file `own_path`/
/// `module` join) would show up here as a quadratic, not a wrong answer.
#[test]
fn qualified_import_resolve_wall_grows_linearly_with_file_count() {
    let dir = std::env::temp_dir().join("sprefa-extract-62-go-module-plane");
    std::fs::create_dir_all(&dir).expect("scratch root");
    let small = qualified_corpus(&dir, 200);
    let large = qualified_corpus(&dir, 400);
    let wall200 = resolve_wall(&small);
    let wall400 = resolve_wall(&large);
    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}
