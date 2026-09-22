//! `dl8 run` through the real binary, a real clock and a real local HTTP
//! listener. Each `fixtures/reconcile/*.dl7` compiles to a program file; the
//! fetch fixture spells its url `__URL__`, which the test replaces with the
//! listener's address before compiling. Every run is killed past 10 s.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RUN_CAP: Duration = Duration::from_secs(10);

/// One directory per test, so two tests never share a db file.
fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("dl8-reconcile-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

struct Compiled {
    program: PathBuf,
    names: Value,
}

fn compile(source: &Path, program: &Path) -> Compiled {
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(source)
        .output()
        .unwrap();
    let compiled: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "compile no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(compiled["diagnostics"], json!([]), "compile diagnostics");
    std::fs::write(program, &output.stdout).unwrap();
    Compiled {
        program: program.to_path_buf(),
        names: compiled["program"]["names"].clone(),
    }
}

fn compile_fetch(directory: &Path, url: &str) -> Compiled {
    let text = std::fs::read_to_string(fixture("reconcile/1_fetch.dl7")).unwrap();
    let source = directory.join("fetch.dl7");
    std::fs::write(&source, text.replace("__URL__", url)).unwrap();
    compile(&source, &directory.join("fetch.json"))
}

/// Runs `dl8 run`, killing it past `RUN_CAP`; returns stdout JSON and exit code.
fn run(program: &Path, args: &[&str]) -> (Value, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("run")
        .arg(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > RUN_CAP {
            let _ = child.kill();
            panic!(
                "dl8 run {} {args:?} exceeded {RUN_CAP:?}",
                program.display()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let value = serde_json::from_slice(&stdout)
        .unwrap_or_else(|e| panic!("run no JSON ({e}); stderr: {stderr}"));
    (value, status.code().unwrap_or(-1))
}

/// `const(X)` cells as plain JSON, one Vec per row of the named relation.
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

/// A listener that answers every request with one fixed response and counts
/// the requests. Its accept thread lives until the test process exits.
fn listen(status: u16, body: &'static str) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/body", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buffer[..n]),
                }
            }
            counter.fetch_add(1, Ordering::SeqCst);
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (url, hits)
}

/// A port nothing listens on: bound once, then released.
fn closed_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{address}/body")
}

#[test]
pub fn timer_fires_once_per_tick_and_the_count_reads_every_fire() {
    let directory = scratch("timer");
    let compiled = compile(
        &fixture("reconcile/0_timer.dl7"),
        &directory.join("timer.json"),
    );
    let (out, code) = run(&compiled.program, &["--serve", "timer", "--max-ticks", "3"]);
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(out["ticks"], json!(3));
    assert_eq!(
        rows(&out, &compiled.names, "timer"),
        vec![
            vec![json!(1), json!(1)],
            vec![json!(1), json!(2)],
            vec![json!(1), json!(3)]
        ]
    );
    let counts = rows(&out, &compiled.names, "FiredCount");
    assert_eq!(counts.last(), Some(&vec![json!(3)]), "counts {counts:?}");
}

#[test]
pub fn timer_numbering_continues_past_the_ticks_in_the_db() {
    let directory = scratch("timer-db");
    let compiled = compile(
        &fixture("reconcile/0_timer.dl7"),
        &directory.join("timer.json"),
    );
    let db = directory.join("run.db").to_string_lossy().to_string();
    let args = ["--serve", "timer", "--max-ticks", "2", "--db", db.as_str()];
    let (first, _) = run(&compiled.program, &args);
    assert_eq!(rows(&first, &compiled.names, "timer").len(), 2);
    let (second, code) = run(&compiled.program, &args);
    assert_eq!(code, 0, "exit code");
    let ticks: Vec<Value> = rows(&second, &compiled.names, "timer")
        .into_iter()
        .map(|row| row[1].clone())
        .collect();
    assert_eq!(ticks, vec![json!(1), json!(2), json!(3), json!(4)]);
}

#[test]
pub fn fetch_json_body_lands_as_a_row_the_reader_joins() {
    let directory = scratch("fetch-ok");
    let (url, hits) = listen(200, r#"{"login":"hafley66"}"#);
    let compiled = compile_fetch(&directory, &url);
    let (out, code) = run(&compiled.program, &["--serve", "fetch_json"]);
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    let body = json!(r#"{"login":"hafley66"}"#);
    assert_eq!(
        rows(&out, &compiled.names, "fetch_json"),
        vec![vec![json!(url), body.clone()]]
    );
    assert_eq!(
        rows(&out, &compiled.names, "Body"),
        vec![vec![json!(url), body]]
    );
    assert!(rows(&out, &compiled.names, "fetch_json_error").is_empty());
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "requests: one at comptime (fetch_json is Once), one at runtime"
    );
    assert_eq!(out["ticks"], json!(1));
}

#[test]
pub fn fetch_json_non_2xx_is_one_error_row_and_no_body_row() {
    let directory = scratch("fetch-500");
    let (url, hits) = listen(500, r#"{"message":"boom"}"#);
    let compiled = compile_fetch(&directory, &url);
    let (out, code) = run(&compiled.program, &["--serve", "fetch_json"]);
    assert_eq!(code, 0, "exit code");
    assert!(rows(&out, &compiled.names, "fetch_json").is_empty());
    let errors = rows(&out, &compiled.names, "fetch_json_error");
    assert_eq!(errors.len(), 1, "errors {errors:?}");
    assert_eq!(errors[0][..2], [json!(url), json!(500)]);
    assert_eq!(
        rows(&out, &compiled.names, "Failed"),
        vec![vec![json!(url), json!(500)]]
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "requests: one at comptime, one at runtime"
    );
}

#[test]
pub fn fetch_json_body_that_is_not_json_is_an_error_row() {
    let directory = scratch("fetch-text");
    let (url, _) = listen(200, "not json");
    let compiled = compile_fetch(&directory, &url);
    let (out, _) = run(&compiled.program, &["--serve", "fetch_json"]);
    assert!(rows(&out, &compiled.names, "fetch_json").is_empty());
    let errors = rows(&out, &compiled.names, "fetch_json_error");
    assert_eq!(errors.len(), 1, "errors {errors:?}");
    assert_eq!(errors[0][1], json!(200));
}

#[test]
pub fn fetch_json_closed_port_is_an_error_row_with_status_zero() {
    let directory = scratch("fetch-closed");
    let url = closed_url();
    let compiled = compile_fetch(&directory, &url);
    let (out, code) = run(&compiled.program, &["--serve", "fetch_json"]);
    assert_eq!(code, 0, "exit code");
    assert!(rows(&out, &compiled.names, "fetch_json").is_empty());
    let errors = rows(&out, &compiled.names, "fetch_json_error");
    assert_eq!(errors.len(), 1, "errors {errors:?}");
    assert_eq!(errors[0][..2], [json!(url), json!(0)]);
    assert!(!errors[0][2].as_str().unwrap().is_empty(), "message");
}

/// COUNT test: the store counts its own INSERT statements per process.
#[test]
pub fn a_second_run_against_the_db_answers_nothing_and_inserts_nothing() {
    let directory = scratch("fetch-db");
    let (url, hits) = listen(200, r#"[1,2,3]"#);
    let compiled = compile_fetch(&directory, &url);
    let db = directory.join("run.db").to_string_lossy().to_string();
    let args = ["--serve", "fetch_json", "--db", db.as_str()];
    let (first, code) = run(&compiled.program, &args);
    assert_eq!(code, 0, "first exit code");
    assert_eq!(first["ticks"], json!(1));
    assert!(first["insert_statements"].as_u64().unwrap() > 0);
    let (second, code) = run(&compiled.program, &args);
    assert_eq!(code, 0, "second exit code");
    assert_eq!(second["ticks"], json!(0), "second run ticks");
    assert_eq!(second["insert_statements"], json!(0), "second run inserts");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "requests across compile and both runs"
    );
    assert_eq!(
        rows(&second, &compiled.names, "Body"),
        vec![vec![json!(url), json!("[1,2,3]")]]
    );
}

#[test]
pub fn serving_a_name_with_no_executor_is_a_diagnostic() {
    let directory = scratch("no-executor");
    let compiled = compile(
        &fixture("reconcile/0_timer.dl7"),
        &directory.join("timer.json"),
    );
    let (out, code) = run(&compiled.program, &["--serve", "Fired"]);
    assert_eq!(code, 1, "exit code");
    assert_eq!(out["closure"], json!([]));
    assert_eq!(
        out["diagnostics"],
        json!([{
            "phase": "eval",
            "payload": {"f": "served_relation_no_executor", "args": [{"a": "Fired"}]}
        }])
    );
}

#[test]
pub fn fetch_json_without_a_declared_error_relation_is_a_diagnostic() {
    let directory = scratch("no-error-rel");
    let compiled = compile(
        &fixture("host_effect/0_pending.dl7"),
        &directory.join("pending.json"),
    );
    let (out, code) = run(&compiled.program, &["--serve", "fetch_json"]);
    assert_eq!(code, 1, "exit code");
    assert_eq!(
        out["diagnostics"],
        json!([{
            "phase": "eval",
            "payload": {"f": "executor_relation_unknown", "args": [{"a": "fetch_json_error"}]}
        }])
    );
}
