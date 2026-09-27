use serde_json::json;
use serde_json::Value;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

// From tests/_14_host_effect.rs
// `effect` rows for served relations, through the real binary. Each
// `fixtures/host_effect/*.dl7` compiles to a checked runtime program, whose
// `program` object goes straight into `dl8 eval --serve <name>`; the closure
// must match the sibling `*.expected.json`. The fixture path is replaced by
// `<fixture>` so the expected files carry no absolute path.
//
// The served set is a runtime input, so a program run without `--serve` writes
// no `effect` row at all; `4_unserved` pins that, and with it every oracle case.
//
// `dl8 compile` prints `program.names`, the module-level `:/4` binds of
// `root_graph`, which is what `--serve` resolves against. No test rebuilds it.

/// Which relations each fixture asks the outside to settle.
pub const SERVED: [(&str, &str); 5] = [
    ("0_pending", "fetch_json"),
    ("1_settled", "fetch_json"),
    ("2_source", "tick"),
    ("3_loading", "fetch_json"),
    ("4_unserved", ""),
];

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

/// Compile one fixture and keep its whole stdout; `dl8 eval` reads the
/// `program` key out of it.
fn transport(binary: &str, source: &Path, stem: &str) -> Result<PathBuf, String> {
    let output = Command::new(binary)
        .args(["compile", &source.to_string_lossy()])
        .output()
        .map_err(|e| format!("{stem}: compile spawn {e}"))?;
    serde_json::from_slice::<Value>(&output.stdout).map_err(|e| {
        format!(
            "{stem}: compile no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let path = std::env::temp_dir().join(format!("dl8-effect-{}-{stem}.json", std::process::id()));
    std::fs::write(&path, &output.stdout).unwrap();
    Ok(path)
}

/// Compile one fixture, evaluate it with its served set, and compare the
/// closure and diagnostics to the sibling expected file.
fn check_fixture(
    binary: &str,
    stem: &str,
    served: &str,
    engine: Option<&str>,
) -> Result<(), String> {
    let source = fixture_path(stem);
    let canonical = std::fs::canonicalize(&source).map_err(|e| format!("{stem}: {e}"))?;
    let program_path = transport(binary, &canonical, stem)?;
    let program = program_path.to_string_lossy().to_string();
    let mut args = vec!["eval", program.as_str()];
    if !served.is_empty() {
        args.push("--serve");
        args.push(served);
    }
    let output = {
        let mut command = Command::new(binary);
        command.args(&args);
        if let Some(engine) = engine {
            command.env("DL8_ENGINE", engine);
        }
        command.output().map_err(|e| format!("spawn: {e}"))?
    };
    let value = serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "no JSON on stdout ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let got = (value, output.status.code().unwrap_or(-1));
    let _ = std::fs::remove_file(&program_path);
    let got = normalize(&got.0, &canonical.to_string_lossy());
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
    let mut failures = Vec::new();
    // `ir:sqlite` is omitted while ivm-sqlite's `install_program` does not
    // finish this program within the 20-second binary-run budget.
    for engine in [None, Some("ir:dd")] {
        for (stem, served) in SERVED {
            if let Err(error) = check_fixture(binary, stem, served, engine) {
                failures.push(format!("{}: {error}", engine.unwrap_or("default")));
            }
        }
    }
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

#[test]
pub fn every_declared_relation_reaches_the_names_table() {
    let binary = env!("CARGO_BIN_EXE_dl8");
    let canonical = std::fs::canonicalize(fixture_path("1_settled")).unwrap();
    let (compiled, code) = run(binary, &["compile", &canonical.to_string_lossy()]).unwrap();
    assert_eq!(code, 0, "compile exit code");
    let names = compiled["program"]["names"].as_object().unwrap();
    for declared in ["fetch_json", "Watch", "Body"] {
        let relation = names
            .get(declared)
            .unwrap_or_else(|| panic!("{declared} missing from names"));
        assert_eq!(relation["f"], "ref", "{declared} is not a ref");
    }
}

// From tests/_17_reconcile.rs
// `dl8 run` through the real binary, a real clock and a real local HTTP
// listener. Each `fixtures/reconcile/*.dl7` compiles to a program file; the
// fetch fixture spells its url `__URL__`, which the test replaces with the
// listener's address before compiling. Every run is killed past 10 s.

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
fn s17reconcile_run(program: &Path, args: &[&str]) -> (Value, i32) {
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
    let (out, code) =
        s17reconcile_run(&compiled.program, &["--serve", "timer", "--max-ticks", "3"]);
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
    let (first, _) = s17reconcile_run(&compiled.program, &args);
    assert_eq!(rows(&first, &compiled.names, "timer").len(), 2);
    let (second, code) = s17reconcile_run(&compiled.program, &args);
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
    let (out, code) = s17reconcile_run(&compiled.program, &["--serve", "fetch_json"]);
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
    let (out, code) = s17reconcile_run(&compiled.program, &["--serve", "fetch_json"]);
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
    let (out, _) = s17reconcile_run(&compiled.program, &["--serve", "fetch_json"]);
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
    let (out, code) = s17reconcile_run(&compiled.program, &["--serve", "fetch_json"]);
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
    let (first, code) = s17reconcile_run(&compiled.program, &args);
    assert_eq!(code, 0, "first exit code");
    assert_eq!(first["ticks"], json!(1));
    assert!(first["insert_statements"].as_u64().unwrap() > 0);
    let (second, code) = s17reconcile_run(&compiled.program, &args);
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
    let (out, code) = s17reconcile_run(&compiled.program, &["--serve", "Fired"]);
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
    let (out, code) = s17reconcile_run(&compiled.program, &["--serve", "fetch_json"]);
    assert_eq!(code, 1, "exit code");
    assert_eq!(
        out["diagnostics"],
        json!([{
            "phase": "eval",
            "payload": {"f": "executor_relation_unknown", "args": [{"a": "fetch_json_error"}]}
        }])
    );
}

// From tests/_20_hosts.rs
// `dl8 run` through the real binary with the git and extract executors. Each
// test makes a throwaway repository with the `git` CLI, the test being the
// boundary; the executors under test reach git only through soopy. The
// `fixtures/hosts/*.dl7` programs spell their roots `__ROOT__`. Every run is
// killed past 10 s.

const s20hosts_RUN_CAP: Duration = Duration::from_secs(10);

const FIRST_DATE: &str = "1700000000 +0000";
const SECOND_DATE: &str = "1700000100 +0000";

fn s20hosts_scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("dl8-hosts-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::canonicalize(&directory).unwrap()
}

fn s20hosts_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/hosts")
        .join(name)
}

fn git(root: &Path, date: &str, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "dl8")
        .env("GIT_AUTHOR_EMAIL", "dl8@example.invalid")
        .env("GIT_COMMITTER_NAME", "dl8")
        .env("GIT_COMMITTER_EMAIL", "dl8@example.invalid")
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn commit(root: &Path, date: &str, path: &str, text: &str) -> String {
    std::fs::write(root.join(path), text).unwrap();
    git(root, date, &["add", path]);
    git(root, date, &["commit", "-q", "-m", path]);
    git(root, date, &["rev-parse", "HEAD"])
}

/// A repository on `main` with one commit adding `a.txt`.
fn repository(directory: &Path) -> (PathBuf, String) {
    let root = directory.join("repo");
    std::fs::create_dir_all(&root).unwrap();
    git(&root, FIRST_DATE, &["init", "-q", "-b", "main"]);
    let first = commit(&root, FIRST_DATE, "a.txt", "one\n");
    (root, first)
}

struct s20hosts_Compiled {
    program: PathBuf,
    names: Value,
}

fn s20hosts_compile(
    directory: &Path,
    name: &str,
    replacements: &[(&str, &str)],
) -> s20hosts_Compiled {
    let mut text = std::fs::read_to_string(s20hosts_fixture(name)).unwrap();
    for (from, to) in replacements {
        text = text.replace(from, to);
    }
    let source = directory.join(name);
    std::fs::write(&source, text).unwrap();
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
    let program = directory.join(format!("{name}.json"));
    std::fs::write(&program, &output.stdout).unwrap();
    s20hosts_Compiled {
        program,
        names: compiled["program"]["names"].clone(),
    }
}

/// A live `dl8 run`: stdout and stderr drained on their own threads, stderr
/// forwarded line by line so a test can wait on a log line.
struct Running {
    child: Child,
    started: Instant,
    stdout: std::thread::JoinHandle<Vec<u8>>,
    stderr_lines: Receiver<String>,
    stderr: Vec<String>,
}

fn spawn(program: &Path, args: &[&str], env: &[(&str, &str)]) -> Running {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dl8"));
    command
        .arg("run")
        .arg(program)
        .args(args)
        .env("RUST_LOG", "dl8=info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let (send, stderr_lines) = channel();
    let stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if send.send(line).is_err() {
                return;
            }
        }
    });
    Running {
        child,
        started: Instant::now(),
        stdout,
        stderr_lines,
        stderr: Vec::new(),
    }
}

impl Running {
    fn wait_for_line(&mut self, needle: &str) {
        loop {
            let left = s20hosts_RUN_CAP.saturating_sub(self.started.elapsed());
            match self.stderr_lines.recv_timeout(left) {
                Ok(line) => {
                    let found = line.contains(needle);
                    self.stderr.push(line);
                    if found {
                        return;
                    }
                }
                Err(_) => {
                    let _ = self.child.kill();
                    panic!(
                        "no stderr line with {needle:?} within {s20hosts_RUN_CAP:?}: {:#?}",
                        self.stderr
                    );
                }
            }
        }
    }

    fn finish(mut self) -> (Value, i32) {
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if self.started.elapsed() > s20hosts_RUN_CAP {
                let _ = self.child.kill();
                self.stderr.extend(self.stderr_lines.try_iter());
                panic!("dl8 run exceeded {s20hosts_RUN_CAP:?}: {:#?}", self.stderr);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let stdout = self.stdout.join().unwrap();
        let value = serde_json::from_slice(&stdout).unwrap_or_else(|e| {
            self.stderr.extend(self.stderr_lines.try_iter());
            panic!("run no JSON ({e}); stderr: {:#?}", self.stderr)
        });
        (value, status.code().unwrap_or(-1))
    }
}

fn s20hosts_run(program: &Path, args: &[&str], env: &[(&str, &str)]) -> (Value, i32) {
    spawn(program, args, env).finish()
}

/// `const(X)` cells as plain JSON, one sorted Vec per row of the named relation.
fn s20hosts_rows(out: &Value, names: &Value, name: &str) -> Vec<Vec<Value>> {
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
pub fn git_refs_answers_the_snapshot_then_the_ref_that_moved() {
    let directory = s20hosts_scratch("refs");
    let (root, first) = repository(&directory);
    let root_text = root.to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "0_refs.dl7", &[("__ROOT__", &root_text)]);
    let mut running = spawn(
        &compiled.program,
        &["--serve", "git.refs", "--max-ticks", "2"],
        &[],
    );
    running.wait_for_line("armed");
    let second = commit(&root, SECOND_DATE, "b.txt", "two\n");
    let (out, code) = running.finish();
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(out["ticks"], json!(2));
    let mut expected = vec![
        vec![text(&root_text), text("HEAD"), text(&first)],
        vec![text(&root_text), text("HEAD"), text(&second)],
        vec![text(&root_text), text("refs/heads/main"), text(&first)],
        vec![text(&root_text), text("refs/heads/main"), text(&second)],
    ];
    expected.sort_by_key(|row| row.iter().map(|v| v.to_string()).collect::<Vec<_>>());
    assert_eq!(s20hosts_rows(&out, &compiled.names, "git.refs"), expected);
    let mut heads = vec![vec![text(&first)], vec![text(&second)]];
    heads.sort_by_key(|row| row[0].to_string());
    assert_eq!(s20hosts_rows(&out, &compiled.names, "Head"), heads);
    assert!(s20hosts_rows(&out, &compiled.names, "git.refs_error").is_empty());
}

#[test]
pub fn git_refs_on_a_directory_that_is_no_repository_is_an_error_row() {
    let directory = s20hosts_scratch("refs-missing");
    let root_text = directory.join("nothing").to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "0_refs.dl7", &[("__ROOT__", &root_text)]);
    let (out, code) = s20hosts_run(
        &compiled.program,
        &["--serve", "git.refs", "--max-ticks", "1"],
        &[],
    );
    assert_eq!(code, 0, "exit code");
    assert!(s20hosts_rows(&out, &compiled.names, "git.refs").is_empty());
    let errors = s20hosts_rows(&out, &compiled.names, "git.refs_error");
    assert_eq!(errors.len(), 1, "errors {errors:?}");
    assert_eq!(errors[0][0], text(&root_text));
}

#[test]
pub fn git_history_answers_one_edge_per_parent() {
    let directory = s20hosts_scratch("history");
    let (root, first) = repository(&directory);
    let second = commit(&root, SECOND_DATE, "b.txt", "two\n");
    let root_text = root.to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "1_history.dl7", &[("__ROOT__", &root_text)]);
    let (out, code) = s20hosts_run(&compiled.program, &["--serve", "git.history"], &[]);
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(
        s20hosts_rows(&out, &compiled.names, "Edge"),
        vec![vec![text(&second), text(&first)]]
    );
    assert!(s20hosts_rows(&out, &compiled.names, "Failed").is_empty());
    assert_eq!(out["ticks"], json!(1));
}

#[test]
pub fn git_history_on_a_directory_that_is_no_repository_is_an_error_row() {
    let directory = s20hosts_scratch("history-missing");
    let root_text = directory.join("nothing").to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "1_history.dl7", &[("__ROOT__", &root_text)]);
    let (out, code) = s20hosts_run(&compiled.program, &["--serve", "git.history"], &[]);
    assert_eq!(code, 0, "exit code");
    assert!(s20hosts_rows(&out, &compiled.names, "Edge").is_empty());
    assert_eq!(s20hosts_rows(&out, &compiled.names, "Failed").len(), 1);
}

#[test]
pub fn fs_at_answers_the_files_and_blobs_of_each_revision() {
    let directory = s20hosts_scratch("repo-at");
    let (root, first) = repository(&directory);
    let second = commit(&root, SECOND_DATE, "b.txt", "two\n");
    let root_text = root.to_string_lossy().to_string();
    let blob = |revision: &str, path: &str| {
        text(&git(
            &root,
            FIRST_DATE,
            &["rev-parse", &format!("{revision}:{path}")],
        ))
    };
    for (revision, expected) in [
        (
            first.clone(),
            vec![vec![text("a.txt"), blob(&first, "a.txt")]],
        ),
        (
            second.clone(),
            vec![
                vec![text("a.txt"), blob(&second, "a.txt")],
                vec![text("b.txt"), blob(&second, "b.txt")],
            ],
        ),
    ] {
        let compiled = s20hosts_compile(
            &directory,
            "2_fs_at.dl7",
            &[("__ROOT__", &root_text), ("__SHA__", &revision)],
        );
        let (out, code) = s20hosts_run(&compiled.program, &["--serve", "fs.at"], &[]);
        assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
        assert_eq!(
            s20hosts_rows(&out, &compiled.names, "File"),
            expected,
            "at {revision}"
        );
        assert!(s20hosts_rows(&out, &compiled.names, "fs.at_error").is_empty());
    }
}

/// A `const(Atom)` cell as `rows` projects it.
fn atom(name: &str) -> Value {
    json!({ "a": name })
}

/// `ref(application(primitive(Name), [Value]))`: the `intern` of one primitive.
fn value_node(primitive: &str, value: Value) -> Value {
    json!({
        "f": "application",
        "args": [{ "f": "primitive", "args": [{ "a": primitive }] }, [value]]
    })
}

/// A list cell keeps each element's own `ref` tag.
fn tagged(node: Value) -> Value {
    json!({ "f": "ref", "args": [node] })
}

/// The untagged identity the program's `names` table carries for a name.
fn identity(names: &Value, name: &str) -> Value {
    names[name]["args"][0].clone()
}

/// `application(<fs.json>, [Path])`: the root node of one document.
fn document(names: &Value, path: &str) -> Value {
    json!({ "f": "application", "args": [identity(names, "fs.json"), [{ "s": path }]] })
}

/// `edge(Owner, Label)`: a member node, `edge_ref`'s own shape.
fn member(owner: Value, label: Value) -> Value {
    json!({ "f": "edge", "args": [owner, label] })
}

fn sorted(mut s20hosts_rows: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    s20hosts_rows.sort_by_key(|row| row.iter().map(|v| v.to_string()).collect::<Vec<_>>());
    s20hosts_rows
}

/// `4_fs_json.dl7` over one document written into the scratch directory.
fn json_run(directory: &Path, name: &str, body: &str) -> (Value, Value, String) {
    let path = directory.join(name);
    std::fs::write(&path, body).unwrap();
    let path = path.to_string_lossy().to_string();
    let compiled = s20hosts_compile(directory, "4_fs_json.dl7", &[("__DOC__", &path)]);
    let (out, code) = s20hosts_run(&compiled.program, &["--serve", "fs.json"], &[]);
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    (out, compiled.names, path)
}

#[test]
pub fn fs_json_reads_an_object_into_edges_under_one_root() {
    let directory = s20hosts_scratch("fs-json");
    let document_path = std::fs::canonicalize(s20hosts_fixture("todo.json")).unwrap();
    let path = document_path.to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "4_fs_json.dl7", &[("__DOC__", &path)]);
    let (out, code) = s20hosts_run(&compiled.program, &["--serve", "fs.json"], &[]);
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(out["ticks"], json!(1));
    let names = &compiled.names;
    let root = document(names, &path);
    let meta = member(root.clone(), atom("meta"));
    assert_eq!(
        s20hosts_rows(&out, names, "Root"),
        vec![vec![text(&path), root.clone()]]
    );
    assert_eq!(
        s20hosts_rows(&out, names, "Member"),
        sorted(vec![
            vec![
                atom("title"),
                value_node("str", json!({ "s": "x" })),
                json!(0)
            ],
            vec![atom("n"), value_node("int", json!(3)), json!(1)],
            vec![
                atom("tags"),
                json!([
                    tagged(value_node("str", json!({ "s": "a" }))),
                    tagged(value_node("str", json!({ "s": "b" })))
                ]),
                json!(2)
            ],
            vec![atom("owner"), identity(names, "none"), json!(3)],
            vec![atom("meta"), meta.clone(), json!(4)],
        ])
    );
    let edges = s20hosts_rows(&out, names, "Edge");
    assert_eq!(edges.len(), 6, "one edge per member, nested included");
    assert!(
        edges.contains(&vec![
            meta,
            atom("v"),
            value_node("int", json!(1)),
            json!(0)
        ]),
        "the nested object owns its own edge: {edges:?}"
    );
    assert!(s20hosts_rows(&out, names, "Failed").is_empty());
}

#[test]
pub fn ir_fs_json_answers_once_per_request() {
    let directory = s20hosts_scratch("ir-fs-json-once");
    let document_path = std::fs::canonicalize(s20hosts_fixture("todo.json")).unwrap();
    let path = document_path.to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "4_fs_json.dl7", &[("__DOC__", &path)]);

    // `ir:sqlite` is omitted while ivm-sqlite's `install_program` does not
    // finish this program within the 20-second binary-run budget.
    for engine in ["ir:dd"] {
        let (out, code) = s20hosts_run(
            &compiled.program,
            &["--serve", "fs.json"],
            &[("DL8_ENGINE", engine)],
        );
        assert_eq!(code, 0, "{engine}: diagnostics {}", out["diagnostics"]);
        assert_eq!(out["ticks"], json!(1), "{engine}: answer frontier count");
        assert_eq!(
            s20hosts_rows(&out, &compiled.names, "Root").len(),
            1,
            "{engine}: answer rows"
        );
        assert_eq!(
            s20hosts_rows(&out, &compiled.names, "Member").len(),
            5,
            "{engine}: document member rows"
        );
    }
}

#[test]
pub fn fs_json_reads_a_top_level_array_and_a_top_level_scalar() {
    let directory = s20hosts_scratch("fs-json-roots");
    let (out, names, path) = json_run(&directory, "list.json", "[1, \"a\"]");
    assert_eq!(
        s20hosts_rows(&out, &names, "Root"),
        vec![vec![
            text(&path),
            json!([
                tagged(value_node("int", json!(1))),
                tagged(value_node("str", json!({ "s": "a" })))
            ])
        ]]
    );
    assert!(
        s20hosts_rows(&out, &names, "Edge").is_empty(),
        "a list owns no edge"
    );

    let (out, names, path) = json_run(&directory, "scalar.json", "3");
    assert_eq!(
        s20hosts_rows(&out, &names, "Root"),
        vec![vec![text(&path), value_node("int", json!(3))]]
    );
    assert!(
        s20hosts_rows(&out, &names, "Edge").is_empty(),
        "a scalar owns no edge"
    );
}

/// RFC 7396 sections 1 and 2, the case `plans/v8/probes/2026-09-17-json-null-vs-absent.dl7`
/// spells: `{"owner": null}` and `{}` are two graphs.
#[test]
pub fn fs_json_tells_present_null_from_an_absent_key() {
    let directory = s20hosts_scratch("fs-json-null");
    let (out, names, _) = json_run(
        &directory,
        "present.json",
        "{\"title\": \"x\", \"owner\": null}",
    );
    let title = vec![
        atom("title"),
        value_node("str", json!({ "s": "x" })),
        json!(0),
    ];
    assert_eq!(
        s20hosts_rows(&out, &names, "Member"),
        sorted(vec![
            title.clone(),
            vec![atom("owner"), identity(&names, "none"), json!(1)],
        ])
    );

    let (out, names, _) = json_run(&directory, "absent.json", "{\"title\": \"x\"}");
    assert_eq!(s20hosts_rows(&out, &names, "Member"), vec![title]);
}

#[test]
pub fn fs_json_on_a_file_that_is_no_document_is_an_error_row() {
    let directory = s20hosts_scratch("fs-json-missing");
    let missing = directory.join("nothing.json").to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "4_fs_json.dl7", &[("__DOC__", &missing)]);
    let (out, code) = s20hosts_run(&compiled.program, &["--serve", "fs.json"], &[]);
    assert_eq!(code, 0, "exit code");
    assert!(s20hosts_rows(&out, &compiled.names, "Root").is_empty());
    let failed = s20hosts_rows(&out, &compiled.names, "Failed");
    assert_eq!(failed.len(), 1, "errors {failed:?}");
    assert_eq!(failed[0][0], text(&missing));

    let (out, names, _) = json_run(&directory, "bad.json", "{not json");
    assert!(s20hosts_rows(&out, &names, "Root").is_empty());
    assert_eq!(s20hosts_rows(&out, &names, "Failed").len(), 1);
}

fn extract_roots() -> [PathBuf; 2] {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    let parent = root.parent().unwrap().to_path_buf();
    [root, parent]
}

fn built_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(shared) = std::env::var_os("CARGO_TARGET_DIR") {
        out.push(PathBuf::from(shared).join("debug/ryi"));
    }
    for root in extract_roots() {
        out.push(root.join("hafley-rs/target/debug/ryi"));
        out.push(root.join("hafley-rs/crates/sprefa-extract/target/debug/ryi"));
    }
    out
}

fn s20hosts_which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

/// `$SPREFA_EXTRACT_BIN`, `ryi` on PATH, then the `tests/_16_extract_tsi.rs`
/// search. The external crate is never built by this suite.
fn extract_bin() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("SPREFA_EXTRACT_BIN") {
        let path = PathBuf::from(named);
        return path.is_file().then_some(path);
    }
    s20hosts_which("ryi").or_else(|| built_candidates().into_iter().find(|path| path.is_file()))
}

#[test]
pub fn extract_answers_rows_for_each_family_over_the_corpus() {
    let Some(binary) = extract_bin() else {
        println!(
            "SKIP: ryi executable is absent; hafley-rs now declares the external extractor binary as ryii"
        );
        return;
    };
    let directory = s20hosts_scratch("extract");
    let corpus = std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/extract/corpus"),
    )
    .unwrap();
    let corpus_text = corpus.to_string_lossy().to_string();
    let binary = binary.to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "3_extract.dl7", &[("__ROOT__", &corpus_text)]);
    let (out, code) = s20hosts_run(
        &compiled.program,
        &["--serve", "extract"],
        &[("SPREFA_EXTRACT_BIN", &binary)],
    );
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(
        s20hosts_rows(&out, &compiled.names, "Failed"),
        Vec::<Vec<Value>>::new()
    );
    let facts = s20hosts_rows(&out, &compiled.names, "Fact");
    let count = |family: &str| facts.iter().filter(|row| row[0] == text(family)).count();
    let counts = [count("type"), count("call"), count("diet_scip")];
    println!("extract rows type/call/diet_scip {counts:?}");
    assert!(counts.iter().all(|n| *n > 0), "rows per family {counts:?}");
    let kinds: Vec<&Value> = facts
        .iter()
        .filter(|row| row[0] == text("call"))
        .map(|row| &row[1])
        .collect();
    assert!(
        kinds.contains(&&text("resolved_edge")),
        "call kinds {kinds:?}"
    );
    for row in &facts {
        let payload: Value = serde_json::from_str(row[2].as_str().unwrap()).unwrap();
        assert_eq!(payload["record"], row[1], "payload carries its kind");
    }
}

#[test]
pub fn extract_with_no_binary_is_one_error_row_per_family() {
    let directory = s20hosts_scratch("extract-missing");
    let missing = directory.join("no-extract").to_string_lossy().to_string();
    let compiled = s20hosts_compile(
        &directory,
        "3_extract.dl7",
        &[("__ROOT__", &directory.to_string_lossy())],
    );
    let (out, code) = s20hosts_run(
        &compiled.program,
        &["--serve", "extract"],
        &[("SPREFA_EXTRACT_BIN", &missing)],
    );
    assert_eq!(code, 0, "exit code");
    assert!(s20hosts_rows(&out, &compiled.names, "Fact").is_empty());
    let failed = s20hosts_rows(&out, &compiled.names, "Failed");
    let families: Vec<&Value> = failed.iter().map(|row| &row[0]).collect();
    assert_eq!(families, [&text("call"), &text("diet_scip"), &text("type")]);
}

#[test]
pub fn org_program_reads_the_head_of_each_required_repository() {
    let projects = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("projects"))
        .unwrap_or_default();
    let required = ["instant", "sprefa", "hafley-rs", "hafley-rxjs"];
    let absent: Vec<&str> = required
        .iter()
        .copied()
        .filter(|name| !projects.join(name).join(".git").exists())
        .collect();
    if !absent.is_empty() {
        println!(
            "ignored: no local clone under {} for {absent:?}",
            projects.display()
        );
        return;
    }
    let directory = s20hosts_scratch("org");
    let projects_text = projects.to_string_lossy().to_string();
    let compiled = s20hosts_compile(&directory, "org.dl7", &[("__PROJECTS__", &projects_text)]);
    let (out, code) = s20hosts_run(
        &compiled.program,
        &["--serve", "git.refs", "--max-ticks", "1"],
        &[],
    );
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert!(s20hosts_rows(&out, &compiled.names, "git.refs_error").is_empty());
    let heads = s20hosts_rows(&out, &compiled.names, "head_sha");
    let roots: Vec<Value> = heads.iter().map(|row| row[0].clone()).collect();
    let mut expected: Vec<Value> = required
        .iter()
        .map(|name| text(&format!("{projects_text}/{name}")))
        .collect();
    expected.sort_by_key(|value| value.to_string());
    assert_eq!(roots, expected, "one HEAD per required repository");
    for row in &heads {
        assert_eq!(row[1].as_str().unwrap().len(), 40, "sha {row:?}");
    }
}

// From tests/_23_comptime_effect.rs
// `effect` rows answered inside the compiler fixpoint, through the real
// binary. Each `fixtures/comptime_effect/*.dl7` runs `dl8 compile --trace`
// from the crate root, so the document path in its seed is relative to it.
//
// The trace carries one `Round::Answer` line per answer step; its count is the
// COUNT receipt. The last `Decision` line is the comptime fixpoint's stable
// round (the first belongs to the macro program).

const LIMIT: Duration = Duration::from_secs(10);

struct s23comptimeeffect_Compiled {
    out: Value,
    trace: String,
    code: i32,
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn s23comptimeeffect_compile_with_engine(
    stem: &str,
    engine: Option<&str>,
) -> s23comptimeeffect_Compiled {
    let source = format!("fixtures/comptime_effect/{stem}.dl7");
    let started = Instant::now();
    let mut command = Command::new(env!("CARGO_BIN_EXE_dl8"));
    command
        .current_dir(root())
        .args(["compile", "--trace", &source]);
    if let Some(engine) = engine {
        command.env("DL8_ENGINE", engine);
    }
    let output = command.output().expect("spawn dl8");
    let took = started.elapsed();
    assert!(took < LIMIT, "{stem}: compile took {took:?}");
    let out = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{stem}: no JSON ({e}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    s23comptimeeffect_Compiled {
        out,
        trace: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code().unwrap_or(-1),
    }
}

fn s23comptimeeffect_compile(stem: &str) -> s23comptimeeffect_Compiled {
    s23comptimeeffect_compile_with_engine(stem, None)
}

fn kernel(name: &str) -> Value {
    json!({"f": "ref", "args": [{"f": "kernel", "args": [{"a": name}]}]})
}

/// Compiler rows of the relation `rel` names.
fn s23comptimeeffect_rows(compiled: &s23comptimeeffect_Compiled, rel: &Value) -> usize {
    compiled.out["compiler_rows"]
        .as_array()
        .expect("compiler_rows")
        .iter()
        .filter(|row| &row["args"][0] == rel)
        .count()
}

fn named(compiled: &s23comptimeeffect_Compiled, name: &str) -> usize {
    let rel = compiled.out["program"]["names"][name].clone();
    assert!(!rel.is_null(), "no name {name}");
    s23comptimeeffect_rows(compiled, &rel)
}

fn answers(compiled: &s23comptimeeffect_Compiled) -> usize {
    compiled.trace.matches("Answer {").count()
}

fn stable_round(compiled: &s23comptimeeffect_Compiled) -> Option<i64> {
    let line = compiled
        .trace
        .lines()
        .filter(|line| line.contains("outcome: Stable"))
        .last()?;
    let rest = line.split("round: ").nth(1)?;
    rest.split(',').next()?.trim().parse().ok()
}

fn assert_clean(stem: &str, compiled: &s23comptimeeffect_Compiled) {
    assert_eq!(compiled.code, 0, "{stem}: {}", compiled.out["diagnostics"]);
    assert_eq!(compiled.out["diagnostics"], json!([]), "{stem}");
}

#[test]
pub fn a_once_executor_answers_its_effect_row_at_comptime() {
    let compiled = s23comptimeeffect_compile("0_fs_json");
    assert_clean("0_fs_json", &compiled);
    assert_eq!(named(&compiled, "Member"), 2, "title and done");
    assert_eq!(named(&compiled, "fs.json"), 1);
    assert_eq!(s23comptimeeffect_rows(&compiled, &kernel("effect")), 1);
    assert_eq!(answers(&compiled), 1);
    assert_eq!(stable_round(&compiled), Some(3));
}

#[test]
pub fn ir_comptime_stops_when_the_fs_json_request_is_stable() {
    // `ir:sqlite` is omitted while ivm-sqlite's `install_program` does not
    // finish this program within the 20-second binary-run budget.
    for engine in ["ir:dd"] {
        let compiled = s23comptimeeffect_compile_with_engine("0_fs_json", Some(engine));
        assert_clean(engine, &compiled);
        assert_eq!(named(&compiled, "Member"), 2, "{engine}: title and done");
        assert_eq!(named(&compiled, "fs.json"), 1, "{engine}: one answer row");
        assert_eq!(answers(&compiled), 1, "{engine}: executor calls");
        // IR settles the answer and its dependent rules in one live-engine round.
        assert_eq!(stable_round(&compiled), Some(2), "{engine}: stable round");
    }
}

#[test]
pub fn a_relation_with_no_executor_is_never_asked() {
    let compiled = s23comptimeeffect_compile("1_no_executor");
    assert_clean("1_no_executor", &compiled);
    assert_eq!(named(&compiled, "Member"), 0);
    assert_eq!(s23comptimeeffect_rows(&compiled, &kernel("effect")), 0);
    assert_eq!(answers(&compiled), 0);
    assert_eq!(stable_round(&compiled), Some(1));
}

/// Comptime leaves `timer` unserved; `dl8 eval --serve timer` on the same
/// program still writes its effect row.
#[test]
pub fn a_continuing_executor_waits_for_the_runtime() {
    let compiled = s23comptimeeffect_compile("2_continuing");
    assert_clean("2_continuing", &compiled);
    assert_eq!(s23comptimeeffect_rows(&compiled, &kernel("effect")), 0);
    assert_eq!(answers(&compiled), 0);

    let path =
        std::env::temp_dir().join(format!("dl8-comptime-effect-{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&compiled.out).unwrap()).unwrap();
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .args(["eval", &path.to_string_lossy(), "--serve", "timer"])
        .output()
        .expect("spawn dl8");
    let _ = std::fs::remove_file(&path);
    assert!(started.elapsed() < LIMIT);
    let out: Value = serde_json::from_slice(&output.stdout).expect("eval JSON");
    let effects = out["closure"]
        .as_array()
        .expect("closure")
        .iter()
        .filter(|row| row["rel"] == kernel("effect"))
        .count();
    assert_eq!(effects, 1, "{}", out["closure"]);
}

#[test]
pub fn answered_rows_feed_a_rule_that_mints_an_edge() {
    let compiled = s23comptimeeffect_compile("3_two_rounds");
    assert_clean("3_two_rounds", &compiled);
    assert_eq!(named(&compiled, "Heading"), 1);
    assert_eq!(answers(&compiled), 1);
    assert_eq!(stable_round(&compiled), Some(3));
}
