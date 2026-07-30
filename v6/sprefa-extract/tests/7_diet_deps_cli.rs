//! The `--deps` contract: diet module resolution, graded on shape here and on
//! numbers by `tools/1_madge_oracle.sh diet`.
//!
//! THE GOLDEN PINS JSONL FIELD NAMES. The v6 host decodes by top-level key, and
//! `--deps` reuses the `file_edge` record `--scip-deps` produces, so the module
//! graph is ONE relation regardless of which resolver filled it. A rename on
//! either side is a breaking change and has to show up as a diff.
//!
//! Every resolution policy in `src/deps.rs` has a line in `fixtures/deps/app.ts`
//! and an assertion below. The two policies that produce NO edge are asserted as
//! absences, because a resolver that silently invents an edge for a package
//! import or a broken path is worse than one that resolves nothing.

use std::collections::BTreeSet;
use std::process::Command;

use sprefa_extract::{resolve_specifier, Policy, TsconfigPaths};

const DEPS_ROOT: &str = "tests/fixtures/deps";

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

/// Every `.ts` under the fixture root: the diet resolver's universe IS its
/// argument list, so the corpus is named explicitly.
fn corpus() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DEPS_ROOT);
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

fn deps_edges() -> String {
    let corpus = corpus();
    let mut args: Vec<&str> = vec!["--deps", "--project-root", DEPS_ROOT];
    args.extend(corpus.iter().map(String::as_str));
    run(&args)
}

/// THE GOLDEN. Six edges, one per resolving policy plus the second name on the
/// util edge, and `symbols` counts distinct bound names exactly as the SCIP fold
/// counts distinct symbols.
#[test]
fn diet_deps_resolves_every_policy_to_file_edges() {
    assert_eq!(
        deps_edges(),
        concat!(
            r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","symbols":1}"#,
            "\n",
            r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/helper.ts","symbols":1}"#,
            "\n",
            r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/mapped.ts","symbols":1}"#,
            "\n",
            r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","symbols":2}"#,
            "\n",
            r#"{"record":"file_edge","src_path":"app.ts","dst_path":"side.ts","symbols":1}"#,
            "\n",
            r#"{"record":"file_edge","src_path":"app.ts","dst_path":"widget/index.ts","symbols":1}"#,
            "\n",
        )
    );
}

/// The two NON-edges. `rxjs` is a package and stops at the node_modules
/// boundary; `./gone.ts` names nothing in the universe. Neither may produce a
/// row, and neither may produce a row pointing somewhere plausible-looking.
#[test]
fn diet_deps_mints_no_edge_for_packages_or_broken_paths() {
    let edges = deps_edges();
    assert!(!edges.contains("rxjs"), "{edges}");
    assert!(!edges.contains("gone"), "{edges}");
}

/// The specifier row now carries its source module. THIS IS THE STEP THAT WAS
/// MISSING: oxc already captured import and export-from rows, but the row said
/// only that a name entered scope and refused to say from where, which made the
/// module graph inexpressible from phase-1 facts.
#[test]
fn specifier_rows_carry_the_source_module() {
    let facts = run(&["--family", "call", &format!("{DEPS_ROOT}/app.ts")]);
    for expected in [
        r#""name":"exact","kind":"named","module":"./lib/util.ts""#,
        r#""name":"boxed","kind":"named","module":"./widget""#,
        r#""name":"of","kind":"named","module":"rxjs""#,
        r#""name":"reexported","kind":"reexport","module":"./lib/util.ts""#,
        r#""name":"./side.ts","kind":"side_effect","module":"./side.ts""#,
    ] {
        assert!(facts.contains(expected), "missing {expected}");
    }
}

/// Each policy asserted directly, including the two stops and the unresolved
/// case. The CLI golden proves the edges; this proves the resolver says WHY,
/// which is what keeps every rule a stated policy instead of a heuristic.
#[test]
fn every_resolution_policy_is_named() {
    let universe: BTreeSet<String> = [
        "app.ts",
        "lib/util.ts",
        "lib/helper.ts",
        "lib/bare.ts",
        "lib/mapped.ts",
        "widget/index.ts",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let tsconfig = TsconfigPaths::read(std::path::Path::new(DEPS_ROOT));

    let cases: &[(&str, Option<&str>, Policy)] = &[
        ("./lib/util.ts", Some("lib/util.ts"), Policy::RelativeExact),
        (
            "./lib/helper.js",
            Some("lib/helper.ts"),
            Policy::RelativeEmittedRewrite,
        ),
        (
            "./lib/bare",
            Some("lib/bare.ts"),
            Policy::RelativeExtensionInferred,
        ),
        (
            "./widget",
            Some("widget/index.ts"),
            Policy::RelativeIndexFile,
        ),
        ("@app/mapped", Some("lib/mapped.ts"), Policy::TsconfigPaths),
        ("rxjs", None, Policy::NodeModulesBoundary),
        ("node:fs", None, Policy::NodeModulesBoundary),
        ("/etc/passwd", None, Policy::AbsolutePath),
        ("./gone.ts", None, Policy::RelativeUnresolved),
    ];
    for (specifier, target, policy) in cases {
        let (hit, applied) = resolve_specifier("app.ts", specifier, &universe, &tsconfig);
        assert_eq!(hit.as_deref(), *target, "target for {specifier}");
        assert_eq!(applied, *policy, "policy for {specifier}");
    }
}

/// `..` climbs, and the answer is computed lexically. A resolver that consulted
/// the filesystem here would follow symlinks out of the universe it was handed.
#[test]
fn parent_traversal_is_lexical() {
    let universe: BTreeSet<String> = ["a/b/c.ts", "a/shared.ts", "top.ts"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let tsconfig = TsconfigPaths::default();
    let cases: &[(&str, &str, Option<&str>)] = &[
        ("a/b/c.ts", "../shared.ts", Some("a/shared.ts")),
        ("a/b/c.ts", "../../top.ts", Some("top.ts")),
        // Climbing past the root clamps, which makes the specifier resolve to
        // nothing rather than escape the corpus.
        ("a/b/c.ts", "../../../../top.ts", Some("top.ts")),
        ("a/b/c.ts", "./nope.ts", None),
    ];
    for (from, specifier, target) in cases {
        let (hit, _) = resolve_specifier(from, specifier, &universe, &tsconfig);
        assert_eq!(hit.as_deref(), *target, "{from} + {specifier}");
    }
}

/// The tsconfig reader survives comments and trailing commas, and an
/// unparseable file degrades to an EMPTY config rather than an error. That
/// degradation under-resolves (every bare specifier stops at the node_modules
/// boundary) and never mis-resolves, which is the correct failure direction for
/// a best-effort resolver.
#[test]
fn the_tsconfig_reader_degrades_to_empty_never_to_wrong() {
    let real = TsconfigPaths::read(std::path::Path::new(DEPS_ROOT));
    assert_eq!(real.base_url.as_deref(), Some(""));
    assert_eq!(
        real.paths.get("@app/*").map(Vec::as_slice),
        Some(["lib/*".to_string()].as_slice())
    );

    let broken = TsconfigPaths::parse("{ this is not json ");
    assert_eq!(broken, TsconfigPaths::default());
    let universe: BTreeSet<String> = ["lib/mapped.ts"].into_iter().map(str::to_string).collect();
    let (hit, policy) = resolve_specifier("app.ts", "@app/mapped", &universe, &broken);
    assert_eq!(hit, None);
    assert_eq!(policy, Policy::NodeModulesBoundary);
}

/// `--deps` without `--project-root` is a named error. A module graph's node
/// names are project-relative paths, so there is no answer without a root and
/// guessing one would silently reshape every path in the output.
#[test]
fn diet_deps_without_a_project_root_is_a_named_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(["--deps", &format!("{DEPS_ROOT}/app.ts")])
        .output()
        .expect("extract binary runs");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        message.contains("project-root"),
        "the error must name what is missing, got: {message}"
    );
}
