use dl8::_6_eval::evaluate::evaluate;
use dl8::_6_eval::evaluate::Trace;
use dl8::_6_eval::json::program_from_json;
use dl8::_6_eval::Program;
use dl8::_6_eval::TermId;
use dl8::_6_eval::Universe;
use serde_json::json;
use serde_json::Value;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

// From tests/_10_literals.rs
// Literal parity through the real binary. Each `fixtures/literals/*.dl7`
// compiles to a checked runtime program, which is transported into `dl8 eval`
// and evaluated; the closure must match the sibling `*.expected.json`, written
// by hand from the program text. The fixture path is replaced by `<fixture>`
// so the expected files carry no absolute path.
//
// `dl8 compile` prints the `program` object `dl8 eval` reads; the compile
// output file goes into `dl8 eval` unchanged, and no test rebuilds it.

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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/literals");
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

/// `dl8 compile`'s whole stdout is the program file; `dl8 eval` reads the
/// `program` key out of it.
fn compile_to(binary: &str, source: &Path, program: &Path) -> Result<(), String> {
    let output = Command::new(binary)
        .arg("compile")
        .arg(source)
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    if output.stdout.is_empty() {
        return Err(format!(
            "no stdout (exit {:?}); stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::write(program, &output.stdout).map_err(|e| format!("{e}"))
}

/// Compile one fixture, carry its runtime program through `dl8 eval`, and
/// compare the closure and diagnostics to the sibling expected file.
fn check_fixture(binary: &str, source: &Path) -> Result<(), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let program_path =
        std::env::temp_dir().join(format!("dl8-literals-{}-{stem}.json", std::process::id()));
    compile_to(binary, &canonical, &program_path).map_err(|e| format!("{stem}: compile {e}"))?;
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
pub fn every_literal_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no literal fixtures found");
    let failures: Vec<String> = fixtures
        .iter()
        .filter_map(|source| check_fixture(binary, source).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
pub fn float_and_bool_tokens_are_shape_checked() {
    use dl8::_0_read::tokens::{bool_token, float_token, integer_token};
    assert!(float_token("1.5") && float_token("-0.25"));
    assert!(!float_token("1.") && !float_token(".5") && !float_token("1.2.3"));
    assert!(!integer_token("1.5"));
    assert!(bool_token("true") && bool_token("false"));
    assert!(!bool_token("True") && !bool_token("tru"));
}

#[test]
pub fn term_order_puts_numbers_then_bools() {
    use dl8::_6_eval::Universe;
    use std::cmp::Ordering;
    let mut u = Universe::new();
    let integer = u.int(2);
    let float = u.float(2.0);
    let boolean = u.boolean(false);
    let string = u.string("z");
    let atom = u.atom("a");
    assert_eq!(u.cmp(integer, float), Ordering::Less);
    assert_eq!(u.cmp(float, integer), Ordering::Greater);
    assert_eq!(u.cmp(float, boolean), Ordering::Less);
    assert_eq!(u.cmp(boolean, string), Ordering::Less);
    assert_eq!(u.cmp(string, atom), Ordering::Less);
}

// From tests/_11_aggregates.rs
// Aggregate-head parity through the real binary. Each
// `fixtures/aggregates/*.dl7` compiles to a checked runtime program, which is
// transported into `dl8 eval` and evaluated; the closure must match the sibling
// `*.expected.json`, written from the program text. The fixture path is
// replaced by `<fixture>` so the expected files carry no absolute path.
//
// `dl8 compile` prints the `program` object `dl8 eval` reads; the compile
// output file goes into `dl8 eval` unchanged, and no test rebuilds it.

/// The fixture path is machine-specific; the expected files spell it
/// `<fixture>`.
fn s11aggregates_normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| s11aggregates_normalize(v, path))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| (key.clone(), s11aggregates_normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn s11aggregates_fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/aggregates");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "dl7"))
        .collect();
    out.sort();
    out
}

fn s11aggregates_run(binary: &str, mode: &str, input: &Path) -> Result<Value, String> {
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

/// `dl8 compile`'s whole stdout is the program file; `dl8 eval` reads the
/// `program` key out of it.
fn s11aggregates_compile_to(binary: &str, source: &Path, program: &Path) -> Result<(), String> {
    let output = Command::new(binary)
        .arg("compile")
        .arg(source)
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    if output.stdout.is_empty() {
        return Err(format!(
            "no stdout (exit {:?}); stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::write(program, &output.stdout).map_err(|e| format!("{e}"))
}

/// Compile one fixture, carry its runtime program through `dl8 eval`, and
/// compare the closure and diagnostics to the sibling expected file.
fn s11aggregates_check_fixture(binary: &str, source: &Path) -> Result<(), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let program_path =
        std::env::temp_dir().join(format!("dl8-aggregates-{}-{stem}.json", std::process::id()));
    s11aggregates_compile_to(binary, &canonical, &program_path)
        .map_err(|e| format!("{stem}: compile {e}"))?;
    let got =
        s11aggregates_run(binary, "eval", &program_path).map_err(|e| format!("{stem}: eval {e}"));
    let _ = std::fs::remove_file(&program_path);
    let got = s11aggregates_normalize(&got?, &fixture_path);
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
pub fn every_aggregate_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let s11aggregates_fixtures = s11aggregates_fixtures();
    assert!(
        !s11aggregates_fixtures.is_empty(),
        "no aggregate fixtures found"
    );
    let failures: Vec<String> = s11aggregates_fixtures
        .iter()
        .filter_map(|source| s11aggregates_check_fixture(binary, source).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// From tests/_12_term_lt.rs
// `any.lt` parity through the real binary. Each `fixtures/term_lt/*.dl7`
// compiles to a checked runtime program, which is transported into `dl8 eval`
// and evaluated; the closure must match the sibling `*.expected.json`, written
// by hand from the program text. The fixture path is replaced by `<fixture>`
// so the expected files carry no absolute path.
//
// `dl8 compile` prints the `program` object `dl8 eval` reads; the compile
// output file goes into `dl8 eval` unchanged, and no test rebuilds it.

/// The fixture path is machine-specific; the expected files spell it
/// `<fixture>`.
fn s12termlt_normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| s12termlt_normalize(v, path)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), s12termlt_normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn s12termlt_fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/term_lt");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "dl7"))
        .collect();
    out.sort();
    out
}

fn s12termlt_run(binary: &str, mode: &str, input: &Path) -> Result<Value, String> {
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

/// `dl8 compile`'s whole stdout is the program file; `dl8 eval` reads the
/// `program` key out of it.
fn s12termlt_compile_to(binary: &str, source: &Path, program: &Path) -> Result<(), String> {
    let output = Command::new(binary)
        .arg("compile")
        .arg(source)
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    if output.stdout.is_empty() {
        return Err(format!(
            "no stdout (exit {:?}); stderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    std::fs::write(program, &output.stdout).map_err(|e| format!("{e}"))
}

/// Compile one fixture, carry its runtime program through `dl8 eval`, and
/// compare the closure and diagnostics to the sibling expected file.
fn s12termlt_check_fixture(binary: &str, source: &Path) -> Result<(), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let program_path =
        std::env::temp_dir().join(format!("dl8-term-lt-{}-{stem}.json", std::process::id()));
    s12termlt_compile_to(binary, &canonical, &program_path)
        .map_err(|e| format!("{stem}: compile {e}"))?;
    let got = s12termlt_run(binary, "eval", &program_path).map_err(|e| format!("{stem}: eval {e}"));
    let _ = std::fs::remove_file(&program_path);
    let got = s12termlt_normalize(&got?, &fixture_path);
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
    let s12termlt_fixtures = s12termlt_fixtures();
    assert!(!s12termlt_fixtures.is_empty(), "no term_lt fixtures found");
    let failures: Vec<String> = s12termlt_fixtures
        .iter()
        .filter_map(|source| s12termlt_check_fixture(binary, source).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// From tests/_15_fold.rs
// Programmable-fold parity through the real binary. Each `fixtures/fold/*.dl7`
// is compiled by `dl8 compile`, whose comptime rounds evaluate the program in
// process; the rows the fixture's own relations carry, plus the diagnostics,
// must match the sibling `*.expected.json`.
//
// Compile supplies the fixture's own relation IDs and source locations used
// by the frozen expected rows. The eval transport also carries fold heads.

/// A relation reference the fixture file owns, rather than the prelude or the
/// kernel.
fn owned_by_fixture(relation: &Value, path: &str) -> bool {
    serde_json::to_string(relation)
        .map(|text| text.contains(path))
        .unwrap_or(false)
}

/// `call(Relation, Arguments)` rows of the fixture's own relations, in the
/// order the closure carries them.
fn fixture_rows(compiler_rows: &Value, path: &str) -> Result<Vec<Value>, String> {
    let rows = compiler_rows
        .as_array()
        .ok_or("compiler_rows is not a list")?;
    let mut out = Vec::new();
    for row in rows {
        if row.get("f").and_then(Value::as_str) != Some("call") {
            continue;
        }
        let args = row
            .get("args")
            .and_then(Value::as_array)
            .ok_or("call/2 expected")?;
        let relation = args.first().ok_or("call without relation")?;
        if !owned_by_fixture(relation, path) {
            continue;
        }
        let arguments = args.get(1).ok_or("call without arguments")?;
        out.push(json!({ "rel": relation, "args": arguments }));
    }
    Ok(out)
}

/// The fixture path is machine-specific; the expected files spell it
/// `<fixture>`.
fn s15fold_normalize(value: &Value, path: &str) -> Value {
    match value {
        Value::String(s) => Value::String(s.replace(path, "<fixture>")),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| s15fold_normalize(v, path)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, v)| (key.clone(), s15fold_normalize(v, path)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn s15fold_fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/fold");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "dl7"))
        .collect();
    out.sort();
    out
}

fn compile(binary: &str, input: &Path, engine: Option<&str>) -> Result<Value, String> {
    let mut command = Command::new(binary);
    command.arg("compile").arg(input);
    if let Some(engine) = engine {
        command.env("DL8_ENGINE", engine);
    }
    let output = command
        .output()
        .map_err(|e| format!("compile spawn: {e}"))?;
    serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "compile no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn observed(binary: &str, source: &Path, engine: Option<&str>) -> Result<(String, Value), String> {
    let stem = source.file_stem().unwrap().to_string_lossy().to_string();
    let canonical = std::fs::canonicalize(source).map_err(|e| format!("{stem}: {e}"))?;
    let fixture_path = canonical.to_string_lossy().to_string();
    let compiled = compile(binary, &canonical, engine).map_err(|e| format!("{stem}: {e}"))?;
    let rows = fixture_rows(&compiled["compiler_rows"], &fixture_path)
        .map_err(|e| format!("{stem}: {e}"))?;
    let got = json!({ "rows": rows, "diagnostics": compiled["diagnostics"] });
    Ok((stem, s15fold_normalize(&got, &fixture_path)))
}

fn s15fold_check_fixture(binary: &str, source: &Path, engine: Option<&str>) -> Result<(), String> {
    let (stem, got) = observed(binary, source, engine)?;
    let expected_path = source.with_file_name(format!("{stem}.expected.json"));
    let text = std::fs::read_to_string(&expected_path).map_err(|e| format!("{stem}: {e}"))?;
    let want: Value = serde_json::from_str(&text).map_err(|e| format!("{stem}: {e}"))?;
    for key in ["rows", "diagnostics"] {
        if got[key] != want[key] {
            return Err(format!(
                "{stem} engine={engine:?}: {key} differ\n  want: {}\n  got:  {}",
                want[key], got[key]
            ));
        }
    }
    Ok(())
}

#[test]
pub fn every_fold_fixture_matches_expected() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let s15fold_fixtures = s15fold_fixtures();
    assert!(!s15fold_fixtures.is_empty(), "no fold fixtures found");
    let failures: Vec<String> = [Some("ir:dd")]
        .into_iter()
        .flat_map(|engine| {
            s15fold_fixtures
                .iter()
                .filter_map(move |source| s15fold_check_fixture(binary, source, engine).err())
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The folded value each fixture is written to produce, and the diagnostic the
/// zero-row step reports. Pins the numbers the expected files encode in term
/// shape: 142 = 7+35+100, 284 = twice that, 12 = 1,2,4 folded in ascending
/// order by an accumulator that doubles first (21 is the descending answer).
#[test]
pub fn every_fold_fixture_carries_its_written_answer() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let wanted: [(&str, Option<i64>, &str); 4] = [
        ("0_kernel_step", Some(142), ""),
        ("1_program_step", Some(284), ""),
        ("2_order_matters", Some(12), ""),
        ("3_step_no_row", None, "fold_step_no_row"),
    ];
    let mut failures: Vec<String> = Vec::new();
    for engine in [Some("ir:dd")] {
        for source in s15fold_fixtures() {
            let (stem, got) = observed(binary, &source, engine).unwrap();
            let Some((_, value, diagnostic)) = wanted.iter().find(|(name, _, _)| *name == stem)
            else {
                failures.push(format!("{stem}: fixture has no written answer"));
                continue;
            };
            let text = got.to_string();
            match value {
                Some(n) => {
                    let folded = json!({ "f": "const", "args": [n] }).to_string();
                    if !text.contains(&folded) {
                        failures.push(format!("{stem}: {n} absent from the rows"));
                    }
                }
                None => {
                    if !text.contains(diagnostic) {
                        failures.push(format!("{stem}: {diagnostic} absent"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// From tests/_24_demand_count.rs
// The demand gate. `positive_solutions` proved every bound goal on a
// rule-heading relation top down, with the proof memo rebuilt per firing;
// one firing of the macrotime caret rule `ambiguous_return` over the
// prelude's 16151 syntax rows cost 48353 demand calls. Relations whose
// rules never need bound head arguments no longer demand, and the memo
// lives once per round. On `origin/main` the prelude does not flow through
// macrotime, so `tests/fixtures/demand_count/` reproduces the bleed at
// fixture scale and this test drives `evaluate` directly: 100 source rows,
// a derived relation whose rule needs nothing bound (the bleed), and one
// whose kernel goal reads its head argument (demand stays).

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/demand_count")
        .join(name)
        .display()
        .to_string()
}

fn relation(u: &mut Universe, name: &str) -> TermId {
    let inner = u.atom(name);
    u.compound("ref", vec![inner])
}

#[test]
fn demand_calls_stay_under_the_round_bar() {
    let text = std::fs::read_to_string(fixture("ambiguous_return_bleed.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut u = Universe::new();
    let program = program_from_json(&mut u, &value).unwrap();
    let caret = relation(&mut u, "caret_item");
    let flagged = relation(&mut u, "flagged");
    let mut demand_calls = 0usize;
    let closure = evaluate(&mut u, &program, &mut |t: Trace| {
        if let Trace::Demand { calls } = t {
            demand_calls += calls;
        }
    });
    assert!(closure.diagnostics.is_empty(), "unexpected diagnostics");
    let caret_rows = closure.rows.iter().filter(|r| r.rel == caret).count();
    let flagged_rows = closure.rows.iter().filter(|r| r.rel == flagged).count();
    assert_eq!(caret_rows, 100, "caret_item rows");
    assert_eq!(flagged_rows, 50, "flagged rows");
    println!("demand calls: {demand_calls}");
    assert!(
        demand_calls < 500,
        "demand calls {demand_calls} over the round bar"
    );
}
