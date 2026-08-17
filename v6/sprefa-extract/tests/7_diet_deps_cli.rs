//! The `--deps` contract: diet module resolution, graded on shape here and on
//! numbers by `tools/1_madge_oracle.sh diet`.
//!
//! THE GOLDEN PINS JSONL FIELD NAMES. The v6 host decodes by top-level key, and
//! `--deps` reuses the `file_edge` record `--scip-deps` produces, so the module
//! graph is ONE relation regardless of which resolver filled it. A rename on
//! either side is a breaking change and has to show up as a diff.
//!
//! Every resolution policy in `src/deps.rs` has a line in `fixtures/deps/app.ts`
//! and an assertion below. The three policies that produce NO edge are asserted
//! as absences AND as `file_unresolved` rows: a resolver that silently invents
//! an edge for a package import is worse than one that resolves nothing, and one
//! that drops the stop entirely cannot be asked which imports left the corpus.
//!
//! SABOTAGE RECEIPTS (each run red, then reverted):
//!  - freezing `fold_edges`'s key kind to `named` merges the two `lib/bare.ts`
//!    rows into one at symbols=2 and the three `lib/util.ts` rows into one:
//!    `diet_deps_resolves_every_policy_to_file_edges` fails at 6 rows to 9.
//!  - narrowing `fold_unresolved` to `Policy::RelativeUnresolved` loses the
//!    rxjs and `/lib/util.ts` rows: `diet_deps_records_every_stop` fails at 1
//!    row to 3.
//!  - making `renamed` return `Some(imported)` unconditionally puts
//!    `"imported":"exact"` on a plain named import:
//!    `specifier_rows_carry_the_source_module` fails on the `null` line.

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

/// The `--deps` lines carrying one record tag, in the order the binary printed
/// them (`sorted_lines`, so lexicographic on the serialized row).
fn deps_records(tag: &str) -> Vec<String> {
    let needle = format!(r#""record":"{tag}""#);
    deps_edges()
        .lines()
        .filter(|line| line.contains(&needle))
        .map(str::to_string)
        .collect()
}

/// Hand-written expectations, sorted by the same rule the binary prints under.
fn sorted(lines: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    out.sort();
    out
}

/// THE GOLDEN. Nine edges: the edge key is (src, dst, kind), so `./lib/bare`
/// carries a named row and a namespace row and `./lib/util.ts` carries three,
/// and `symbols` counts the distinct bound names OF THAT KIND exactly as the
/// SCIP fold counts distinct symbols.
///
/// Hand-derived from the fixture, never copied from the binary: the rows are
/// written in the sorted order `project::sorted_lines` produces.
#[test]
fn diet_deps_resolves_every_policy_to_file_edges() {
    let expected = [
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","kind":"named","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/bare.ts","kind":"namespace","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/helper.ts","kind":"named","symbols":2}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/mapped.ts","kind":"named","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"default","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"named","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"lib/util.ts","kind":"reexport","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"side.ts","kind":"side_effect","symbols":1}"#,
        r#"{"record":"file_edge","src_path":"app.ts","dst_path":"widget/index.ts","kind":"named","symbols":1}"#,
    ];
    assert_eq!(deps_records("file_edge"), sorted(&expected));
}

/// The three NON-edges. `rxjs` is a package and stops at the node_modules
/// boundary, `/lib/util.ts` is filesystem-absolute, and `./gone.ts` names
/// nothing in the universe. None may produce an edge, and none may produce one
/// pointing somewhere plausible-looking.
#[test]
fn diet_deps_mints_no_edge_for_packages_or_broken_paths() {
    for line in deps_edges().lines() {
        if !line.contains(r#""record":"file_edge""#) {
            continue;
        }
        for stopped in ["rxjs", "gone", "/lib/util.ts"] {
            assert!(!line.contains(stopped), "{stopped} minted an edge: {line}");
        }
    }
}

/// EVERY STOP IS A ROW. A dropped stop is indistinguishable from an import that
/// was never written, which is the difference between "this file imports nothing
/// outside the corpus" and "this resolver could not tell you".
#[test]
fn diet_deps_records_every_stop() {
    let expected = [
        r#"{"record":"file_unresolved","src_path":"app.ts","module":"./gone.ts","reason":"relative_unresolved"}"#,
        r#"{"record":"file_unresolved","src_path":"app.ts","module":"/lib/util.ts","reason":"absolute_path"}"#,
        r#"{"record":"file_unresolved","src_path":"app.ts","module":"rxjs","reason":"node_modules_boundary"}"#,
    ];
    assert_eq!(deps_records("file_unresolved"), sorted(&expected));
}

/// The specifier row now carries its source module. THIS IS THE STEP THAT WAS
/// MISSING: oxc already captured import and export-from rows, but the row said
/// only that a name entered scope and refused to say from where, which made the
/// module graph inexpressible from phase-1 facts.
#[test]
fn specifier_rows_carry_the_source_module() {
    let facts = run(&["--family", "call", &format!("{DEPS_ROOT}/app.ts")]);
    for expected in [
        r#""name":"exact","kind":"named","module":"./lib/util.ts","imported":null"#,
        r#""name":"boxed","kind":"named","module":"./widget","imported":null"#,
        r#""name":"of","kind":"named","module":"rxjs","imported":null"#,
        r#""name":"reexported","kind":"reexport","module":"./lib/util.ts","imported":null"#,
        r#""name":"./side.ts","kind":"side_effect","module":"./side.ts","imported":null"#,
    ] {
        assert!(facts.contains(expected), "missing {expected}");
    }
}

/// THE IMPORTED NAME, which is the one column v5's `module_binding` carried and
/// this row did not. `import {inner as outer}` binds `outer` locally and asks
/// the source module for `inner`; without the second name the row cannot say
/// which export it reached, and a default import cannot say `default` at all.
#[test]
fn renamed_specifiers_carry_the_source_name() {
    let facts = run(&["--family", "call", &format!("{DEPS_ROOT}/app.ts")]);
    for expected in [
        r#""name":"outer","kind":"named","module":"./lib/helper.js","imported":"inner""#,
        r#""name":"defaults","kind":"default","module":"./lib/util.ts","imported":"default""#,
        r#""name":"everything","kind":"namespace","module":"./lib/bare","imported":null"#,
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
