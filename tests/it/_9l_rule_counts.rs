//! `DL8_RULES=interpret` against `compile` on both engines: output parity and work counts for
//! the registry build, every census program, every census runtime program, the macrotime oracle,
//! and one rule edit on one installed session. Ignored by default; run with
//! `cargo test --test it _9l_rule_counts::rule_interpreter_counts -- --ignored --nocapture`.

use dl8::_6_eval::ir_eval::IrSession;
use dl8::_6_eval::json::{program_from_json, program_to_json};
use dl8::_6_eval::{Program, Trace, Universe};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const CONFIGS: [(&str, &str); 4] = [
    ("ir:sqlite", "compile"),
    ("ir:sqlite", "interpret"),
    ("ir:dd", "compile"),
    ("ir:dd", "interpret"),
];
const LOG: &str = "ivm_sqlite::work=info,ivm_dd=debug,dl8::ir=info,dl8::rules=info,probe=info";
const EDIT: &str = "plans/v8/probes/2026-10-05-rule-edit";

type Counts = BTreeMap<String, u64>;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl8-rule-counts-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fields(text: &str) -> BTreeMap<&str, &str> {
    text.split_whitespace().filter_map(|token| token.split_once('=')).collect()
}

/// Adds the work events of `stderr` to `counts`; with `step`, only the lines between
/// `probe step="<step>"` and the next probe line.
fn add(counts: &mut Counts, stderr: &str, step: Option<&str>) {
    let mut inside = step.is_none();
    for line in stderr.lines() {
        if line.contains("probe: probe") {
            if let Some(step) = step {
                inside = line.contains(&format!("step=\"{step}\""));
            }
            continue;
        }
        if !inside {
            continue;
        }
        let mut bump = |key: String, value: &str| {
            if let Ok(value) = value.parse::<u64>() {
                *counts.entry(key).or_default() += value;
            }
        };
        if let Some((_, rest)) = line.split_once("ivm_sqlite::work: sqlite work ") {
            let f = fields(rest);
            let phase = f.get("phase").copied().unwrap_or("?").trim_matches('"').to_owned();
            for (key, value) in &f {
                if *key != "phase" {
                    bump(format!("sqlite {phase} {key}"), value);
                    bump(format!("sqlite total {key}"), value);
                }
            }
        } else if let Some((_, rest)) = line.split_once("ivm_dd: dd install ") {
            let f = fields(rest);
            bump("dd install events".into(), "1");
            for key in ["operators", "nodes", "arrangements"] {
                bump(format!("dd install {key}"), f.get(key).copied().unwrap_or("0"));
            }
        } else if let Some((_, rest)) = line.split_once("ivm_dd: dd settle ") {
            let f = fields(rest);
            bump("dd settle events".into(), "1");
            for key in ["changes_in", "changes_out", "epochs", "steps", "arranged_records"] {
                bump(format!("dd settle {key}"), f.get(key).copied().unwrap_or("0"));
            }
        } else if let Some((_, rest)) = line.split_once("dl8::ir: ir install ") {
            bump("ir installs".into(), "1");
            bump("ir install nodes".into(), fields(rest).get("nodes").copied().unwrap_or("0"));
        } else if let Some((_, rest)) = line.split_once("dl8::rules: rules mode ") {
            let f = fields(rest);
            let mode = f.get("mode").copied().unwrap_or("?").trim_matches('"').to_owned();
            bump(format!("rules evaluations {mode}"), "1");
            for key in ["rules", "interpreted", "compiled"] {
                bump(format!("rules {key}"), f.get(key).copied().unwrap_or("0"));
            }
        }
    }
}

fn dl8(engine: &str, mode: &str, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root())
        .args(args)
        .env("DL8_ENGINE", engine)
        .env("DL8_RULES", mode)
        .env("RUST_LOG", LOG)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn table(title: &str, columns: &[Counts]) -> String {
    let names: BTreeSet<&String> = columns.iter().flat_map(|c| c.keys()).collect();
    let mut out = format!("### {title}\n\n| count | sqlite compile | sqlite interpret | dd compile | dd interpret |\n|---|---|---|---|---|\n");
    for name in names {
        let cells: Vec<String> = columns.iter().map(|c| c.get(name).copied().unwrap_or(0).to_string()).collect();
        out.push_str(&format!("| {name} | {} |\n", cells.join(" | ")));
    }
    out
}

fn dl7_files() -> Vec<String> {
    let mut found = Vec::new();
    let mut todo: Vec<PathBuf> = ["fixtures", "examples", "book/src/probes", "std"].iter().map(|d| root().join(d)).collect();
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path);
            } else if path.extension().is_some_and(|e| e == "dl7") {
                found.push(path.strip_prefix(root()).unwrap().display().to_string());
            }
        }
    }
    found.sort();
    found
}

/// Pretty `dl8 eval` output without its top-level `counters` object, which counts work.
fn without_counters(stdout: &str) -> String {
    let mut out = Vec::new();
    let mut skipping = false;
    for line in stdout.lines() {
        if line == "  \"counters\": {" {
            skipping = true;
            continue;
        }
        if skipping {
            skipping = !(line == "  }," || line == "  }");
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

fn runtime(u: &mut Universe, path: &Path) -> Program {
    let compiled = dl8::compile(u, path, &mut |_| {}).unwrap();
    assert!(compiled.diagnostics.is_empty(), "{}", path.display());
    let json = program_to_json(u, compiled.runtime_program).unwrap();
    program_from_json(u, &json).unwrap()
}

/// The edit, run in a child process of this test binary so its log is the child's stderr.
#[test]
#[ignore]
fn rule_edit_child() {
    if std::env::var_os("DL8_EDIT_CHILD").is_none() {
        return;
    }
    let config = hafley_observe::Config::from_env("dl8-rule-edit", "0", "warn", false).unwrap();
    hafley_observe::init(config).unwrap();
    std::env::set_current_dir(root().join(EDIT)).unwrap();
    let mut u = Universe::new();
    let first = runtime(&mut u, Path::new("1_reach.dl7"));
    let second = runtime(&mut u, Path::new("2_reach_edited.dl7"));
    let mut session = IrSession::configured();
    for (step, program) in [("before", &first), ("edit", &second)] {
        tracing::info!(target: "probe", step, "probe");
        let closure = session.evaluate(&mut u, program, None, &mut |_: Trace| {});
        tracing::info!(target: "probe", step = "done", "probe");
        let mut rows: Vec<String> = closure
            .rows
            .iter()
            .map(|row| {
                let args: Vec<String> = row.args.iter().map(|a| u.display(*a).to_string()).collect();
                format!("{}({})", u.display(row.rel), args.join(","))
            })
            .collect();
        rows.sort();
        println!("EDIT {step} {} {}", closure.diagnostics.len(), rows.join(";"));
    }
}

#[test]
#[ignore]
fn rule_interpreter_counts() {
    let mut report = String::new();

    // Registry build: `cmp` with hafley_scm's registry.rs.
    let oracle = std::fs::read_to_string(root().join("hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs")).unwrap();
    let mut columns = Vec::new();
    for (engine, mode) in CONFIGS {
        let out = scratch(&format!("registry-{}-{mode}", &engine[3..]));
        let (code, _, stderr) = dl8(engine, mode, &["build", "std/tsi_registry_emit.dl7", "--out", out.to_str().unwrap()]);
        assert_eq!(code, 0, "{engine} {mode}: {stderr}");
        let written = std::fs::read_to_string(out.join("target/tsi/registry.rs")).unwrap();
        assert!(written == oracle, "{engine} {mode}: registry.rs differs");
        let mut counts = Counts::new();
        add(&mut counts, &stderr, None);
        columns.push(counts);
    }
    report.push_str(&table("registry (`dl8 build std/tsi_registry_emit.dl7`, cmp identical in all four)", &columns));

    // Census: `dl8 compile` of every program; exit code and stdout equal across the four runs.
    let files = dl7_files();
    let mut runs: Vec<Vec<(i32, String, String)>> = Vec::new();
    for (engine, mode) in CONFIGS {
        runs.push(files.iter().map(|file| dl8(engine, mode, &["compile", file])).collect());
    }
    let mut ok = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let first = &runs[0][index];
        for (config, run) in CONFIGS.iter().zip(&runs) {
            assert_eq!((run[index].0, &run[index].1), (first.0, &first.1), "{file}: {config:?} differs from sqlite compile");
        }
        if first.0 == 0 {
            ok.push(index);
        }
    }
    let columns: Vec<Counts> = runs
        .iter()
        .map(|run| {
            let mut counts = Counts::new();
            for &index in &ok {
                add(&mut counts, &run[index].2, None);
            }
            counts
        })
        .collect();
    report.push_str(&format!(
        "\n{}\n{} programs, {} exit 0 in all four runs; output equal in all four.\n",
        table("census (`dl8 compile`, summed over programs exiting 0)", &columns),
        files.len(),
        ok.len()
    ));

    // Runtime programs of the census: `dl8 eval`, closure and diagnostics equal across the four.
    let dir = scratch("runtime");
    let mut programs = Vec::new();
    let mut too_deep = 0;
    for &index in &ok {
        let Ok(value) = serde_json::from_str::<Value>(&runs[0][index].1) else {
            too_deep += 1;
            continue;
        };
        if value["program"]["rules"].is_array() {
            let path = dir.join(format!("{index}.json"));
            std::fs::write(&path, serde_json::to_vec(&serde_json::json!({ "program": value["program"] })).unwrap()).unwrap();
            programs.push((files[index].clone(), path));
        }
    }
    let mut columns = vec![Counts::new(); 4];
    for (file, path) in &programs {
        let mut reference = None;
        for (config, counts) in CONFIGS.iter().zip(columns.iter_mut()) {
            let (code, stdout, stderr) = dl8(config.0, config.1, &["eval", path.to_str().unwrap()]);
            let got = (code, without_counters(&stdout));
            match &reference {
                None => reference = Some(got),
                Some(want) => assert_eq!(&got, want, "{file}: runtime {config:?}"),
            }
            add(counts, &stderr, None);
        }
    }
    report.push_str(&format!(
        "\n{}\n{} runtime programs; `dl8 eval` output less its `counters` field equal in all four. {} census outputs past serde_json's depth limit are not re-read.\n",
        table("runtime programs (`dl8 eval`)", &columns),
        programs.len(),
        too_deep
    ));

    // Macrotime oracle: `dl8 expand` stdout equal across the four.
    let mut cases: Vec<String> = std::fs::read_dir(root().join("oracle/macrotime"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path().strip_prefix(root()).unwrap().display().to_string())
        .filter(|path| path.ends_with(".json") && path.rsplit('_').next().is_some_and(|tail| tail.trim_end_matches(".json").parse::<u32>().is_ok()))
        .collect();
    cases.sort();
    for case in &cases {
        let outputs: Vec<(i32, String)> = CONFIGS.iter().map(|(engine, mode)| {
            let (code, stdout, _) = dl8(engine, mode, &["expand", case]);
            (code, stdout)
        }).collect();
        assert!(outputs.iter().all(|output| output == &outputs[0]), "{case}: expand differs");
    }
    report.push_str(&format!("\nmacrotime oracle: {} cases, `dl8 expand` output equal in all four.\n", cases.len()));

    // One rule edit on one installed session.
    let mut before = Vec::new();
    let mut edit = Vec::new();
    let mut rows: Option<String> = None;
    for (engine, mode) in CONFIGS {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["_9l_rule_counts::rule_edit_child", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
            .env("DL8_EDIT_CHILD", "1")
            .env("DL8_ENGINE", engine)
            .env("DL8_RULES", mode)
            .env("RUST_LOG", LOG)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(output.status.success(), "{engine} {mode}: {stderr}");
        let edit_rows: String = stdout.lines().filter(|line| line.starts_with("EDIT ")).collect::<Vec<_>>().join("\n");
        match &rows {
            None => rows = Some(edit_rows),
            Some(want) => assert_eq!(&edit_rows, want, "{engine} {mode}: edit rows"),
        }
        let mut counts = Counts::new();
        add(&mut counts, &stderr, Some("before"));
        before.push(counts);
        let mut counts = Counts::new();
        add(&mut counts, &stderr, Some("edit"));
        edit.push(counts);
    }
    report.push_str(&format!("\n{}\n{}\nedit closures equal in all four.\n", table("rule edit: first evaluation", &before), table("rule edit: the edit only", &edit)));
    println!("{report}");
}
