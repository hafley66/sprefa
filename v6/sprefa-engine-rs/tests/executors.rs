// TEST: one case per linked executor, every one against a local listener.
// Sabotage receipt: dropping the `headers` loop in `http::send` turns
// `every_request_header_comes_from_the_row` red on the Authorization assertion.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sprefa_engine_rs::executors::http::{send_all, Method, Request};
use sprefa_engine_rs::executors::{
    EnvExecutor, HttpGetExecutor, HttpPostExecutor, SoopyCheckoutExecutor, TomlJsonExecutor,
};
use sprefa_engine_rs::hosts::IHostExecutor;

/// `std::env::set_var` mutates one process table, so the two cases that touch
/// it never run beside each other.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn serialized() -> MutexGuard<'static, ()> {
    ENVIRONMENT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn inputs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[test]
fn env_var_absent_answers_zero_rows() {
    let _guard = serialized();
    std::env::set_var("DL_GHCACHER_TEST_VAR", "/env/config.toml");
    let present = EnvExecutor
        .run("env_var", "", &inputs(&[("name", "DL_GHCACHER_TEST_VAR")]))
        .expect("present var");
    std::env::remove_var("DL_GHCACHER_TEST_VAR");
    assert_eq!(present[0]["value"], "/env/config.toml");

    let absent = EnvExecutor
        .run("env_var", "", &inputs(&[("name", "DL_GHCACHER_NEVER_SET")]))
        .expect("absent var");
    assert!(
        absent.is_empty(),
        "an unset variable is zero rows, never an empty row"
    );
}

#[test]
fn toml_json_decodes_a_document() {
    let home = tempfile::tempdir().expect("tempdir");
    let path = home.path().join("config.toml");
    std::fs::write(&path, "[global]\ndb_path = \"/repo/db.sqlite\"\n").expect("write toml");
    let spelled = path.to_string_lossy().to_string();

    let decoded = TomlJsonExecutor
        .run("toml_json", "", &inputs(&[("config_path", &spelled)]))
        .expect("decode");
    assert_eq!(decoded[0]["doc"]["global"]["db_path"], "/repo/db.sqlite");

    let missing = home
        .path()
        .join("absent.toml")
        .to_string_lossy()
        .to_string();
    let empty = TomlJsonExecutor
        .run("toml_json", "", &inputs(&[("config_path", &missing)]))
        .expect("absent file");
    assert!(empty.is_empty(), "a missing config is zero rows");
}

#[test]
fn soopy_checkout_reads_head_and_names_the_clone_gap() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root");
    let dest_root = root
        .parent()
        .expect("parent of root")
        .to_string_lossy()
        .to_string();
    let repo_slug = root
        .file_name()
        .expect("root directory name")
        .to_string_lossy()
        .to_string();

    let stdout = SoopyCheckoutExecutor
        .run(
            "repo_checkout",
            "",
            &inputs(&[
                ("repo_slug", &repo_slug),
                ("dest_root", &dest_root),
                ("want_sha", ""),
            ]),
        )
        .expect("observe this checkout");
    let head = stdout[0]["head_sha"].as_str().expect("head_sha text");
    assert_eq!(head.len(), 40, "a resolved HEAD is a full object id");
    assert!(head.chars().all(|c| c.is_ascii_hexdigit()));

    let failure = SoopyCheckoutExecutor
        .run(
            "repo_checkout",
            "",
            &inputs(&[
                ("repo_slug", "never/cloned"),
                ("dest_root", &dest_root),
                ("want_sha", "sha-a1"),
            ]),
        )
        .expect_err("an absent checkout is a named stop");
    assert!(
        failure.message.contains("soopy_clone_missing"),
        "{}",
        failure.message
    );
}

// RULING executor_namespacing: registry.pl arrival_executor/2 is the one
// roster; LINKED_EXECUTORS and executor_for answer the same names.
#[test]
fn executor_roster_matches_registry() {
    let registry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../prolog/compile/registry.pl");
    let source = std::fs::read_to_string(&registry)
        .unwrap_or_else(|error| panic!("read {}: {error}", registry.display()));
    let mut registry_names: Vec<String> = source
        .lines()
        .filter_map(|line| {
            let row = line
                .trim()
                .strip_prefix("arrival_executor(")?
                .strip_suffix(").")?;
            let dotted = row.split(',').nth(1)?;
            Some(dotted.trim().trim_matches('\'').to_string())
        })
        .collect();
    let mut linked: Vec<String> = sprefa_engine_rs::hosts::LINKED_EXECUTORS
        .split(',')
        .map(|name| name.trim().to_string())
        .collect();
    registry_names.sort();
    registry_names.dedup();
    linked.sort();
    assert!(
        !registry_names.is_empty(),
        "no arrival_executor rows parsed"
    );
    assert_eq!(
        registry_names, linked,
        "registry roster != LINKED_EXECUTORS"
    );
    for name in &linked {
        assert!(
            sprefa_engine_rs::hosts::executor_for(name).is_some(),
            "no executor links roster name {name}"
        );
    }
}

// ═══ the one transport ══════════════════════════════════════════════════════

/// What one served request saw: the header lines and the request body, so a
/// case can assert the wire rather than only the answer.
#[derive(Default)]
struct Seen {
    lines: Vec<String>,
    body: String,
}

fn read_request(stream: &mut TcpStream) -> Seen {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut seen = Seen::default();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return seen;
        }
        let lowered = line.to_ascii_lowercase();
        if let Some(rest) = lowered.strip_prefix("content-length:") {
            length = rest.trim().parse().unwrap_or(0);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        seen.lines.push(line.trim_end().to_string());
    }
    let mut body = vec![0u8; length];
    if length > 0 && reader.read_exact(&mut body).is_ok() {
        seen.body = String::from_utf8_lossy(&body).to_string();
    }
    seen
}

fn write_response(stream: &mut TcpStream, status: &str, headers: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write response");
    stream.flush().expect("flush response");
}

/// A listener that records every request and answers 304 to a conditional ask.
fn serve(
    body: String,
    etag: &'static str,
    rounds: usize,
) -> (String, Arc<Mutex<Vec<Seen>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let log = Arc::new(Mutex::new(Vec::new()));
    let recorded = log.clone();
    let handle = std::thread::spawn(move || {
        for _ in 0..rounds {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let seen = read_request(&mut stream);
            let conditional = seen.lines.iter().any(|line| {
                line.to_ascii_lowercase().starts_with("if-none-match:") && line.contains(etag)
            });
            if conditional {
                write_response(
                    &mut stream,
                    "304 Not Modified",
                    &format!("ETag: {etag}\r\n"),
                    "",
                );
            } else {
                write_response(
                    &mut stream,
                    "200 OK",
                    &format!("ETag: {etag}\r\nX-RateLimit-Remaining: 4998\r\nContent-Type: application/json\r\n"),
                    &body,
                );
            }
            recorded.lock().expect("request log").push(seen);
        }
    });
    (format!("http://127.0.0.1:{port}"), log, handle)
}

/// The 304 is the PROGRAM's conditional GET: the executor holds no ETag, so
/// `If-None-Match` reaches the wire only because the row carried it.
#[test]
fn http_get_answers_200_then_304_from_the_rows_own_header() {
    let body = r#"{"full_name":"cli/cli","stargazers_count":7}"#.to_string();
    let (base, _log, handle) = serve(body.clone(), "\"tag-v1\"", 2);
    let url = format!("{base}/repos/cli/cli");

    let first = HttpGetExecutor
        .run(
            "http_get",
            "",
            &inputs(&[
                ("url", &url),
                ("headers", r#"{"Accept":"application/vnd.github+json"}"#),
                ("prev_etag", ""),
            ]),
        )
        .expect("first GET");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["status"], 200);
    assert_eq!(
        first[0]["body"], body,
        "the body column is the document text"
    );
    assert_eq!(first[0]["bytes"], body.len());
    let headers: serde_json::Value =
        serde_json::from_str(first[0]["response_headers"].as_str().expect("header text"))
            .expect("a json object of headers");
    assert_eq!(headers["etag"], "\"tag-v1\"", "header names are lowercased");
    assert_eq!(
        headers["x-ratelimit-remaining"], 4998,
        "a whole-number header is a JSON number so `decode(.., R: int)` reads it"
    );

    let second = HttpGetExecutor
        .run(
            "http_get",
            "",
            &inputs(&[
                ("url", &url),
                ("headers", r#"{"If-None-Match":"\"tag-v1\""}"#),
                ("prev_etag", "\"tag-v1\""),
            ]),
        )
        .expect("conditional GET");
    assert_eq!(second[0]["status"], 304);
    assert_eq!(second[0]["bytes"], 0, "a 304 carries no body bytes");
    assert_eq!(
        second[0]["body"], "null",
        "the previous body is the PROGRAM's `last_body` join, never this executor's memory"
    );
    handle.join().expect("listener thread");
}

#[test]
fn every_request_header_comes_from_the_row() {
    let (base, log, handle) = serve("{}".to_string(), "\"t\"", 1);
    HttpGetExecutor
        .run(
            "http_get",
            "",
            &inputs(&[
                ("url", &format!("{base}/user")),
                (
                    "headers",
                    r#"{"Accept":"application/vnd.github+json","Authorization":"Bearer row-token"}"#,
                ),
            ]),
        )
        .expect("GET");
    handle.join().expect("listener thread");
    let seen = log.lock().expect("request log");
    let lines = seen[0].lines.join("\n").to_ascii_lowercase();
    assert!(
        lines.contains("authorization: bearer row-token"),
        "the token reaches the wire from the ROW, never from this process's env: {lines}"
    );
    assert!(lines.contains("accept: application/vnd.github+json"));
}

#[test]
fn http_post_sends_the_rows_request_body() {
    let (base, log, handle) = serve(r#"{"data":{"ok":1}}"#.to_string(), "\"t\"", 1);
    let query = r#"{"query":"query { viewer { login } }"}"#;
    let answered = HttpPostExecutor
        .run(
            "http_post",
            "",
            &inputs(&[
                ("url", &format!("{base}/graphql")),
                ("headers", r#"{"Content-Type":"application/json"}"#),
                ("request_body", query),
            ]),
        )
        .expect("POST");
    handle.join().expect("listener thread");
    assert_eq!(answered[0]["status"], 200);
    assert_eq!(answered[0]["body"], r#"{"data":{"ok":1}}"#);
    let seen = log.lock().expect("request log");
    assert_eq!(
        seen[0].body, query,
        "the GraphQL query is built by rule, not here"
    );
    assert!(
        seen[0].lines[0].starts_with("POST "),
        "{:?}",
        seen[0].lines[0]
    );
}

/// COUNT RECEIPT for the 10-second law. Eight endpoints against a listener that
/// holds each request 3s: serial is 24s, the bounded pool answers under 4s.
#[test]
fn eight_endpoints_against_a_slow_listener_answer_under_four_seconds() {
    let _guard = serialized();
    std::env::set_var("DL_HTTP_CONCURRENCY", "8");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let served = std::thread::spawn(move || {
        let mut answering = Vec::new();
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            answering.push(std::thread::spawn(move || {
                read_request(&mut stream);
                std::thread::sleep(Duration::from_secs(3));
                write_response(&mut stream, "200 OK", "", r#"{"ok":1}"#);
            }));
        }
        for handle in answering {
            let _ = handle.join();
        }
    });

    let requests: Vec<Request> = (1..=8)
        .map(|n| Request {
            host: "http_get".to_string(),
            method: Method::Get,
            url: format!("http://127.0.0.1:{port}/repos/org/r{n}"),
            headers: Vec::new(),
            body: None,
        })
        .collect();
    let started = Instant::now();
    let answers = send_all(&requests);
    let wall = started.elapsed();
    std::env::remove_var("DL_HTTP_CONCURRENCY");
    served.join().expect("listener thread");

    assert_eq!(answers.len(), 8);
    for (index, answered) in answers.iter().enumerate() {
        let row = answered.as_ref().expect("every endpoint answered");
        assert_eq!(row["status"], 200, "endpoint {index}");
    }
    assert!(
        wall < Duration::from_secs(4),
        "8 endpoints against a 3s listener took {wall:?}; serial would be 24s"
    );
}
