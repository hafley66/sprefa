//! `effect` rows for served relations, through the real binary. Each
//! `fixtures/host_effect/*.dl7` compiles to a checked runtime program, which is
//! transported into `dl8 eval --serve <name>` and evaluated; the closure must
//! match the sibling `*.expected.json`. The fixture path is replaced by
//! `<fixture>` so the expected files carry no absolute path.
//!
//! The served set is a runtime input, so a program run without `--serve` writes
//! no `effect` row at all; `4_unserved` pins that, and with it every oracle case.
//!
//! `dl8 compile` prints the `checked_datalog/4` term, whose
//! `root_graph(Nodes, Edges)` carries the module-level `:/4` binds this
//! transport turns into the `names` table `--serve` resolves against.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which relations each fixture asks the outside to settle.
pub const SERVED: [(&str, &str); 5] = [
    ("0_pending", "fetch_json"),
    ("1_settled", "fetch_json"),
    ("2_source", "tick"),
    ("3_loading", "fetch_json"),
    ("4_unserved", ""),
];

fn program_json(runtime_program: &Value) -> Result<Value, String> {
    let checked = runtime_program
        .get("args")
        .and_then(Value::as_array)
        .ok_or("runtime program is not checked_datalog/4")?;
    let graph = checked
        .first()
        .ok_or("checked_datalog without root_graph")?;
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
    let names = module_names(graph)?;
    Ok(json!({ "program": { "rules": rules, "seeds": seeds, "names": names } }))
}

/// `compound(name, args)` when the term is that functor with that arity.
pub fn functor<'a>(value: &'a Value, name: &str, arity: usize) -> Option<&'a Vec<Value>> {
    if value.get("f").and_then(Value::as_str) != Some(name) {
        return None;
    }
    value
        .get("args")
        .and_then(Value::as_array)
        .filter(|args| args.len() == arity)
}

/// `unary(Inner)`.
pub fn unary<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    functor(value, name, 1).map(|args| &args[0])
}

/// Every `:(module(_), Name, ref(Relation), _)` bind as `name -> ref(Relation)`.
pub fn module_names(graph: &Value) -> Result<Value, String> {
    let edges = graph
        .get("args")
        .and_then(Value::as_array)
        .and_then(|args| args.get(1))
        .and_then(Value::as_array)
        .ok_or("root_graph without edges")?;
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    for edge in edges {
        let Some(args) = functor(edge, ":", 4) else {
            continue;
        };
        if unary(&args[0], "module").is_none() {
            continue;
        }
        let (Some(name), true) = (
            args[1].get("a").and_then(Value::as_str),
            unary(&args[2], "ref").is_some(),
        ) else {
            continue;
        };
        out.insert(name.to_string(), args[2].clone());
    }
    Ok(Value::Object(out.into_iter().collect()))
}

/// `arg_json/2`: a variable becomes `{"v": identity}`, every other argument its
/// own term.
fn arg_json(value: &Value) -> Value {
    match value.get("f").and_then(Value::as_str) {
        Some("var") => match value.get("args").and_then(Value::as_array) {
            Some(args) if !args.is_empty() => json!({ "v": args[0] }),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
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
                .map(|(key, v)| (key.clone(), normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn fixture_path(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/host_effect")
        .join(format!("{stem}.dl7"))
}

fn run(binary: &str, args: &[&str]) -> Result<(Value, i32), String> {
    let output = Command::new(binary)
        .args(args)
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    let value = serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    Ok((value, output.status.code().unwrap_or(-1)))
}

/// Compile one fixture and write its transported program to a temporary file.
fn transport(binary: &str, source: &Path, stem: &str) -> Result<PathBuf, String> {
    let compiled = run(binary, &["compile", &source.to_string_lossy()])
        .map_err(|e| format!("{stem}: compile {e}"))?
        .0;
    let program = program_json(&compiled["runtime_program"]).map_err(|e| format!("{stem}: {e}"))?;
    let path = std::env::temp_dir().join(format!("dl8-effect-{}-{stem}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_string(&program).unwrap()).unwrap();
    Ok(path)
}

/// Compile one fixture, evaluate it with its served set, and compare the
/// closure and diagnostics to the sibling expected file.
fn check_fixture(binary: &str, stem: &str, served: &str) -> Result<(), String> {
    let source = fixture_path(stem);
    let canonical = std::fs::canonicalize(&source).map_err(|e| format!("{stem}: {e}"))?;
    let program_path = transport(binary, &canonical, stem)?;
    let program = program_path.to_string_lossy().to_string();
    let mut args = vec!["eval", program.as_str()];
    if !served.is_empty() {
        args.push("--serve");
        args.push(served);
    }
    let got = run(binary, &args).map_err(|e| format!("{stem}: eval {e}"));
    let _ = std::fs::remove_file(&program_path);
    let got = normalize(&got?.0, &canonical.to_string_lossy());
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
pub fn every_effect_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let failures: Vec<String> = SERVED
        .iter()
        .filter_map(|(stem, served)| check_fixture(binary, stem, served).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
pub fn serving_a_name_the_program_does_not_declare_is_a_diagnostic() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let canonical = std::fs::canonicalize(fixture_path("0_pending")).unwrap();
    let program_path = transport(binary, &canonical, "0_pending").unwrap();
    let program = program_path.to_string_lossy().to_string();
    let (got, code) = run(binary, &["eval", &program, "--serve", "fetch_jsonn"]).unwrap();
    let _ = std::fs::remove_file(&program_path);
    assert_eq!(code, 1, "exit code");
    assert_eq!(got["closure"], json!([]), "closure");
    assert_eq!(
        got["diagnostics"],
        json!([{
            "phase": "eval",
            "payload": {"f": "served_relation_unknown", "args": [{"a": "fetch_jsonn"}]}
        }])
    );
}
