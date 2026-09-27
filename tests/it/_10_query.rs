//! `dl8 query` through the real binary over one throwaway git repository.
//! Each source family is compared with `dl8 run` using the same compiled
//! program and the Rust engine. The file test also reads the existing
//! `dl8::fs_at` `files` trace field as the executor's enumeration count.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

const FIRST_DATE: &str = "1700000000 +0000";
const SECOND_DATE: &str = "1700000100 +0000";

struct Scratch {
    directory: PathBuf,
    repo: PathBuf,
    first: String,
    second: String,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
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

fn commit(root: &Path, date: &str, paths: &[&str], message: &str) -> String {
    for path in paths {
        git(root, date, &["add", path]);
    }
    git(root, date, &["commit", "-q", "-m", message]);
    git(root, date, &["rev-parse", "HEAD"])
}

fn scratch(name: &str) -> Scratch {
    let directory = std::env::temp_dir().join(format!(
        "dl8-query-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    let repo = directory.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, FIRST_DATE, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.join("b.txt"), "two\n").unwrap();
    let first = commit(&repo, FIRST_DATE, &["a.txt", "b.txt"], "first");
    std::fs::write(repo.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").unwrap();
    let second = commit(&repo, SECOND_DATE, &["src/lib.rs"], "second");
    Scratch {
        directory,
        repo,
        first,
        second,
    }
}

fn command_output(command: &mut Command, label: &str) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{label} exited {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn query(case: &Scratch, source: &Path, format: &str, log: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dl8"));
    command
        .args(["query"])
        .arg(source)
        .arg("--repo")
        .arg(&case.repo)
        .args(["--format", format])
        .env("DL8_ENGINE", "rust");
    if let Some(log) = log {
        command.env("RUST_LOG", log);
    }
    command_output(&mut command, &format!("query --format {format}"))
}

fn rust_eval(case: &Scratch, source: &Path, served: &str, relation: &str) -> Vec<Value> {
    let compiled = command_output(
        Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("compile")
            .arg(source)
            .current_dir(&case.repo),
        "compile",
    );
    let compiled_json: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(compiled_json["diagnostics"], json!([]));
    let program = case.directory.join("program.json");
    std::fs::write(&program, compiled.stdout).unwrap();
    let evaluated = command_output(
        Command::new(env!("CARGO_BIN_EXE_dl8"))
            .arg("run")
            .arg(&program)
            .args(["--serve", served])
            .env("DL8_ENGINE", "rust")
            .current_dir(&case.repo),
        "run",
    );
    let evaluated: Value = serde_json::from_slice(&evaluated.stdout).unwrap();
    assert_eq!(evaluated["diagnostics"], json!([]));
    let relation_id = &compiled_json["program"]["names"][relation];
    let mut rows: Vec<Value> = evaluated["closure"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| &row["rel"] == relation_id)
        .map(|row| json!({"relation": relation, "args": row["args"]}))
        .collect();
    rows.sort_by_key(Value::to_string);
    rows
}

fn json_rows(output: &Output) -> Vec<Value> {
    let mut rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "query JSON: {error}; stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    rows.sort_by_key(Value::to_string);
    rows
}

fn traced_count(trace: &str, target: &str, field: &str) -> usize {
    let lines: Vec<&str> = trace
        .lines()
        .filter(|line| line.contains(target) && line.contains(field))
        .collect();
    assert_eq!(lines.len(), 1, "one {target} trace event: {trace}");
    let suffix = lines[0]
        .split_once(field)
        .unwrap_or_else(|| panic!("{field} in {}", lines[0]))
        .1
        .trim_start_matches(|character| character == ' ' || character == '=');
    suffix
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|error| panic!("{field} count in {}: {error}", lines[0]))
}

#[test]
fn files_query_matches_rust_eval_in_json_and_table_and_counts_enumerated_files() {
    let case = scratch("files");
    let source = case.directory.join("files.dl7");
    std::fs::write(
        &source,
        r#"(fs: (import "@std/fs"))
(: Scan (* (: root str) (: sha str)))
(Scan "." "HEAD")
(: File (* (: path str) (: blob str)))
(<- (File "a.txt" ?Blob)
    (Scan ?Root ?Sha)
    (fs.at ?Root ?Sha "a.txt" ?Blob))
"#,
    )
    .unwrap();

    let json = query(&case, &source, "json", Some("dl8::fs_at=info"));
    let rows = json_rows(&json);
    assert_eq!(rows, rust_eval(&case, &source, "fs.at", "File"));
    assert_eq!(rows.len(), 1, "one constant-path match");
    assert_eq!(
        rows[0]["args"][0],
        json!({"f": "const", "args": [{"s": "a.txt"}]})
    );

    let table = query(&case, &source, "table", None);
    let table = String::from_utf8(table.stdout).unwrap();
    assert_eq!(
        table,
        format!(
            "(File \"a.txt\" \"{}\")\n",
            rows[0]["args"][1]["args"][0]["s"].as_str().unwrap()
        )
    );

    let trace = String::from_utf8(json.stderr).unwrap();
    let enumerated = traced_count(&trace, "dl8::fs_at", "files");
    let matched = rows.len();
    const K: usize = 2;
    assert_eq!(enumerated, matched + K, "fs.at trace files = matched + K");
}

#[test]
fn git_query_matches_rust_eval_in_json_and_table() {
    let case = scratch("git");
    let source = case.directory.join("git.dl7");
    std::fs::write(
        &source,
        r#"(git: (import "@std/git"))
(: Scan (* (: root str)))
(Scan ".")
(: Edge (* (: sha str) (: parent str)))
(<- (Edge ?Sha ?Parent)
    (Scan ?Root)
    (git.history ?Root ?Sha ?Parent))
"#,
    )
    .unwrap();

    let json = query(&case, &source, "json", None);
    let rows = json_rows(&json);
    assert_eq!(rows, rust_eval(&case, &source, "git.history", "Edge"));
    assert_eq!(
        rows,
        vec![json!({
            "relation": "Edge",
            "args": [
                {"f": "const", "args": [{"s": case.second}]},
                {"f": "const", "args": [{"s": case.first}]}
            ]
        })]
    );

    let table = query(&case, &source, "table", None);
    assert_eq!(
        String::from_utf8(table.stdout).unwrap(),
        format!("(Edge \"{}\" \"{}\")\n", case.second, case.first)
    );
}

#[test]
fn git_refs_query_takes_one_snapshot_without_a_watcher() {
    let case = scratch("refs");
    let source = case.directory.join("refs.dl7");
    std::fs::write(
        &source,
        r#"(git: (import "@std/git"))
(: Scan (* (: root str)))
(Scan ".")
(: Head (* (: sha str)))
(<- (Head ?Sha)
    (Scan ?Root)
    (git.refs ?Root "HEAD" ?Sha))
"#,
    )
    .unwrap();

    let json = query(&case, &source, "json", Some("dl8::git_refs=info"));
    assert_eq!(
        json_rows(&json),
        vec![json!({
            "relation": "Head",
            "args": [{"f": "const", "args": [{"s": case.second}]}]
        })]
    );
    let trace = String::from_utf8(json.stderr).unwrap();
    let events: Vec<&str> = trace
        .lines()
        .filter(|line| line.contains("dl8::git_refs") && line.contains("watcher"))
        .collect();
    assert_eq!(events.len(), 1, "one git.refs snapshot event: {trace}");
    assert!(
        events[0].contains("watcher=false") || events[0].contains("watcher = false"),
        "{}",
        events[0]
    );

    let table = query(&case, &source, "table", None);
    assert_eq!(
        String::from_utf8(table.stdout).unwrap(),
        format!("(Head \"{}\")\n", case.second)
    );
}
