// TEST: one case per linked ghcacher executor, every one against a local
// fixture. Sabotage receipt: deleting the `If-None-Match` header in
// `fetch::conditional_get` turns `answers_200_then_304` red on the second GET.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use sprefa_engine_rs::executors::{
    EnvExecutor, GhReposExecutor, HttpFetchExecutor, SoopyCheckoutExecutor, TomlJsonExecutor,
};
use sprefa_engine_rs::hosts::IHostExecutor;

/// `std::env::set_var` mutates one process table, so the two cases that touch
/// it never run beside each other.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn serialized() -> MutexGuard<'static, ()> {
    ENVIRONMENT.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn inputs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// One request, one response, `Connection: close` so the next GET is a new
/// accept and the listener never has to multiplex.
fn answer_once(stream: &mut TcpStream, body: &str, etag: &str) -> bool {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut conditional = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if line.to_ascii_lowercase().starts_with("if-none-match:") && line.contains(etag) {
            conditional = true;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
    }
    let response = if conditional {
        format!("HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nConnection: close\r\n\r\n")
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nETag: {etag}\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };
    stream.write_all(response.as_bytes()).expect("write response");
    stream.flush().expect("flush response");
    conditional
}

fn serve(body: String, etag: &'static str, rounds: usize) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let handle = std::thread::spawn(move || {
        for _ in 0..rounds {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            answer_once(&mut stream, &body, etag);
        }
    });
    (format!("http://127.0.0.1:{port}"), handle)
}

#[test]
fn http_fetch_answers_200_then_304() {
    // The ETag store is one process-global map, so every case that calls
    // `forget_all` has to hold the same guard or they wipe each other.
    let _guard = serialized();
    sprefa_engine_rs::executors::fetch::forget_all();
    let body = r#"{"full_name":"cli/cli","stargazers_count":7}"#.to_string();
    let (base, handle) = serve(body.clone(), "\"tag-v1\"", 2);
    let url = format!("{base}/repos/cli/cli");

    let first = HttpFetchExecutor
        .run("fetch", "", &inputs(&[("ep", &url), ("prev", "")]))
        .expect("first GET");
    assert_eq!(first.len(), 1);
    let first = &first[0];
    assert_eq!(first["status"], 200);
    assert_eq!(first["tag"], "\"tag-v1\"");
    assert_eq!(first["full_name"], "cli/cli", "body fields splice in");
    assert_eq!(first["stargazers_count"], 7);

    let second = HttpFetchExecutor
        .run("fetch", "", &inputs(&[("ep", &url), ("prev", "\"tag-v1\"")]))
        .expect("conditional GET");
    let second = &second[0];
    assert_eq!(second["status"], 304);
    assert_eq!(second["bytes"], 0, "a 304 carries no body bytes");
    // CONTRACT CHANGE (ghcache lane): the ROW now answers the remembered body
    // rather than the empty string, the arm `repos.rs` and `pulls.rs` already
    // took. `bytes` above is what proves nothing crossed the wire; the body
    // column has to stay json_valid because a host may declare it `json`.
    assert_eq!(second["body"], body, "the previous body is the answer");
    assert_eq!(second["full_name"], "cli/cli", "so its fields still splice in");
    handle.join().expect("listener thread");
}

#[test]
fn gh_repos_walks_one_page_and_stops() {
    let _guard = serialized();
    sprefa_engine_rs::executors::fetch::forget_all();
    let body = r#"[{"full_name":"acme/one","stargazers_count":3,"default_branch":"main"},
                   {"full_name":"acme/two","stargazers_count":5,"default_branch":"trunk"}]"#
        .to_string();
    let (base, handle) = serve(body, "\"org-v1\"", 1);
    std::env::set_var("DL_GITHUB_API_BASE", &base);

    let rows = GhReposExecutor
        .run("gh_repos", "", &inputs(&[("org", "acme"), ("bucket", "1")]))
        .expect("org walk");
    std::env::remove_var("DL_GITHUB_API_BASE");
    assert_eq!(rows.len(), 2, "a short page ends the walk");
    assert_eq!(rows[0]["repo_slug"], "acme/one");
    assert_eq!(rows[0]["exists_flag"], 1);
    assert_eq!(rows[1]["stars"], 5);
    handle.join().expect("listener thread");
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
    assert!(absent.is_empty(), "an unset variable is zero rows, never an empty row");
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

    let missing = home.path().join("absent.toml").to_string_lossy().to_string();
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
    let dest_root = root.parent().expect("parent of root").to_string_lossy().to_string();
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
    let registry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../prolog/compile/registry.pl");
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
    assert!(!registry_names.is_empty(), "no arrival_executor rows parsed");
    assert_eq!(registry_names, linked, "registry roster != LINKED_EXECUTORS");
    for name in &linked {
        assert!(
            sprefa_engine_rs::hosts::executor_for(name).is_some(),
            "no executor links roster name {name}"
        );
    }
}

// COUNT RECEIPT for the batching seam: six endpoints ride ONE `eps` JSON
// array (json_group_array in the language), so one executor call answers six
// rows. The serve() rounds argument is the network-side count.
#[test]
fn http_fetch_batches_six_endpoints_in_one_executor_call() {
    let _hold = serialized();
    sprefa_engine_rs::executors::fetch::forget_all();
    let body = r#"{"kind":"row"}"#.to_string();
    let (base, handle) = serve(body, "\"tag-b\"", 6);
    std::env::set_var("DL_GITHUB_API_BASE", &base);
    let eps: Vec<String> = (1..=6).map(|n| format!("repos/org/r{n}")).collect();
    let eps_json = serde_json::to_string(&eps).expect("eps array");

    let rows = HttpFetchExecutor
        .run("gh_pr_poll", "", &inputs(&[("eps", &eps_json), ("prev", "")]))
        .expect("one batched executor call");
    std::env::remove_var("DL_GITHUB_API_BASE");
    handle.join().expect("listener served all six");

    assert_eq!(rows.len(), 6, "six endpoints, one call, six rows");
    for (row, ep) in rows.iter().zip(&eps) {
        assert_eq!(row["status"], 200);
        assert_eq!(row["ep"], ep.as_str(), "each row names its endpoint");
        assert_eq!(row["kind"], "row", "body fields splice in per endpoint");
    }
}

// TEST: prwatch-every-tick. The lane report measured 6857541 wire bytes on
// EVERY 60s tick over six ticks. These two cases separate the two candidate
// causes: whether the executor asks conditionally at all, and whether the
// PROGRAM's per-tick demand defeats it.
// Sabotage receipt: deleting the `If-None-Match` header in
// `fetch::conditional_get` turns `pulls_second_pass_is_a_304` red.

fn pulls_body() -> String {
    r#"[{"number":1,"title":"one","state":"open","updated_at":"2026-01-01T00:00:00Z",
         "head":{"ref":"feature/a","sha":"aaa"},"merge_commit_sha":"","draft":false}]"#
        .to_string()
}

#[test]
fn pulls_second_pass_is_a_304_and_moves_zero_bytes() {
    let _guard = serialized();
    sprefa_engine_rs::executors::fetch::forget_all();
    // Two rounds: page 1 of pass one, then page 1 of pass two. A short page
    // ends each walk, so the executor asks once per pass.
    let (base, handle) = serve(pulls_body(), "\"pulls-v1\"", 2);
    std::env::set_var("DL_GITHUB_API_BASE", &base);

    let before = sprefa_engine_rs::executors::cost::wire_bytes_total();
    let first = sprefa_engine_rs::executors::GhPullsExecutor
        .run("pulls", "", &inputs(&[("repo_slug", "o/n"), ("bucket", "1")]))
        .expect("first pass");
    let after_first = sprefa_engine_rs::executors::cost::wire_bytes_total();

    let second = sprefa_engine_rs::executors::GhPullsExecutor
        .run("pulls", "", &inputs(&[("repo_slug", "o/n"), ("bucket", "2")]))
        .expect("second pass");
    let after_second = sprefa_engine_rs::executors::cost::wire_bytes_total();
    std::env::remove_var("DL_GITHUB_API_BASE");

    assert_eq!(first.len(), 1, "pass one reads the page");
    assert_eq!(
        second.len(),
        1,
        "pass two answers the same rows from the remembered body"
    );
    assert!(
        after_first > before,
        "pass one moves the body over the wire"
    );
    assert_eq!(
        after_second, after_first,
        "pass two is a 304 and must move ZERO wire bytes; a fresh 200 here is \
         the prwatch-every-tick defect"
    );
    handle.join().expect("listener thread");
}

/// TEST: the prwatch-every-tick receipt. Ten conditional GETs against one
/// unchanged resource: the FIRST moves the body, the other nine are 304s that
/// move zero bytes, and the resident set does not climb across them.
/// The lane report measured 6857541 bytes on every one of six ticks and RSS
/// 47 -> 104 MB; this is the shape that measurement should have had.
/// Sabotage receipt: dropping `prev_etag` from the executor's inputs turns the
/// `wire bytes` assertion red on tick 2.
#[test]
fn ten_conditional_ticks_move_the_body_once_and_leave_rss_flat() {
    let _guard = serialized();
    sprefa_engine_rs::executors::fetch::forget_all();
    let (base, handle) = serve(pulls_body(), "\"pulls-v1\"", 10);
    std::env::set_var("DL_GITHUB_API_BASE", &base);

    let mut wire = Vec::new();
    let mut rss = Vec::new();
    let mut rows_per_tick = Vec::new();
    for bucket in 1..=10 {
        let before = sprefa_engine_rs::executors::cost::wire_bytes_total();
        let rows = sprefa_engine_rs::executors::GhPullsExecutor
            .run(
                "pulls",
                "",
                &inputs(&[("repo_slug", "o/n"), ("bucket", &bucket.to_string())]),
            )
            .expect("tick");
        wire.push(sprefa_engine_rs::executors::cost::wire_bytes_total() - before);
        rows_per_tick.push(rows.len());
        rss.push(sprefa_engine_rs::executors::cost::resident_kb());
    }
    std::env::remove_var("DL_GITHUB_API_BASE");

    assert!(wire[0] > 0, "tick 1 reads the body, got {wire:?}");
    assert_eq!(
        &wire[1..],
        &[0u64; 9],
        "ticks 2..10 must every one be a 304 moving zero bytes, got {wire:?}"
    );
    assert_eq!(
        rows_per_tick,
        vec![1usize; 10],
        "a 304 still answers the same rows from the remembered body"
    );

    // RSS is sampled, so it is bounded rather than pinned: nine 304s must not
    // grow the process the way nine fresh 6.8MB bodies did.
    let low = rss.iter().min().copied().unwrap_or(0);
    let high = rss.iter().max().copied().unwrap_or(0);
    assert!(
        high - low < 8 * 1024,
        "resident set climbed {}KB across ten conditional ticks: {rss:?}",
        high - low
    );
    handle.join().expect("listener thread");
}
