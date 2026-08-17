//! The `--package-deps` contract: workspace-internal manifest-to-manifest edges,
//! one arm per manifest kind, keyed on project-relative manifest PATHS.
//!
//! It is v5's `crate_edge` (`src/graph/modgraph/rust.rs:468`) generalized: that
//! relation was Cargo-only, and it keyed on crate names, which needs a second
//! dictionary to reach a file. `file_edge` already keys on paths, so this grain
//! keys the same way and the two join directly.
//!
//! Expected rows are hand-derived from `fixtures/packages`, never copied from the
//! binary, and written in the sorted order `project::sorted_lines` prints under.
//!
//! SABOTAGE RECEIPTS (each run red, then reverted, `left` naming the loss):
//!  - reading the code name instead of `package = "gamma"` in `cargo_edges`
//!    loses the alpha -> gamma `dev` row: the golden fails at 9 rows to 10.
//!  - folding `peerDependencies` into `normal` in `npm_edges` reprints the
//!    js/app -> js/peer row under the wrong kind: the golden fails on it.
//!  - deleting the `Replacement::FilePath` arm from `gomod_edges` loses
//!    go/svc -> go/lib `replace`, the row that proves one pair carries two
//!    kinds: the golden fails at 9 rows to 10.
//!  - dropping the `destination != manifest.path` guard makes
//!    `a_self_dependency_is_no_edge` report one row.

use std::process::Command;

use sprefa_extract::{fold_package_edges, Manifest, ManifestKind};

const PACKAGES_ROOT: &str = "tests/fixtures/packages";

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// Every manifest under the fixture root: the fold's universe IS its argument
/// list, so the corpus is named explicitly.
fn manifest_paths() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PACKAGES_ROOT);
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if matches!(name, "Cargo.toml" | "package.json" | "go.mod") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

fn package_edges() -> Vec<String> {
    let corpus = manifest_paths();
    let mut args: Vec<&str> = vec!["--package-deps", "--project-root", PACKAGES_ROOT];
    args.extend(corpus.iter().map(String::as_str));
    run(&args).lines().map(str::to_string).collect()
}

fn sorted(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    out.sort();
    out
}

/// THE GOLDEN. Ten edges over three ecosystems, including the two pairs that
/// carry two kinds each (alpha -> gamma dev + build, go/svc -> go/lib require +
/// replace), which is what proves `kind` is part of the edge key.
#[test]
fn package_deps_folds_every_manifest_arm() {
    let expected = [
        r#"{"record":"package_edge","src_manifest":"crates/alpha/Cargo.toml","dst_manifest":"crates/beta/Cargo.toml","kind":"normal"}"#,
        r#"{"record":"package_edge","src_manifest":"crates/alpha/Cargo.toml","dst_manifest":"crates/gamma/Cargo.toml","kind":"build"}"#,
        r#"{"record":"package_edge","src_manifest":"crates/alpha/Cargo.toml","dst_manifest":"crates/gamma/Cargo.toml","kind":"dev"}"#,
        r#"{"record":"package_edge","src_manifest":"go/svc/go.mod","dst_manifest":"go/lib/go.mod","kind":"replace"}"#,
        r#"{"record":"package_edge","src_manifest":"go/svc/go.mod","dst_manifest":"go/lib/go.mod","kind":"require"}"#,
        r#"{"record":"package_edge","src_manifest":"go/tool/go.mod","dst_manifest":"go/lib/go.mod","kind":"replace"}"#,
        r#"{"record":"package_edge","src_manifest":"go/tool/go.mod","dst_manifest":"go/svc/go.mod","kind":"require"}"#,
        r#"{"record":"package_edge","src_manifest":"js/app/package.json","dst_manifest":"js/lib/package.json","kind":"normal"}"#,
        r#"{"record":"package_edge","src_manifest":"js/app/package.json","dst_manifest":"js/peer/package.json","kind":"peer"}"#,
        r#"{"record":"package_edge","src_manifest":"js/app/package.json","dst_manifest":"js/tools/package.json","kind":"dev"}"#,
    ];
    assert_eq!(package_edges(), sorted(&expected));
}

/// The NON-edges, asserted as absences. A registry package has no manifest in
/// the corpus to point at, a virtual workspace root declares no name, and an
/// unparseable manifest contributes neither a name nor an edge.
#[test]
fn package_deps_mints_no_edge_outside_the_workspace() {
    let edges = package_edges().join("\n");
    for outside in ["serde", "rxjs", "golang.org", "broken", "\"Cargo.toml\""] {
        assert!(
            !edges.contains(outside),
            "{outside} minted an edge: {edges}"
        );
    }
}

/// A manifest depending on itself is no edge. A workspace member that lists its
/// own name (a `path` dependency pointing at its own directory) would otherwise
/// mint a self-loop that every reachability query then has to filter.
#[test]
fn a_self_dependency_is_no_edge() {
    let manifests = [Manifest {
        path: "solo/Cargo.toml".to_string(),
        kind: ManifestKind::Cargo,
        text: "[package]\nname = \"solo\"\n\n[dependencies]\nsolo = { path = \".\" }\n".to_string(),
    }];
    assert!(fold_package_edges(&manifests).is_empty());
}

/// `--package-deps` without `--project-root` is a named error: a package graph's
/// node names are project-relative manifest paths, so guessing a root would
/// silently reshape every node name in the output.
#[test]
fn package_deps_without_a_project_root_is_a_named_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(["--package-deps", &format!("{PACKAGES_ROOT}/Cargo.toml")])
        .output()
        .expect("extract binary runs");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("project-root"),
        "the error must name what is missing, got: {message}"
    );
}
