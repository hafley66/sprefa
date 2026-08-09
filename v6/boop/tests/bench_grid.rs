//! One perf grid row per `Registry::discover()` harness, sourced from
//! `tests/fixtures/<harness.id()>/sessions.json` (tests/fixtures/CONVENTION.md).
//! Zero harness-id literals and zero `match` on id: adding an adapter to the
//! registry, with a corpus, adds a row here with no change to this file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use boop::harness::{Harness, SessionRef};
use boop::ident::{sync_session, Store};
use boop::registry::Registry;
use peak_alloc::PeakAlloc;
use serde::Deserialize;

#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

/// One `sessions.json` entry; every field maps straight onto `SessionRef`.
#[derive(Deserialize)]
struct FixtureSession {
    session_id: String,
    nickname: String,
    path: String,
    cwd: Option<String>,
    git_branch: Option<String>,
    parent: Option<String>,
}

/// What one harness's corpus run measured.
struct CorpusResult {
    events: u64,
    elapsed_ms: f64,
    db_bytes: u64,
    peak_alloc_bytes: u64,
}

fn fixture_dir(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(id)
}

/// `None` when the harness has no `sessions.json`: the "no corpus" case, not
/// an error.
fn load_manifest(id: &str) -> Option<Vec<FixtureSession>> {
    let manifest_path = fixture_dir(id).join("sessions.json");
    let text = fs::read_to_string(&manifest_path).ok()?;
    Some(serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!("malformed fixture manifest {}: {error}", manifest_path.display())
    }))
}

fn session_ref(id: &'static str, base: &Path, entry: &FixtureSession) -> SessionRef {
    let path = base.join(&entry.path);
    let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    SessionRef {
        harness: id,
        session_id: entry.session_id.clone(),
        nickname: entry.nickname.clone(),
        path,
        cwd: entry.cwd.clone(),
        git_branch: entry.git_branch.clone(),
        modified_ms: 0,
        size,
        tmux: None,
        tmux_socket: None,
        parent: entry.parent.clone(),
    }
}

fn scratch_store_path(id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("boop_bench_grid_{}_{id}.db", std::process::id()))
}

/// Read-count the corpus untimed, then time the real `sync_session` write
/// path once per session against a fresh scratch store (never `~/.agent/boop.db`).
fn measure(harness: &dyn Harness) -> Option<CorpusResult> {
    let manifest = load_manifest(harness.id())?;
    let base = fixture_dir(harness.id());
    let sessions: Vec<SessionRef> = manifest
        .iter()
        .map(|entry| session_ref(harness.id(), &base, entry))
        .collect();

    let mut events = 0u64;
    for session in &sessions {
        let chunk = harness.read_from(session, 0).unwrap_or_else(|error| {
            panic!("{}: read_from {} failed: {error}", harness.id(), session.session_id)
        });
        events += chunk.events.len() as u64;
    }

    let db_path = scratch_store_path(harness.id());
    let _ = fs::remove_file(&db_path);
    let store = Store::open(db_path.clone()).expect("open scratch store");

    PEAK_ALLOC.reset_peak_usage();
    let start = Instant::now();
    for session in &sessions {
        sync_session(&store, harness, session).unwrap_or_else(|error| {
            panic!("{}: sync_session {} failed: {error}", harness.id(), session.session_id)
        });
    }
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let peak_alloc_bytes = PEAK_ALLOC.peak_usage() as u64;

    let db_bytes = store.db_bytes().expect("read db_bytes");
    drop(store);
    let _ = fs::remove_file(&db_path);

    Some(CorpusResult {
        events,
        elapsed_ms,
        db_bytes,
        peak_alloc_bytes,
    })
}

fn format_row(id: &str, result: &Option<CorpusResult>) -> String {
    match result {
        None => format!("| {id} | no corpus | - | - | - | - | - |\n"),
        Some(result) => {
            let events_per_sec = if result.elapsed_ms > 0.0 {
                result.events as f64 / (result.elapsed_ms / 1000.0)
            } else {
                f64::INFINITY
            };
            let bytes_per_event = if result.events > 0 {
                result.db_bytes as f64 / result.events as f64
            } else {
                0.0
            };
            format!(
                "| {id} | {} | {:.1} | {:.0} | {} | {:.1} | {} |\n",
                result.events,
                result.elapsed_ms,
                events_per_sec,
                result.db_bytes,
                bytes_per_event,
                result.peak_alloc_bytes
            )
        }
    }
}

#[test]
fn bench_grid() {
    let registry = Registry::discover();
    let harnesses = registry.all();

    let rows: Vec<(&'static str, Option<CorpusResult>)> = harnesses
        .iter()
        .map(|harness| (harness.id(), measure(harness.as_ref())))
        .collect();

    let mut markdown = String::from(
        "| harness | events | elapsed_ms | events/s | db_bytes | bytes/event | peak_alloc_bytes |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for (id, result) in &rows {
        markdown.push_str(&format_row(id, result));
    }

    println!("{markdown}");
    let target_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    fs::create_dir_all(&target_dir).expect("create target dir");
    fs::write(target_dir.join("bench-grid.md"), &markdown).expect("write bench-grid.md");

    assert_eq!(
        rows.len(),
        harnesses.len(),
        "one grid row per discovered harness"
    );
    for (id, result) in &rows {
        if let Some(result) = result {
            assert!(result.events > 0, "{id}: measured harness ingested zero events");
        }
    }
}

/// A harness id with no `sessions.json` is `no corpus`, never a silent skip
/// and never a hardcoded id check in the bench.
#[test]
fn a_missing_manifest_is_no_corpus_not_an_error() {
    assert!(load_manifest("no-such-harness-id-in-this-repo").is_none());
}
