//! The kinks the TypeScript 5.9 entrypoint crawl found
//! (`plans/extract-crawl-2026-08-29/ts5.REPORT.md` section 6), one test per
//! kink, each over the fixture the crawl minted under
//! `tests/fixtures/ts5_findings/`.

use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
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

/// `(caller_name, callee_name)` for every `resolved_edge`, sorted.
fn edges(args: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = run(args)
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| (text(row, "caller_name"), text(row, "callee_name")))
        .collect();
    out.sort();
    out
}

// ── kink 1: a module-level call site has no covering def ────────────────────

/// SABOTAGE RECEIPT (fail-pre-fix): before the synthetic `<module>` def,
/// `covering_def` returned None for the two module-scope sites and
/// `Resolve<CallF>` dropped them, so this asserted `[("insideFn", "entry")]`
/// alone. In TypeScript 5.9's `src/**` that bail loses 1,358 sites, all eight
/// of `src/tsc/tsc.ts` among them.
#[test]
fn a_module_level_call_site_is_credited_to_the_module() {
    assert_eq!(
        edges(&[
            "--resolve",
            "--family",
            "call",
            "tests/fixtures/ts5_findings/top_level_call.ts",
            "tests/fixtures/ts5_findings/top_level_callee.ts",
        ]),
        [
            ("<module>".to_string(), "entry".to_string()),
            ("<module>".to_string(), "entry".to_string()),
            ("insideFn".to_string(), "entry".to_string()),
        ]
    );
}

/// The `--resolve` caller key must exist as a `node` row in `--family call`,
/// or a crawl joining the two planes cannot pass through the module.
#[test]
fn the_module_def_is_a_node_row_spanning_the_file() {
    let source =
        std::fs::read("tests/fixtures/ts5_findings/top_level_call.ts").expect("fixture readable");
    let modules: Vec<Value> = run(&[
        "--family",
        "call",
        "tests/fixtures/ts5_findings/top_level_call.ts",
    ])
    .into_iter()
    .filter(|row| row["record"] == "node" && row["name"] == "<module>")
    .collect();

    assert_eq!(modules.len(), 1, "one module def per file that needs one");
    assert_eq!(text(&modules[0], "kind"), "module");
    assert_eq!(modules[0]["span"]["start"], 0);
    assert_eq!(modules[0]["span"]["end"], source.len());
}

/// The module def is minted only where the module scope owns a call site: v5
/// has no module-def facet, so an unconditional row would land a v6-only line
/// in the PORTED `call_def` facet and break every captured oracle in
/// `tests/golden_parity.rs`.
#[test]
fn a_file_with_no_module_level_call_site_mints_no_module_def() {
    let modules = run(&["--family", "call", "tests/fixtures/ts/sample.ts"])
        .iter()
        .filter(|row| row["record"] == "node" && row["name"] == "<module>")
        .count();
    assert_eq!(modules, 0);
}

// ── kink 3: a member call on an unknown receiver ────────────────────────────

/// SABOTAGE RECEIPT (fail-pre-fix): `call_name_match` reads only the trailing
/// segment, so `out.push(x)` bound to the free `push` in `tracing_like.ts` and
/// this asserted one `("collect", "push")` edge. `src/compiler/tracing.ts:push`
/// alone captures 2,064 array pushes over TypeScript 5.9's `src/**`.
#[test]
fn an_array_push_does_not_bind_to_a_free_function_named_push() {
    assert_eq!(
        edges(&[
            "--resolve",
            "--family",
            "call",
            "tests/fixtures/ts5_findings/receiver_blind_prototype.ts",
            "tests/fixtures/ts5_findings/tracing_like.ts",
        ]),
        [] as [(String, String); 0]
    );
}

/// The RESIDUE the narrowed rule accepts, and the PR #538 face of the kink: an
/// unrelated `push` that is a class METHOD keeps its edge. Blocking it too
/// costs 5 defs of entrypoint reachability over TypeScript 5.9 and drops 290
/// more edges (report section 9.2); the coordinator's rule is method-or-nothing,
/// so this asserts the accepted false positive rather than pretending it is gone.
#[test]
fn an_array_push_still_binds_to_a_class_method_named_push() {
    assert_eq!(
        edges(&[
            "--resolve",
            "--family",
            "call",
            "tests/fixtures/ts_findings/receiver_blind_method/consumer.ts",
            "tests/fixtures/ts_findings/receiver_blind_method/writer.ts",
        ]),
        [
            ("collect".to_string(), "push".to_string()),
            ("collect".to_string(), "push".to_string()),
        ]
    );
}

/// A member call on an unknown receiver whose callee is NOT a builtin member
/// name keeps its name match: `program.getTypeChecker()` is 173 edges over
/// TypeScript 5.9 and every one of them is right.
#[test]
fn a_non_builtin_member_name_keeps_its_match() {
    assert_eq!(
        edges(&[
            "--resolve",
            "--family",
            "call",
            "tests/fixtures/ts5_findings/user_api_receiver/consumer.ts",
            "tests/fixtures/ts5_findings/user_api_receiver/program.ts",
        ]),
        [("run".to_string(), "getTypeChecker".to_string())]
    );
}

/// The rule keys on the RECEIVER, not on member-ness: a namespace-import
/// binding and `this` both name a scope this file can see.
#[test]
fn a_namespace_import_receiver_and_this_still_resolve() {
    assert_eq!(
        edges(&[
            "--resolve",
            "--family",
            "call",
            "tests/fixtures/ts5_findings/known_receiver/consumer.ts",
            "tests/fixtures/ts5_findings/known_receiver/ns.ts",
        ]),
        [
            ("run".to_string(), "normalize".to_string()),
            ("run".to_string(), "tidy".to_string()),
        ]
    );
}

/// The receiver reaches `Resolve<CallF>` through `callee_path`, the seat the
/// 4a site-key discipline reserved for it (`CallSite`, src/types.rs:467-472:
/// "ts/go emit None today and catch up with their resolve arms").
#[test]
fn a_member_call_site_carries_its_path_as_written() {
    let paths: Vec<String> = run(&[
        "--family",
        "call",
        "tests/fixtures/ts5_findings/receiver_blind_prototype.ts",
    ])
    .iter()
    .filter(|row| row["record"] == "site")
    .map(|row| text(row, "callee_path"))
    .collect();
    assert_eq!(paths, ["out.push".to_string()]);
}

// ── kink 4: a function named as a value ─────────────────────────────────────

/// SABOTAGE RECEIPT (fail-pre-fix): only Call/New/JSX minted a row, so
/// `table.push(handler)` put nothing in the call plane naming `handler` and
/// this read `[]`. Five of the ten largest unreachable defs in the TypeScript
/// 5.9 entrypoint crawl are transformers registered exactly this way.
#[test]
fn a_function_named_as_a_value_mints_a_reference_row() {
    let refs: Vec<(String, String)> = run(&[
        "--family",
        "call",
        "tests/fixtures/ts5_findings/function_ref_as_value.ts",
    ])
    .iter()
    .filter(|row| row["record"] == "reference")
    .map(|row| (text(row, "functor"), text(row, "position")))
    .collect();
    assert_eq!(refs, [("handler".to_string(), "value".to_string())]);
}

/// The resolve leg: the reference resolves to a `value_ref` edge from the def
/// that names it, so a crawl walks INTO `handler` from `register`.
#[test]
fn a_value_reference_resolves_to_a_value_ref_edge() {
    let rows: Vec<(String, String, String)> = run(&[
        "--resolve",
        "--family",
        "call",
        "tests/fixtures/ts5_findings/function_ref_as_value.ts",
    ])
    .iter()
    .filter(|row| row["record"] == "resolved_edge")
    .map(|row| {
        (
            text(row, "caller_name"),
            text(row, "callee_name"),
            text(row, "kind"),
        )
    })
    .collect();
    assert_eq!(
        rows,
        [(
            "register".to_string(),
            "handler".to_string(),
            "value_ref".to_string()
        )]
    );
}

/// The row is minted for a name this file DECLARES or IMPORTS. A bare argument
/// that names a parameter names no def anywhere, and would be one row per
/// argument over the whole corpus.
#[test]
fn a_local_named_as_an_argument_mints_no_reference_row() {
    let refs = run(&[
        "--family",
        "call",
        "tests/fixtures/ts5_findings/known_receiver/consumer.ts",
    ])
    .iter()
    .filter(|row| row["record"] == "reference")
    .count();
    assert_eq!(refs, 0);
}

// ── kink 5: --scip-facts needs a git worktree ───────────────────────────────

/// SABOTAGE RECEIPT (fail-pre-fix): `--help` never named the requirement, so
/// running the flag over a scratch copy of a corpus died on
/// `Error: Read(".", ... ". is not inside a Git worktree")` with nothing in the
/// help pointing at the cause.
#[test]
fn scip_facts_help_names_the_git_worktree_requirement() {
    let help = String::from_utf8(
        Command::new(env!("CARGO_BIN_EXE_extract"))
            .arg("--help")
            .output()
            .expect("extract binary runs")
            .stdout,
    )
    .expect("help is UTF-8");
    let line = help
        .lines()
        .find(|line| line.contains("Git worktree"))
        .unwrap_or_else(|| panic!("--help never names the Git worktree requirement"));
    assert!(
        line.contains("--project-root"),
        "the requirement is on --project-root, not on the flag: {line}"
    );
}
