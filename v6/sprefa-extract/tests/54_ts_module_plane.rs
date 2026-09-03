//! The ts module plane: ECMAScript ResolveExport (ECMA-262 16.2.1.6.3) run
//! once per file set, so an imported name binds the way the module system
//! binds it and name-matching across files is only what a FREE name falls to.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): before the plane, `--resolve`
//! never read a `specifier` row, so `TsSource::call_name_match` was the only
//! tier. Every `import_resolve` assertion below read `[]` (the callee had two
//! corpus defs and the `[blob] = blobs.as_slice() else` bail dropped the site),
//! the `resolved_import` record did not exist, and the ambiguous barrel emitted
//! nothing at all instead of an `unresolved` row. Over TypeScript 5.9 `src/**`
//! that bail left 11,768 call sites ambiguous by name, 1,241 of which name
//! exactly one file once the barrel closure is applied.
//!
//! Fixtures: `tests/fixtures/ts5_findings/module_plane/`.

use std::process::Command;
use std::time::Instant;

use serde_json::Value;

const DIR: &str = "tests/fixtures/ts5_findings/module_plane";

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

/// `(caller, callee file stem, callee, kind)` per `resolved_edge`, sorted. The
/// file stem rides the tuple because the whole point is WHICH file was picked.
fn edges(files: &[&str]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = run(files)
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

/// `(local, name, target stem, target_name, kind, hops)` per `resolved_import`
/// BINDING row; the kind=module file edges (`113_ts_module_edges.rs`) bind no
/// name and stay out.
fn imports(files: &[&str]) -> Vec<(String, String, String, String, String, u64)> {
    let mut rows: Vec<(String, String, String, String, String, u64)> = run(files)
        .iter()
        .filter(|row| row["record"] == "resolved_import" && row["kind"] != "module")
        .map(|row| {
            (
                text(row, "local"),
                text(row, "name"),
                stem(&text(row, "target_path")),
                text(row, "target_name"),
                text(row, "kind"),
                row["hops"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn stem(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".ts")
        .to_string()
}

const BARREL: &[&str] = &["barrel_consumer", "index", "helpers", "widgets", "other"];

/// A name imported through an `export * from` barrel binds to the ONE file the
/// barrel reaches, with a second file in the corpus spelling it the same way.
/// This case exists because it is the corpus shape: TypeScript 5.9 routes its
/// whole compiler through `src/compiler/_namespaces/ts.ts`, 73 star lines.
#[test]
fn a_barrel_import_binds_to_the_one_file_the_star_reaches() {
    assert_eq!(
        edges(BARREL),
        [
            (
                "run".to_string(),
                "helpers".to_string(),
                "normalize".to_string(),
                "import_resolve".to_string()
            ),
            (
                "run".to_string(),
                "widgets".to_string(),
                "widen".to_string(),
                "import_resolve".to_string()
            ),
        ]
    );
}

/// The record's own row for the same two bindings. `hops` is 2 (the barrel,
/// then the file that declares it) and `kind` is `star`, which is what tells a
/// consumer the binding came through the ambiguity-prone arm.
#[test]
fn a_barrel_import_states_its_hops_and_its_star_arm() {
    assert_eq!(
        imports(BARREL),
        [
            (
                "normalize".to_string(),
                "normalize".to_string(),
                "helpers".to_string(),
                "normalize".to_string(),
                "star".to_string(),
                2
            ),
            (
                "widen".to_string(),
                "widen".to_string(),
                "widgets".to_string(),
                "widen".to_string(),
                "star".to_string(),
                2
            ),
        ]
    );
}

/// COUNT test: one edge per WRITTEN binding, never one per name in scope. The
/// barrel exports two names and the consumer calls both, so the corpus has
/// exactly two import edges and two import rows; a plane that folded the
/// barrel's whole export set into the consumer would show more.
#[test]
fn the_corpus_has_exactly_one_edge_per_written_binding() {
    let rows = run(BARREL);
    let edge_count = rows
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .count();
    let import_count = rows
        .iter()
        .filter(|row| row["record"] == "resolved_import" && row["kind"] != "module")
        .count();
    assert_eq!((edge_count, import_count), (2, 2));
}

/// `export { inner as outer } from` is the spec's INDIRECT export entry: the
/// consumer asks for `outer`, the recursion asks the target for `inner`, and
/// the edge lands on the name the target actually declares.
#[test]
fn a_renaming_reexport_binds_to_the_declared_name() {
    assert_eq!(
        edges(&["renamed_consumer", "renamed_barrel", "renamed_source"]),
        [(
            "callIt".to_string(),
            "renamed_source".to_string(),
            "inner".to_string(),
            "import_resolve".to_string()
        )]
    );
    assert_eq!(
        imports(&["renamed_consumer", "renamed_barrel", "renamed_source"]),
        [(
            "outer".to_string(),
            "outer".to_string(),
            "renamed_source".to_string(),
            "inner".to_string(),
            "indirect".to_string(),
            2
        )]
    );
}

/// Two star hops. The recursion is what makes the plane an algorithm rather
/// than a one-level import table, and `hops` counts the modules walked.
#[test]
fn a_two_hop_barrel_recurses_to_the_declaration() {
    assert_eq!(
        edges(&[
            "two_hop_consumer",
            "two_hop_outer",
            "two_hop_middle",
            "two_hop_inner"
        ]),
        [(
            "reach".to_string(),
            "two_hop_inner".to_string(),
            "deep".to_string(),
            "import_resolve".to_string()
        )]
    );
    assert_eq!(
        imports(&[
            "two_hop_consumer",
            "two_hop_outer",
            "two_hop_middle",
            "two_hop_inner"
        ])[0]
            .5,
        3
    );
}

/// Spec step 6: two star arms offering DIFFERENT bindings for one name is
/// AMBIGUOUS, which is a link-time error in the language and a NAMED STOP here.
/// No edge, and an `unresolved` row so the absence is a fact rather than
/// silence. This case exists because the alternative (pick one) invents an
/// edge the language itself refuses to resolve.
#[test]
fn two_disagreeing_star_arms_are_ambiguous_and_say_so() {
    let files = &[
        "ambiguous_consumer",
        "ambiguous_barrel",
        "ambiguous_left",
        "ambiguous_right",
    ];
    assert_eq!(edges(files), [] as [(String, String, String, String); 0]);
    let stops: Vec<(String, String)> = run(files)
        .iter()
        .filter(|row| row["record"] == "unresolved")
        .map(|row| (text(row, "reason"), text(row, "detail")))
        .collect();
    assert_eq!(stops, [("ambiguous".to_string(), "collide".to_string())]);
}

/// `import * as ns` binds a MODULE, so `ns.member()` is an export lookup on
/// that module, not a name match on `member`. This case exists because the
/// receiver is the only thing separating a module member call from a method
/// call on an unknown object.
#[test]
fn a_namespace_import_member_call_is_an_export_lookup() {
    assert_eq!(
        edges(&["namespace_consumer", "namespace_target"]),
        [(
            "callMember".to_string(),
            "namespace_target".to_string(),
            "member".to_string(),
            "import_resolve".to_string()
        )]
    );
    assert_eq!(
        imports(&["namespace_consumer", "namespace_target"]),
        [(
            "space".to_string(),
            "*".to_string(),
            "namespace_target".to_string(),
            String::new(),
            "namespace".to_string(),
            1
        )]
    );
}

/// A default import asks the source module for `default`, whatever the local
/// name spells. This case exists because the local name carries NO information
/// about the target, so a name match cannot reach it at all.
#[test]
fn a_default_import_binds_through_the_default_export() {
    assert_eq!(
        edges(&["default_consumer", "default_target"]),
        [(
            "callDefault".to_string(),
            "default_target".to_string(),
            "theDefault".to_string(),
            "import_resolve".to_string()
        )]
    );
}

/// A module-private def and an exported def of the same name: the importer
/// gets the EXPORTED one through its import binding, and the private file's own
/// call keeps binding locally. This case exists because it is the corpus's
/// second-largest ambiguity bucket: 2,399 sites in TypeScript 5.9 `src/**`.
#[test]
fn a_module_private_def_does_not_shadow_an_import_across_files() {
    assert_eq!(
        edges(&["shadow_consumer", "shadow_export", "shadow_private"]),
        [
            (
                "check".to_string(),
                "shadow_export".to_string(),
                "isIdentifier".to_string(),
                "import_resolve".to_string()
            ),
            (
                "parse".to_string(),
                "shadow_private".to_string(),
                "isIdentifier".to_string(),
                "name_resolve".to_string()
            ),
        ]
    );
}

/// Two files star-exporting each other. The spec's resolveSet is the cycle
/// guard; without it the recursion does not terminate. This case exists because
/// a hang is the failure mode, and a hang has no assertion of its own.
#[test]
fn a_reexport_cycle_terminates_and_still_binds() {
    assert_eq!(
        edges(&["cycle_consumer", "cycle_a", "cycle_b"]),
        [(
            "walk".to_string(),
            "cycle_b".to_string(),
            "fromB".to_string(),
            "import_resolve".to_string()
        )]
    );
}

// ── the plane's cost ────────────────────────────────────────────────────────

const RATIO_BUDGET: f64 = 2.5;

/// A barrel corpus of `n` leaf modules, one barrel starring all of them, and
/// `n` consumers each importing one name through it. The shape that makes the
/// plane work hardest: every consumer's binding walks the whole star list.
fn barrel_corpus(dir: &std::path::Path, n: usize) -> Vec<String> {
    let dir = dir.join(format!("n{n}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let mut paths = Vec::new();
    let mut barrel = String::new();
    for index in 0..n {
        let leaf = dir.join(format!("leaf{index}.ts"));
        std::fs::write(
            &leaf,
            format!("export function pick{index}(n: number): number {{ return n + {index}; }}\n"),
        )
        .expect("leaf file");
        paths.push(leaf.to_string_lossy().into_owned());
        barrel.push_str(&format!("export * from \"./leaf{index}.js\";\n"));
    }
    let barrel_path = dir.join("barrel.ts");
    std::fs::write(&barrel_path, barrel).expect("barrel file");
    paths.push(barrel_path.to_string_lossy().into_owned());
    for index in 0..n {
        let consumer = dir.join(format!("use{index}.ts"));
        std::fs::write(
            &consumer,
            format!(
                "import {{ pick{index} }} from \"./barrel.js\";\n\nexport function call{index}(): number {{ return pick{index}({index}); }}\n"
            ),
        )
        .expect("consumer file");
        paths.push(consumer.to_string_lossy().into_owned());
    }
    paths
}

fn resolve_wall(args: &[String]) -> f64 {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg("--family")
        .arg("call")
        .args(args)
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
/// A ResolveExport that re-walked a barrel's star list per call site instead of
/// per binding would show up here as a quadratic, not as a wrong answer.
#[test]
fn barrel_resolve_wall_grows_linearly_with_file_count() {
    let dir = std::env::temp_dir().join("sprefa-extract-54-module-plane");
    std::fs::create_dir_all(&dir).expect("scratch root");
    let small = barrel_corpus(&dir, 200);
    let large = barrel_corpus(&dir, 400);
    let wall200 = resolve_wall(&small);
    let wall400 = resolve_wall(&large);
    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}
