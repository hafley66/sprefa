//! `dl8 run` through the real binary with the git and extract executors. Each
//! test makes a throwaway repository with the `git` CLI, the test being the
//! boundary; the executors under test reach git only through soopy. The
//! `fixtures/hosts/*.dl7` programs spell their roots `__ROOT__`. Every run is
//! killed past 10 s.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

const RUN_CAP: Duration = Duration::from_secs(10);

/// Over this, building extract is a defect to report rather than a budget to keep.
const BUILD_BUDGET: Duration = Duration::from_secs(60);

const FIRST_DATE: &str = "1700000000 +0000";
const SECOND_DATE: &str = "1700000100 +0000";

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("dl8-hosts-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::canonicalize(&directory).unwrap()
}

fn fixture(name: &str) -> PathBuf {
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

struct Compiled {
    program: PathBuf,
    names: Value,
}

fn compile(directory: &Path, name: &str, replacements: &[(&str, &str)]) -> Compiled {
    let mut text = std::fs::read_to_string(fixture(name)).unwrap();
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
    Compiled {
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
            let left = RUN_CAP.saturating_sub(self.started.elapsed());
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
                        "no stderr line with {needle:?} within {RUN_CAP:?}: {:#?}",
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
            if self.started.elapsed() > RUN_CAP {
                let _ = self.child.kill();
                self.stderr.extend(self.stderr_lines.try_iter());
                panic!("dl8 run exceeded {RUN_CAP:?}: {:#?}", self.stderr);
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

fn run(program: &Path, args: &[&str], env: &[(&str, &str)]) -> (Value, i32) {
    spawn(program, args, env).finish()
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
pub fn git_refs_answers_the_snapshot_then_the_ref_that_moved() {
    let directory = scratch("refs");
    let (root, first) = repository(&directory);
    let root_text = root.to_string_lossy().to_string();
    let compiled = compile(&directory, "0_refs.dl7", &[("__ROOT__", &root_text)]);
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
    assert_eq!(rows(&out, &compiled.names, "git.refs"), expected);
    let mut heads = vec![vec![text(&first)], vec![text(&second)]];
    heads.sort_by_key(|row| row[0].to_string());
    assert_eq!(rows(&out, &compiled.names, "Head"), heads);
    assert!(rows(&out, &compiled.names, "git.refs_error").is_empty());
}

#[test]
pub fn git_refs_on_a_directory_that_is_no_repository_is_an_error_row() {
    let directory = scratch("refs-missing");
    let root_text = directory.join("nothing").to_string_lossy().to_string();
    let compiled = compile(&directory, "0_refs.dl7", &[("__ROOT__", &root_text)]);
    let (out, code) = run(
        &compiled.program,
        &["--serve", "git.refs", "--max-ticks", "1"],
        &[],
    );
    assert_eq!(code, 0, "exit code");
    assert!(rows(&out, &compiled.names, "git.refs").is_empty());
    let errors = rows(&out, &compiled.names, "git.refs_error");
    assert_eq!(errors.len(), 1, "errors {errors:?}");
    assert_eq!(errors[0][0], text(&root_text));
}

#[test]
pub fn git_history_answers_one_edge_per_parent() {
    let directory = scratch("history");
    let (root, first) = repository(&directory);
    let second = commit(&root, SECOND_DATE, "b.txt", "two\n");
    let root_text = root.to_string_lossy().to_string();
    let compiled = compile(&directory, "1_history.dl7", &[("__ROOT__", &root_text)]);
    let (out, code) = run(&compiled.program, &["--serve", "git.history"], &[]);
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(
        rows(&out, &compiled.names, "Edge"),
        vec![vec![text(&second), text(&first)]]
    );
    assert!(rows(&out, &compiled.names, "Failed").is_empty());
    assert_eq!(out["ticks"], json!(1));
}

#[test]
pub fn git_history_on_a_directory_that_is_no_repository_is_an_error_row() {
    let directory = scratch("history-missing");
    let root_text = directory.join("nothing").to_string_lossy().to_string();
    let compiled = compile(&directory, "1_history.dl7", &[("__ROOT__", &root_text)]);
    let (out, code) = run(&compiled.program, &["--serve", "git.history"], &[]);
    assert_eq!(code, 0, "exit code");
    assert!(rows(&out, &compiled.names, "Edge").is_empty());
    assert_eq!(rows(&out, &compiled.names, "Failed").len(), 1);
}

#[test]
pub fn fs_at_answers_the_files_and_blobs_of_each_revision() {
    let directory = scratch("repo-at");
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
        let compiled = compile(
            &directory,
            "2_fs_at.dl7",
            &[("__ROOT__", &root_text), ("__SHA__", &revision)],
        );
        let (out, code) = run(&compiled.program, &["--serve", "fs.at"], &[]);
        assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
        assert_eq!(
            rows(&out, &compiled.names, "File"),
            expected,
            "at {revision}"
        );
        assert!(rows(&out, &compiled.names, "fs.at_error").is_empty());
    }
}

fn sprefa_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn built_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(shared) = std::env::var_os("CARGO_TARGET_DIR") {
        out.push(PathBuf::from(shared).join("debug/extract"));
    }
    out.push(sprefa_root().join("hafley-rs/target/debug/extract"));
    out.push(sprefa_root().join("hafley-rs/crates/sprefa-extract/target/debug/extract"));
    out
}

/// The `tests/_16_extract_tsi.rs` search, then one build; the found path is
/// handed to `dl8 run` as `SPREFA_EXTRACT_BIN`.
fn extract_bin() -> PathBuf {
    if let Some(named) = std::env::var_os("SPREFA_EXTRACT_BIN") {
        return PathBuf::from(named);
    }
    if let Some(built) = built_candidates().into_iter().find(|path| path.exists()) {
        return built;
    }
    let linked = sprefa_root().join("hafley-rs/crates/sprefa-extract/Cargo.toml");
    let manifest = std::fs::canonicalize(&linked).unwrap_or(linked);
    assert!(
        manifest.exists(),
        "no sprefa-extract crate at {}; link hafley-rs or set SPREFA_EXTRACT_BIN",
        manifest.display()
    );
    let start = Instant::now();
    let output = Command::new("cargo")
        .args([
            "build",
            "--features",
            "cli",
            "--bin",
            "extract",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cargo build");
    assert!(
        output.status.success(),
        "building extract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        start.elapsed() < BUILD_BUDGET,
        "building extract took {} ms",
        start.elapsed().as_millis()
    );
    built_candidates()
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("cargo build left no extract in {:?}", built_candidates()))
}

#[test]
pub fn extract_answers_rows_for_each_family_over_the_corpus() {
    let directory = scratch("extract");
    let corpus = std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/extract/corpus"),
    )
    .unwrap();
    let corpus_text = corpus.to_string_lossy().to_string();
    let binary = extract_bin().to_string_lossy().to_string();
    let compiled = compile(&directory, "3_extract.dl7", &[("__ROOT__", &corpus_text)]);
    let (out, code) = run(
        &compiled.program,
        &["--serve", "extract"],
        &[("SPREFA_EXTRACT_BIN", &binary)],
    );
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert_eq!(
        rows(&out, &compiled.names, "Failed"),
        Vec::<Vec<Value>>::new()
    );
    let facts = rows(&out, &compiled.names, "Fact");
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
    let directory = scratch("extract-missing");
    let missing = directory.join("no-extract").to_string_lossy().to_string();
    let compiled = compile(
        &directory,
        "3_extract.dl7",
        &[("__ROOT__", &directory.to_string_lossy())],
    );
    let (out, code) = run(
        &compiled.program,
        &["--serve", "extract"],
        &[("SPREFA_EXTRACT_BIN", &missing)],
    );
    assert_eq!(code, 0, "exit code");
    assert!(rows(&out, &compiled.names, "Fact").is_empty());
    let failed = rows(&out, &compiled.names, "Failed");
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
    let directory = scratch("org");
    let projects_text = projects.to_string_lossy().to_string();
    let compiled = compile(&directory, "org.dl7", &[("__PROJECTS__", &projects_text)]);
    let (out, code) = run(
        &compiled.program,
        &["--serve", "git.refs", "--max-ticks", "1"],
        &[],
    );
    assert_eq!(code, 0, "exit code; diagnostics {}", out["diagnostics"]);
    assert!(rows(&out, &compiled.names, "git.refs_error").is_empty());
    let heads = rows(&out, &compiled.names, "head_sha");
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
