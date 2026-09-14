//! `term_lt` parity through the real binary. Each `fixtures/term_lt/*.dl7`
//! compiles to a checked runtime program, which is transported into `dl8 eval`
//! and evaluated; the closure must match the sibling `*.expected.json`, written
//! by hand from the program text. The fixture path is replaced by `<fixture>`
//! so the expected files carry no absolute path.
//!
//! `dl8 compile` prints the `checked_datalog/4` term, whose
//! `datalog_program(_, Seeds, Rules)` holds the same `call/2` and
//! `checked_goal/2` shapes v7 `evaluate/4` takes. `program_json` is the port of
//! `oracle/eval/json_terms.pl`, the transport that shape needs.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn program_json(runtime_program: &Value) -> Result<Value, String> {
    let checked = runtime_program
        .get("args")
        .and_then(Value::as_array)
        .ok_or("runtime program is not checked_datalog/4")?;
    let program = checked
        .get(1)
        .ok_or("checked_datalog without datalog_program")?;
    let args = program
        .get("args")
        .and_then(Value::as_array)
        .ok_or("datalog_program is not a term")?;
    let seeds = args.get(1).ok_or("datalog_program without seeds")?;
    let rules = args.get(2).ok_or("datalog_program without rules")?;
    let seeds = seeds
        .as_array()
        .ok_or("seeds is not a list")?
        .iter()
        .map(call_json)
        .collect::<Result<Vec<_>, _>>()?;
    let rules = rules
        .as_array()
        .ok_or("rules is not a list")?
        .iter()
        .map(rule_json)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "program": { "rules": rules, "seeds": seeds } }))
}

/// `arg_json/2`: a variable becomes `{"v": identity}`, a count aggregate
/// `{"count": arg}`, every other argument its own term.
fn arg_json(value: &Value) -> Value {
    match value.get("f").and_then(Value::as_str) {
        Some("var") => match value.get("args").and_then(Value::as_array) {
            Some(args) if !args.is_empty() => json!({ "v": args[0] }),
            _ => value.clone(),
        },
        Some("aggregate") => match aggregate_count(value) {
            Some(inner) => json!({ "count": arg_json(&inner) }),
            None => value.clone(),
        },
        _ => value.clone(),
    }
}

fn aggregate_count(value: &Value) -> Option<Value> {
    let args = value.get("args").and_then(Value::as_array)?;
    if args.len() == 2 && args[0].get("a").and_then(Value::as_str) == Some("count") {
        return args.get(1).cloned();
    }
    None
}

/// `call_json/2`: `call(Rel, Args)` becomes `{rel, args}`.
fn call_json(value: &Value) -> Result<Value, String> {
    let args = value
        .get("args")
        .and_then(Value::as_array)
        .ok_or("call/2 expected")?;
    let rel = args.first().ok_or("call without relation")?.clone();
    let list = args
        .get(1)
        .and_then(Value::as_array)
        .ok_or("call without arguments")?;
    Ok(json!({
        "rel": rel,
        "args": list.iter().map(arg_json).collect::<Vec<_>>(),
    }))
}

/// `goal_json/2`: `checked_goal(Polarity, Call)` becomes the call plus its
/// polarity name.
fn goal_json(value: &Value) -> Result<Value, String> {
    let args = value
        .get("args")
        .and_then(Value::as_array)
        .ok_or("checked_goal/2 expected")?;
    let polarity = args
        .first()
        .and_then(|p| p.get("a"))
        .and_then(Value::as_str)
        .ok_or("goal without polarity")?;
    let mut call = call_json(args.get(1).ok_or("goal without call")?)?;
    call["polarity"] = json!(polarity);
    Ok(call)
}

/// `rule_json/2`: `rule(call, Goals)` becomes `{head, body}`.
fn rule_json(value: &Value) -> Result<Value, String> {
    let args = value
        .get("args")
        .and_then(Value::as_array)
        .ok_or("rule/2 expected")?;
    let head = call_json(args.first().ok_or("rule without head")?)?;
    let body = args
        .get(1)
        .and_then(Value::as_array)
        .ok_or("rule without body")?;
    let body = body
        .iter()
        .map(goal_json)
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({ "head": head, "body": body }))
}

/// The fixture path is machine-specific; the expected files spell it
/// `<fixture>`.
fn normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => Value::Array(items.iter().map(|v| normalize(v, path)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/term_lt");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "dl7"))
        .collect();
    out.sort();
    out
}

fn run(binary: &str, mode: &str, input: &Path) -> Result<Value, String> {
    let output = Command::new(binary)
        .arg(mode)
        .arg(input)
        .output()
        .map_err(|e| format!("{mode} spawn: {e}"))?;
    serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "{mode} no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// Compile one fixture, carry its runtime program through `dl8 eval`, and
/// compare the closure and diagnostics to the sibling expected file.
fn check_fixture(binary: &str, source: &Path) -> Result<(), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let compiled =
        run(binary, "compile", &canonical).map_err(|e| format!("{stem}: compile {e}"))?;
    let program = program_json(&compiled["runtime_program"]).map_err(|e| format!("{stem}: {e}"))?;
    let program_path =
        std::env::temp_dir().join(format!("dl8-term-lt-{}-{stem}.json", std::process::id()));
    std::fs::write(&program_path, serde_json::to_string(&program).unwrap()).unwrap();
    let got = run(binary, "eval", &program_path).map_err(|e| format!("{stem}: eval {e}"));
    let _ = std::fs::remove_file(&program_path);
    let got = normalize(&got?, &fixture_path);
    let expected_path = source.with_file_name(format!("{stem}.expected.json"));
    let text = std::fs::read_to_string(&expected_path).map_err(|e| format!("{stem}: {e}"))?;
    let want: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
    if got["closure"] != want["closure"] {
        return Err(format!(
            "{stem}: closure differs\n  want: {}\n  got:  {}",
            want["closure"], got["closure"]
        ));
    }
    if got["diagnostics"] != want["diagnostics"] {
        return Err(format!(
            "{stem}: diagnostics differ\n  want: {}\n  got:  {}",
            want["diagnostics"], got["diagnostics"]
        ));
    }
    Ok(())
}

#[test]
pub fn every_term_lt_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no term_lt fixtures found");
    let failures: Vec<String> = fixtures
        .iter()
        .filter_map(|source| check_fixture(binary, source).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
