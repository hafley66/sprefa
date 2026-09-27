//! Continuing executor frontiers through the public `dl8 run` command.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::Duration;

static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);
const EVENT_CAP: Duration = Duration::from_secs(10);
const QUIET_CAP: Duration = Duration::from_millis(500);

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(engine: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "dl8-fs-watch-{}-{}-{}",
        std::process::id(),
        engine.replace(':', "-"),
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

struct Compiled {
    path: PathBuf,
    names: Value,
}

fn compile(directory: &Path) -> Compiled {
    let source = directory.join("watch.dl7");
    std::fs::write(
        &source,
        r#"(fs: (import "@std/fs"))

(: want (* (: path str)))
(: Wants (* (: path str)))
(<- (Wants ?Path) (want ?Path))

(: Body (* (: path str) (: body str)))
(<- (Body ?Path ?Body) (Wants ?Path) (fs.watch ?Path ?Body))

(: Dependent (* (: body str)))
(<- (Dependent ?Body) (Body ?Path ?Body))

(: Independent (* (: value str)))
(Independent "stable")
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(&source)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let compiled: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "compile JSON: {error}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(compiled["diagnostics"], json!([]));
    let program = directory.join("watch.json");
    std::fs::write(&program, output.stdout).unwrap();
    Compiled {
        path: program,
        names: compiled["program"]["names"].clone(),
    }
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    stdout: Receiver<Result<Value, String>>,
    stderr: Receiver<String>,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    fn spawn(program: &Path, engine: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("run")
            .arg(program)
            .args(["--serve", "fs.watch", "--frontiers", "-"])
            .env("DL8_ENGINE", engine)
            .env("RUST_LOG", "dl8=info")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (stdout_send, stdout_recv) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let value = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|error| format!("{error}: {line}"))
                });
                if stdout_send.send(value).is_err() {
                    return;
                }
            }
        });
        let (stderr_send, stderr_recv) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if stderr_send.send(line).is_err() {
                    return;
                }
            }
        });
        Self {
            child,
            stdin,
            stdout: stdout_recv,
            stderr: stderr_recv,
        }
    }

    fn next(&mut self, label: &str) -> Value {
        match self.stdout.recv_timeout(EVENT_CAP) {
            Ok(value) => value.unwrap_or_else(|error| panic!("{label}: {error}")),
            Err(error) => panic!(
                "{label}: no settle within {EVENT_CAP:?}: {error}; stderr: {:?}",
                self.stderr.try_iter().collect::<Vec<_>>()
            ),
        }
    }

    fn frontier(&mut self, path: &Path, weight: i8) {
        serde_json::to_writer(
            &mut self.stdin,
            &json!({"changes":[{"rel":"want","args":[{"f":"const","args":[{"s":path.to_string_lossy()}]}],"w":weight}]}),
        )
        .unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
    }

    fn assert_quiet(&self, label: &str) {
        match self.stdout.recv_timeout(QUIET_CAP) {
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => panic!("{label}: dl8 stdout closed"),
            Ok(value) => panic!("{label}: unexpected settle: {value:?}"),
        }
    }
}

fn deltas(receipt: &Value, relation: &str, weight: i8) -> usize {
    receipt["deltas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|delta| delta["rel"] == relation && delta["w"] == weight)
        .count()
}

fn counter(receipt: &Value, name: &str) -> u64 {
    receipt["counters"][name]
        .as_u64()
        .unwrap_or_else(|| panic!("counter {name}: {}", receipt["counters"]))
}

fn rows(receipt: &Value, names: &Value, name: &str) -> Vec<Value> {
    let relation = &names[name];
    assert!(!relation.is_null(), "missing relation {name}");
    let mut rows: Vec<_> = receipt["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation)
        .cloned()
        .collect();
    rows.sort_by_key(Value::to_string);
    rows
}

pub(super) fn check_fs_watch_edits_retract_requests_and_increment_only_dependents() {
    for engine in ["ir:dd", "ir:sqlite"] {
        let case = scratch(engine);
        let watched = case.0.join("watched.txt");
        std::fs::write(&watched, "before\n").unwrap();
        let program = compile(&case.0);
        let mut running = Running::spawn(&program.path, engine);

        let initial = running.next("initial");
        assert_eq!(initial["ticks"], json!(0), "{engine}: initial tick");

        running.frontier(&watched, 1);
        let requested = running.next("request");
        assert_eq!(
            deltas(&requested, "fs.watch.answer", 1),
            1,
            "{engine}: request did not arm fs.watch: {requested}"
        );
        let independent = rows(&requested, &program.names, "Independent");
        assert_eq!(independent.len(), 1, "{engine}: independent fixture row");
        assert_eq!(
            rows(&requested, &program.names, "Body").len(),
            1,
            "{engine}: initial watched body: {requested}"
        );

        std::fs::write(&watched, "after\n").unwrap();
        let edited = running.next("edit");
        assert_eq!(
            edited["ticks"].as_u64(),
            requested["ticks"].as_u64().map(|tick| tick + 1),
            "{engine}: one edit settle"
        );
        assert_eq!(deltas(&edited, "fs.watch.answer", -1), 1, "{engine}");
        assert_eq!(deltas(&edited, "fs.watch.answer", 1), 1, "{engine}");
        assert_eq!(
            rows(&edited, &program.names, "Independent"),
            independent,
            "{engine}: independent closure"
        );
        assert_eq!(rows(&edited, &program.names, "Body").len(), 1, "{engine}");
        assert_eq!(
            rows(&edited, &program.names, "Dependent").len(),
            1,
            "{engine}"
        );
        assert_eq!(
            counter(&edited, "rows_written") - counter(&requested, "rows_written"),
            2,
            "{engine}: edit writes"
        );

        running.frontier(&watched, -1);
        let _retracted = running.next("retract");
        std::fs::write(&watched, "ignored\n").unwrap();
        running.assert_quiet(&format!("{engine}: edit after request retraction"));
    }
}
