//! `dl8 run` loading the `dylib_echo` plugin (`labs/dylib_echo`) through the
//! real binary. `Cadence::Once` never re-answers one application, so the
//! reload proof chains a second `dylib_echo` application off the first
//! answer's output (`First`/`Second` in the fixture) instead of repeating the
//! same input: run 1 answers `("a","v1:a")` with the v1 plugin and leaves the
//! chained application for `"v1:a"` unanswered (`--max-ticks 1`); the plugin
//! is rebuilt as v2 over the same path; run 2, same `--db`, answers
//! `("v1:a","v2:v1:a")`, proving reload without losing run 1's row. Every run
//! is killed past 10 s.

use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RUN_CAP: Duration = Duration::from_secs(10);
const BUILD_BUDGET: Duration = Duration::from_secs(60);

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("dl8-dylib-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/hosts")
        .join(name)
}

fn plugin_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("labs/dylib_echo/Cargo.toml")
}

struct Compiled {
    program: PathBuf,
    names: Value,
}

fn compile(directory: &Path) -> Compiled {
    let source = fixture("4_dylib.dl7");
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(&source)
        .output()
        .unwrap();
    let compiled: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "compile no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(compiled["diagnostics"], json!([]), "compile diagnostics");
    let program = directory.join("4_dylib.json");
    std::fs::write(&program, &output.stdout).unwrap();
    Compiled {
        program,
        names: compiled["program"]["names"].clone(),
    }
}

/// Builds the `dylib_echo` plugin release artifact into `target_dir`, with
/// `V` baking the answer prefix; the output path is stable across builds so a
/// second build with a different `V` moves the same file's mtime.
fn build_plugin(target_dir: &Path, v: &str) -> (PathBuf, Duration) {
    let started = Instant::now();
    let output = Command::new("cargo")
        .args(["build", "--release", "--offline", "--manifest-path"])
        .arg(plugin_manifest())
        .arg("--target-dir")
        .arg(target_dir)
        .env("V", v)
        .output()
        .expect("cargo build dylib_echo");
    let elapsed = started.elapsed();
    assert!(
        output.status.success(),
        "building dylib_echo {v} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < BUILD_BUDGET,
        "building dylib_echo {v} took {} ms",
        elapsed.as_millis()
    );
    let name = if cfg!(target_os = "macos") {
        "libdylib_echo.dylib"
    } else if cfg!(target_os = "windows") {
        "dylib_echo.dll"
    } else {
        "libdylib_echo.so"
    };
    (target_dir.join("release").join(name), elapsed)
}

struct Ran {
    out: Value,
    code: i32,
    wall: Duration,
}

fn run(program: &Path, args: &[&str], dylib_path: &Path) -> Ran {
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("run")
        .arg(program)
        .args(args)
        .env("DL8_DYLIB_PATH", dylib_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > RUN_CAP {
            let _ = child.kill();
            panic!("dl8 run {} {args:?} exceeded {RUN_CAP:?}", program.display());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let wall = started.elapsed();
    let mut stdout = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut stdout).unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let out = serde_json::from_slice(&stdout)
        .unwrap_or_else(|e| panic!("run no JSON ({e}); stderr: {stderr}"));
    Ran {
        out,
        code: status.code().unwrap_or(-1),
        wall,
    }
}

/// `const(X)` cells as plain JSON, one sorted Vec per row of the named relation.
fn rows(out: &Value, names: &Value, name: &str) -> Vec<Vec<Value>> {
    let relation = &names[name];
    assert!(!relation.is_null(), "{name} missing from names");
    let mut found: Vec<Vec<Value>> = out["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation)
        .map(|row| {
            row["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|cell| match cell["args"][0].get("s") {
                    Some(text) => text.clone(),
                    None => cell["args"][0].clone(),
                })
                .collect()
        })
        .collect();
    found.sort_by_key(|row| row.iter().map(|v| v.to_string()).collect::<Vec<_>>());
    found
}

fn text(value: &str) -> Value {
    json!(value)
}

#[test]
pub fn reload_answers_the_chained_application_without_losing_the_first_row() {
    let directory = scratch("reload");
    let compiled = compile(&directory);
    let plugins = directory.join("plugin-target");
    let db = directory.join("store.sqlite");

    let (v1, build_v1) = build_plugin(&plugins, "v1");
    let run1 = run(
        &compiled.program,
        &["--serve", "dylib_echo", "--db", db.to_str().unwrap(), "--max-ticks", "1"],
        &v1,
    );
    assert_eq!(run1.code, 0, "run1 exit; diagnostics {}", run1.out["diagnostics"]);
    assert_eq!(
        rows(&run1.out, &compiled.names, "dylib_echo"),
        vec![vec![text("a"), text("v1:a")]],
        "run1 answers only the seeded application"
    );
    assert_eq!(
        rows(&run1.out, &compiled.names, "Second"),
        Vec::<Vec<Value>>::new(),
        "run1 has not answered the chained application yet"
    );

    // mtime granularity: past it, so the rebuild below is a strictly later write.
    std::thread::sleep(Duration::from_millis(1100));
    let (v2, build_v2) = build_plugin(&plugins, "v2");
    assert_eq!(v1, v2, "same output path, so the reload sees one file");

    let run2 = run(
        &compiled.program,
        &["--serve", "dylib_echo", "--db", db.to_str().unwrap()],
        &v2,
    );
    assert_eq!(run2.code, 0, "run2 exit; diagnostics {}", run2.out["diagnostics"]);
    assert_eq!(
        rows(&run2.out, &compiled.names, "dylib_echo"),
        vec![
            vec![text("a"), text("v1:a")],
            vec![text("v1:a"), text("v2:v1:a")],
        ],
        "run1's row survives; the reloaded plugin answers the chained one"
    );
    assert_eq!(
        rows(&run2.out, &compiled.names, "Second"),
        vec![vec![text("v1:a"), text("v2:v1:a")]]
    );

    println!(
        "receipts: build v1 {} ms, run1 {} ms, build v2 {} ms, run2 {} ms",
        build_v1.as_millis(),
        run1.wall.as_millis(),
        build_v2.as_millis(),
        run2.wall.as_millis()
    );
}

#[test]
pub fn a_path_that_is_not_a_dylib_is_one_diagnostic_naming_it_and_a_nonzero_exit() {
    let directory = scratch("not-a-dylib");
    let compiled = compile(&directory);
    let bogus = directory.join("bogus.txt");
    std::fs::write(&bogus, b"not a dylib").unwrap();
    let db = directory.join("store.sqlite");

    let ran = run(
        &compiled.program,
        &["--serve", "dylib_echo", "--db", db.to_str().unwrap()],
        &bogus,
    );
    assert_ne!(ran.code, 0, "exit code");
    assert_eq!(ran.out["closure"], json!([]), "no rows on a load failure");
    let diagnostics = ran.out["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "diagnostics {diagnostics:?}");
    let payload = serde_json::to_string(&diagnostics[0]).unwrap();
    assert!(payload.contains("dylib_load_failed"), "{payload}");
    assert!(
        payload.contains(bogus.to_str().unwrap()),
        "diagnostic names the path: {payload}"
    );
}
