use dl8::_4_comptime::load::read::read_dir_tree;
use dl8::_4_comptime::load::read::read_source_fact_files;
use dl8::_4_comptime::load::read::read_tsi_stream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

// From tests/_5_load_oracle.rs
// Oracle parity through the real binary. Every `oracle/load/*.json` holds one
// call v7 made to a project fact loader, frozen by `oracle/load/dump_load.pl`
// while v7 ran its own plunit files, compiled three projects, and answered a
// crafted probe per diagnostic code the corpus does not reach. `dl8 load`
// must print the same rows, basements, origins and diagnostics, and exit with
// the same code.
//
// FAIL-FIRST RECEIPT: on the base sha `dl8 load` is the `_0_load` stub, which
// prints nothing and exits 3, so `every_loader_case_matches_v7` fails on the
// first case with "no JSON on stdout" and the impure-edge test fails to link
// `dl8::_4_comptime::load::read`.
//
// `oracle/load/status.json` carries `skip` for cases the port does not answer
// yet (empty), and `checked_not_committed` for cases the freeze verified and
// kept out of git for size.

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/load")
}

fn status() -> serde_json::Value {
    let text = std::fs::read_to_string(oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn cases() -> Vec<PathBuf> {
    let status = status();
    let skip: Vec<String> = status["skip"]
        .as_array()
        .expect("status.json without skip")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect();
    let mut out: Vec<PathBuf> = std::fs::read_dir(oracle_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            name != "status.json" && !skip.contains(&name)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_loader_case_matches_v7() {
    let cases = cases();
    assert!(!cases.is_empty(), "no loader cases found");
    let mut failures = Vec::new();
    for path in &cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("load")
            .arg(path)
            .output()
            .unwrap();
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &case["expected"];
        let fields = want.as_object().expect("expected is an object");
        for field in fields.keys() {
            if got[field] != want[field] {
                failures.push(format!(
                    "{}: {field} differs\n  want: {}\n  got:  {}",
                    path.display(),
                    truncate(&want[field]),
                    truncate(&got[field])
                ));
            }
        }
        let want_code = match want.get("diagnostics").and_then(|d| d.as_array()) {
            Some(rows) if !rows.is_empty() => 1,
            _ => 0,
        };
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The byte edge, over the committed directory the pure cases were frozen
/// from. Each assertion ties an edge function to the case that carries the
/// same bytes as already-read data.
#[test]
fn the_byte_edge_reproduces_the_case_inputs() {
    let cases = oracle_dir().join("cases");

    let stream = cases.join("tsi/0_syntax_user.jsonl");
    let lines = read_tsi_stream(&stream).unwrap();
    assert_eq!(lines, frozen_lines("0_syntax_user.jsonl"));

    let broken = cases.join("tsi/1_broken.jsonl");
    assert_eq!(
        read_tsi_stream(&broken).unwrap(),
        frozen_lines("1_broken.jsonl")
    );

    let facts = cases.join("source_facts/1_expected.json");
    let read = read_source_fact_files(std::slice::from_ref(&facts));
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].0, facts);
    assert_eq!(
        read[0].1.as_ref().unwrap(),
        &frozen_text("source_facts/1_expected.json")
    );

    let tree = read_dir_tree(&cases.join("project")).unwrap();
    assert_eq!(
        tree,
        vec![
            cases.join("project/0_src/0_root.dl7"),
            cases.join("project/0_src/1_models/0_users.dl7"),
        ]
    );
}

/// The `lines` array of the committed `load_tsi_lines` case whose origin ends
/// in this file name.
fn frozen_lines(name: &str) -> Vec<String> {
    for path in cases() {
        let text = std::fs::read_to_string(&path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let input = &case["input"];
        if input["call"] != "load_tsi_lines" {
            continue;
        }
        let origin = input["origin"]["a"].as_str().unwrap_or_default();
        if !origin.ends_with(name) {
            continue;
        }
        return input["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["s"].as_str().unwrap().to_string())
            .collect();
    }
    panic!("no committed load_tsi_lines case for {name}");
}

/// The `text` of the committed `load_source_fact_texts` case whose path ends
/// in this suffix.
fn frozen_text(suffix: &str) -> String {
    for path in cases() {
        let text = std::fs::read_to_string(&path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let input = &case["input"];
        if input["call"] != "load_source_fact_texts" {
            continue;
        }
        for file in input["files"].as_array().unwrap() {
            let args = file["args"].as_array().unwrap();
            let path = args[0]["a"].as_str().unwrap_or_default();
            if path.ends_with(suffix) {
                return args[1]["s"].as_str().unwrap().to_string();
            }
        }
    }
    panic!("no committed load_source_fact_texts case for {suffix}");
}

fn truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}

// From tests/_6_comptime_oracle.rs
// Oracle parity through the real binary. Every `oracle/comptime/*.json` holds
// one comptime call v7 made, frozen by `oracle/comptime/dump_comptime.pl`.
//
// FAIL-FIRST RECEIPTS (2026-09-12), each applied, observed, reverted:
//
// 1. `_2_rounds.rs` stable arm returning `closure` instead of
//    `strip_intern_rows(u, &closure)`: `2_partial` exits 3 with
//    `replay compiler facts length differs`, so the replay guard catches a
//    wrong `CompilerFacts` before the byte compare ever runs.
// 2. `_5_finish.rs` `semantic_label` losing its `ref` arm: `2_partial`
//    reports `compiled differs`; `:/4` rows whose label is a reference drop
//    out of `TypeGraphFacts`.
//
// Receipts 3 to 5 covered `_4_host.rs` and the pre-erase relation list; both
// the file and the host planning rows are deleted.
//
// Three mutations did NOT fail and are recorded as uncovered paths:
// dropping `sum` from `graph_seeds` (no committed graph carries a `sum/1`
// node), leaving `compiler_round_seeds` unsorted (the evaluator inserts seeds
// into a set, so their order is unobservable), and dropping the
// `frozen_generated_rules` arm of the stability test (no committed round
// changes its generated rules without also changing its edges).

fn s6comptimeoracle_oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/comptime")
}

fn s6comptimeoracle_status() -> serde_json::Value {
    let text = std::fs::read_to_string(s6comptimeoracle_oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn skipped() -> Vec<String> {
    s6comptimeoracle_status()["skip"]
        .as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn s6comptimeoracle_cases() -> Vec<PathBuf> {
    let skip = skipped();
    let mut out: Vec<PathBuf> = std::fs::read_dir(s6comptimeoracle_oracle_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            name != "status.json" && !skip.contains(&name)
        })
        .collect();
    out.sort();
    out
}

fn s6comptimeoracle_truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}

#[test]
fn dl8_round_counts_match_v7() {
    let s6comptimeoracle_status = s6comptimeoracle_status();
    let rounds = s6comptimeoracle_status["round_counts"]
        .as_object()
        .expect("status.json round_counts");
    for path in s6comptimeoracle_cases() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        if case["entry"] != "evaluate_checked" {
            continue;
        }
        let want = rounds
            .get(&name)
            .unwrap_or_else(|| panic!("{name}: no round count in status.json"));
        // One v7 assemble_generated_program call is one compiler round; one v7
        // refreeze call is one source-refreeze pass.
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("comptime")
            .arg(&path)
            .output()
            .unwrap();
        let got: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let inner = got["rounds"]
            .as_array()
            .expect("rounds")
            .iter()
            .filter(|e| e["event"] == "assemble")
            .count();
        let outer = got["rounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["event"] == "refreeze")
            .count();
        assert_eq!(
            (want["v7_assemble"].as_u64(), want["v7_refreeze"].as_u64()),
            (Some(inner as u64), Some(outer as u64)),
            "{name}: round counts differ"
        );
    }
}

#[test]
fn every_status_number_is_present() {
    let s6comptimeoracle_status = s6comptimeoracle_status();
    let numbers = s6comptimeoracle_status["numbers"]
        .as_object()
        .expect("numbers");
    for key in [
        "calls_dumped",
        "byte_distinct",
        "committed",
        "committed_bytes",
        "checked_but_not_committed",
        "fixtures",
    ] {
        assert!(numbers.contains_key(key), "status.json lacks {key}");
    }
    let uncovered = s6comptimeoracle_status["diagnostic_reasons"]["no_case_reaches_it"]
        .as_object()
        .expect("no_case_reaches_it");
    for (reason, site) in uncovered {
        assert!(
            site.as_str().is_some_and(|s| s.contains(".pl:")),
            "{reason}: no throw site"
        );
    }
}

// From tests/_7_reify_oracle.rs
// Oracle parity through the real binary. Every `oracle/reify/*.json` holds one
// reifier, grapher or artifact-emitter call v7 made, frozen by
// `oracle/reify/dump_reify.pl`. `dl8 reify` must print the same rows, the same
// calls, the same artifact and diagnostics, and exit with the same code.
//
// `oracle/reify/status.json` carries a `skip` array of case file names whose
// arms are not ported yet, so the gate stays green and the debt is a number.
//
// FAIL-FIRST RECEIPTS (2026-09-12), each applied, observed, and reverted:
//
// 1. `_1_rows.rs` writes the atom `rule` instead of `level` for
//    `program_rule_kind` (`0_logical_program_reifier.pl:228`): every
//    `logical_program_rows` and `logical_program_calls*` case differs.
// 2. `_2_calls.rs` `constant_text` keeps the atom instead of running
//    `atom_string/2` (`:161`, `:166`, `:184`): every case carrying
//    `program_rule_kind`, `program_goal` or `program_dependency` calls differs.
// 3. `_3_graph.rs` `argument_target` wraps a `reference` target the way the
//    calls path does (`0a_logical_program_grapher.pl:115` versus
//    `0_logical_program_reifier.pl:113`): `case-17_argument_shapes_1` and
//    `_3` differ. This mutation passed before `cases/17_argument_shapes.pl`
//    existed; that case was authored because of it.
// 4. `_4_emit.rs` dl7 arm returns the bare `[]` rather than `artifacts([])`
//    when key validation fails (`1_artifact_emitter.pl:107`):
//    `case-8_emit_functional_key_conflict_5` reports `artifact differs`.
// 5. `_1_rows.rs` swaps the `argument_child(ArgumentId, input)` arguments
//    (`0_logical_program_reifier.pl:270`): `case-17_argument_shapes_0`, `_1`
//    and `_3` differ.

fn s7reifyoracle_oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/reify")
}

fn s7reifyoracle_status() -> serde_json::Value {
    let text = std::fs::read_to_string(s7reifyoracle_oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn s7reifyoracle_skipped() -> Vec<String> {
    s7reifyoracle_status()["skip"]
        .as_array()
        .expect("status.json without skip")
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn s7reifyoracle_cases() -> Vec<PathBuf> {
    let skip = s7reifyoracle_skipped();
    let mut out: Vec<PathBuf> = std::fs::read_dir(s7reifyoracle_oracle_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            name != "status.json" && !skip.contains(&name)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_reify_case_matches_v7() {
    let s7reifyoracle_cases = s7reifyoracle_cases();
    assert!(!s7reifyoracle_cases.is_empty(), "no reify cases found");
    let mut failures = Vec::new();
    for path in &s7reifyoracle_cases {
        let text = std::fs::read_to_string(path).unwrap();
        let case: serde_json::Value = serde_json::from_str(&text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("reify")
            .arg(path)
            .output()
            .unwrap();
        let got: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!(
                    "{}: no JSON on stdout ({e}); stderr: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
        };
        let want = &case["expected"];
        for (field, value) in want.as_object().expect("expected object") {
            if got.get(field) != Some(value) {
                failures.push(format!(
                    "{}: {field} differs\n  want: {}\n  got:  {}",
                    path.display(),
                    s7reifyoracle_truncate(value),
                    s7reifyoracle_truncate(got.get(field).unwrap_or(&serde_json::Value::Null))
                ));
            }
        }
        let want_code = i32::from(!want["diagnostics"].as_array().is_none_or(|d| d.is_empty()));
        if output.status.code() != Some(want_code) {
            failures.push(format!(
                "{}: exit {:?}, want {want_code}",
                path.display(),
                output.status.code()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_skipped_case_exists_and_names_its_v7_line() {
    let s7reifyoracle_status = s7reifyoracle_status();
    let notes = s7reifyoracle_status["not_ported"]
        .as_object()
        .expect("status.json without not_ported");
    for name in s7reifyoracle_skipped() {
        let path = s7reifyoracle_oracle_dir().join(&name);
        assert!(path.exists(), "{name} is skipped but not committed");
        assert!(
            notes.contains_key(&name),
            "{name} is skipped with no reason"
        );
    }
    for note in notes.values() {
        let note = note.as_str().expect("not_ported note");
        assert!(note.contains(".pl:"), "{note} names no v7 line");
    }
}

#[test]
fn every_diagnostic_reason_is_covered_or_explained() {
    let s7reifyoracle_status = s7reifyoracle_status();
    let reasons = &s7reifyoracle_status["diagnostic_reasons"];
    let covered = reasons["covered_by_authored_case"]
        .as_object()
        .expect("covered_by_authored_case");
    for (reason, note) in covered {
        let note = note.as_str().expect("reason note");
        let file = note.split(',').next().unwrap().trim();
        let path = s7reifyoracle_oracle_dir().join(file);
        assert!(path.exists(), "{reason}: {} is missing", path.display());
    }
    let total = covered.len() + reasons["frozen_but_not_ported"].as_object().unwrap().len();
    assert_eq!(total, 10, "every reason functor must be classified");
}

fn s7reifyoracle_truncate(value: &serde_json::Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(400) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text,
    }
}

// From tests/_8_compile_oracle.rs
// Whole-pipeline parity through the real binary. Each
// `oracle/compile/cases/*.json` is one v7 `compile_dl7/4` or
// `compile_dl7_project_rows/6` call frozen at `f5018ad23` by
// `oracle/compile/freeze.sh`; `dl8 compile` must print the same bytes and exit
// with the same code. `oracle/compile/status.json` indexes all 46 cases; the
// committed subset is whatever fits 6 MB, and `oracle/compile/verify.sh` runs
// dl8 over every one of them.
//
// Each case stores its `compiler_rows` and `runtime_program` as content-anchored
// hunks against `oracle/compile/baseline.json`, the empty program's output;
// `expected_of` rebuilds the full value before the comparison.
//
// The comparison is byte equality per frozen key: `serde_json::to_string` over
// a `Map` is a BTreeMap walk with no whitespace, the same bytes `dl8 compile`
// writes. v7 never printed `program`, so no case freezes it and the 6 MB
// budget below is what keeps it out.
//
// FAIL-FIRST RECEIPT (2026-09-12), applied, observed, reverted:
// `_2_lower/_12_units.rs::install_importer_aliases` starts the alias ordinal
// at 0 instead of `next_owner_index/3 + 1` (`0a_module_lowerer.pl:428`):
// `applications-dl6-0_catalog` differs and the suite is red.
//
// UNCOVERED MUTATIONS, applied and observed green, so the corpus does not
// price them: `_8_driver/_0_read.rs::join` with `""` instead of `"\n"` (every
// prelude file already ends in a newline and no prelude offset reaches the
// output), and dropping the `bound.contains(&name)` shadow guard at
// `0a_module_lowerer.pl:443` (no fixture rebinds a prelude name locally).

fn s8compileoracle_oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("oracle/compile")
}

fn sources_dir() -> String {
    s8compileoracle_oracle_dir()
        .join("sources")
        .display()
        .to_string()
}

/// `<repo>` stands for the crate root, for a case over a fixture outside `sources`.
fn repo_dir() -> String {
    env!("CARGO_MANIFEST_DIR").to_string()
}

fn s8compileoracle_status() -> serde_json::Value {
    let text = std::fs::read_to_string(s8compileoracle_oracle_dir().join("status.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn arguments(case: &serde_json::Value) -> Vec<String> {
    let sources = sources_dir();
    case["input"]["arguments"]
        .as_array()
        .expect("case without arguments")
        .iter()
        .map(|a| {
            a.as_str()
                .unwrap()
                .replace("<root>", &sources)
                .replace("<repo>", &repo_dir())
        })
        .collect()
}

/// Twin of `apply_hunks` in `oracle/refreeze.py`. Each hunk is anchored by
/// the baseline row it follows, found at or after the cursor.
fn apply_hunks(baseline: &[serde_json::Value], hunks: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut cursor = 0;
    for hunk in hunks {
        if !hunk["after"].is_null() {
            let found = cursor
                + baseline[cursor..]
                    .iter()
                    .position(|row| *row == hunk["after"])
                    .expect("hunk anchor is not a baseline row at or after the cursor");
            out.extend_from_slice(&baseline[cursor..=found]);
            cursor = found + 1;
        }
        out.extend(hunk["add"].as_array().unwrap().iter().cloned());
        let remove = hunk["remove"].as_array().unwrap();
        assert_eq!(
            &baseline[cursor..cursor + remove.len()],
            remove.as_slice(),
            "hunk removes rows the baseline does not hold"
        );
        cursor += remove.len();
    }
    out.extend_from_slice(&baseline[cursor..]);
    out
}

/// `oracle/compile/baseline.json` is the output of an empty program. A case
/// freezes `expected` keys verbatim and `delta` keys as hunks against it.
fn expected_of(case: &serde_json::Value, baseline: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut expected = case["expected"].as_object().unwrap().clone();
    let Some(delta) = case.get("delta") else {
        return expected;
    };
    let rows = |value: &serde_json::Value| value.as_array().unwrap().clone();
    let hunks = |value: &serde_json::Value| value.as_array().unwrap().clone();
    if let Some(compiler_rows) = delta.get("compiler_rows") {
        expected.insert(
            "compiler_rows".into(),
            apply_hunks(&rows(&baseline["compiler_rows"]), &hunks(compiler_rows)).into(),
        );
    }
    if let Some(sequences) = delta.get("runtime_program") {
        let mut term = baseline["runtime_program"].clone();
        for (path, list) in sequences.as_object().unwrap() {
            let at = term.pointer_mut(path).expect("delta path is not in the baseline");
            *at = apply_hunks(&rows(at), &hunks(list)).into();
        }
        expected.insert("runtime_program".into(), term);
    }
    expected
}

#[test]
fn every_committed_case_matches_v7() {
    let sources = sources_dir();
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(s8compileoracle_oracle_dir().join("baseline.json")).unwrap(),
    )
    .unwrap();
    let mut checked = 0;
    let mut rows = Vec::new();
    for entry in s8compileoracle_status()["cases"].as_array().unwrap() {
        if !entry["committed"].as_bool().unwrap() {
            continue;
        }
        let stem = entry["stem"].as_str().unwrap();
        let path = s8compileoracle_oracle_dir()
            .join("cases")
            .join(format!("{stem}.json"));
        let case: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let started = std::time::Instant::now();
        let run = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("compile")
            .args(arguments(&case))
            .output()
            .unwrap();
        let ms = started.elapsed().as_millis();
        let got = String::from_utf8(run.stdout).unwrap();
        let got = got
            .trim_end_matches('\n')
            .replace(&sources, "<root>")
            .replace(&repo_dir(), "<repo>");
        let got: serde_json::Value = serde_json::from_str(&got).unwrap();
        let expected = expected_of(&case, &baseline);
        for (key, want) in &expected {
            assert_eq!(
                serde_json::to_string(&got[key]).unwrap(),
                serde_json::to_string(want).unwrap(),
                "{stem}: {key} differs from v7"
            );
        }
        let expected_code = i32::from(!expected["diagnostics"].as_array().unwrap().is_empty());
        assert_eq!(
            run.status.code(),
            Some(expected_code),
            "{stem}: exit code differs from v7"
        );
        rows.push((stem.to_string(), ms, entry["v7_ms"].as_u64()));
        checked += 1;
    }
    assert!(checked > 0, "no committed cases");
    for (stem, ms, v7) in &rows {
        let v7 = v7.map_or("-".to_string(), |v7| v7.to_string());
        println!("{stem:58} dl8 {ms:>5} ms   v7 {v7:>5} ms");
    }
}

#[test]
fn every_committed_case_file_exists_and_every_case_is_indexed() {
    let cases = s8compileoracle_status();
    let cases = cases["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 45, "status.json case count");
    let mut committed = 0;
    for entry in cases {
        let stem = entry["stem"].as_str().unwrap();
        let path = s8compileoracle_oracle_dir()
            .join("cases")
            .join(format!("{stem}.json"));
        assert_eq!(
            path.exists(),
            entry["committed"].as_bool().unwrap(),
            "{stem}: committed flag and case file disagree"
        );
        if path.exists() {
            committed += std::fs::metadata(&path).unwrap().len();
        }
    }
    committed += std::fs::metadata(s8compileoracle_oracle_dir().join("baseline.json"))
        .unwrap()
        .len();
    // `oracle/compile/status.json` carries the same number for a reader.
    let budget = s8compileoracle_status()["budget_bytes"].as_u64().unwrap();
    assert!(
        committed <= budget,
        "committed bytes {committed} over budget {budget}"
    );
}

#[test]
fn the_curry_limit_is_a_case_not_a_skip() {
    let case: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            s8compileoracle_oracle_dir().join("cases/test-fixtures-5_curry.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let reason = &case["expected"]["diagnostics"][0]["args"][2];
    assert_eq!(reason["f"], "source_refreeze_limit_exhausted");
    assert_eq!(reason["args"][0], 16);
}
