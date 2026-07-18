//! `enumerate_with_hash`'s WORK-arm fast path skips re-hashing a file whose
//! mtime+size still match the previously stored row. mtime here is
//! whole-second resolution, so an edit landing in the same filesystem-
//! timestamp second as the write that produced the cached row — same byte
//! length too — reads as unchanged and its new content is never seen. This is
//! the exact racy-write hazard git's index and watchman's cookie both guard
//! against: `_file_walk.ref_secs` (see `Engine::load_walk_ref_secs` /
//! `save_walk_ref_secs`, wired through `enumerate_with_hash`'s
//! `walk_ref_secs` parameter in `src/engine/scan.rs`) disqualifies a cached
//! row from the fast path when its mtime lands in or after the tick that
//! produced it.

use sprefa_v5::{ast::Program, db, engine::Engine, lex, parse};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("racy_mtime_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
"#;

fn file_hash(eng: &Engine, path: &str) -> String {
    eng.query_sql(
        "SELECT hash FROM _file WHERE path = ?1 AND rev = 'WORK'",
        &[serde_json::Value::String(path.to_string())],
    )
    .unwrap()
    .into_iter()
    .next()
    .unwrap_or_else(|| panic!("no _file row for {path}"))[0]
        .as_str()
        .unwrap()
        .to_string()
}

fn floor_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// Force `path`'s mtime to a freshly-read "now" and tick, retrying (a fresh
/// "now" each attempt) until the ENTIRE (set-mtime, tick) step is observably
/// bracketed within one wall-clock second — the deterministic precondition
/// for `enumerate_with_hash`'s own walk reference (captured inside that same
/// tick, persisted to `_file_walk.ref_secs`) to land in that identical
/// second as the file's mtime. Without this, the hazard test below is at the
/// mercy of scheduling: under enough parallel load, `fs::write` and the
/// following `eng.tick()` could straddle a real second boundary, which
/// closes the racy window through entirely legitimate means (see the guard's
/// doc comment in `src/engine/scan.rs`) rather than the one this test means
/// to exercise. Bounded retries so a genuinely stuck environment fails
/// loudly instead of hanging.
fn tick_pinned_to_this_second(eng: &mut Engine, prog: &Program, path: &Path) {
    for attempt in 0..50 {
        let now = SystemTime::now();
        File::open(path).unwrap().set_modified(now).unwrap();
        eng.tick(prog, true).unwrap();
        if floor_secs(SystemTime::now()) == floor_secs(now) {
            return;
        }
        let _ = attempt;
    }
    panic!("could not land a tick within the same wall-clock second as its forced mtime after 50 attempts");
}

/// Same byte length, different content — the mtime+size fast path's blind spot.
const CONTENT_A: &str = "pub struct Alpha {}\n";
const CONTENT_B: &str = "pub struct Bbbbb {}\n";

#[test]
fn same_second_same_length_edit_is_rehashed_not_hidden_by_the_fast_path() {
    assert_eq!(CONTENT_A.len(), CONTENT_B.len(), "test fixture must hold size fixed");

    let d = sandbox("hazard");
    let path = d.join("src/a.rs");
    fs::write(&path, CONTENT_A).unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    tick_pinned_to_this_second(&mut eng, &prog, &path);
    let hash_a = file_hash(&eng, "src/a.rs");

    // Rewrite with new content of the SAME length and force the mtime back
    // to the exact value the pinned tick just stored — the racy collision,
    // reproduced deterministically rather than hoping two writes land inside
    // one real wall-clock second.
    let observed_mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, CONTENT_B).unwrap();
    File::open(&path).unwrap().set_modified(observed_mtime).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), CONTENT_A.len() as u64);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), observed_mtime);

    eng.tick(&prog, true).unwrap();
    let hash_b = file_hash(&eng, "src/a.rs");

    assert_ne!(
        hash_a, hash_b,
        "a same-mtime same-size rewrite must still move the stored hash \
         (the racy-window guard should have forced a rehash)"
    );
}

/// Companion to the hazard test: the guard must disqualify only rows in the
/// racy window, not every cached row. A file whose mtime is comfortably
/// before any walk it's seen by never enters the racy window, so its stored
/// hash must stay stable (fast-path-served, not merely re-derived to the same
/// value) across any number of warm ticks — including two ticks fired back to
/// back, the flake shape this guard exists to close off.
#[test]
fn unchanged_file_outside_the_racy_window_stays_stable_across_ticks() {
    let d = sandbox("quiet");
    let path = d.join("src/a.rs");
    fs::write(&path, CONTENT_A).unwrap();
    // Comfortably outside any racy window before the very first walk, so
    // every subsequent warm tick takes the fast path regardless of how close
    // together (even same-second) the ticks themselves land.
    let past = SystemTime::now() - Duration::from_secs(120);
    File::open(&path).unwrap().set_modified(past).unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    eng.tick(&prog, true).unwrap();
    let hash0 = file_hash(&eng, "src/a.rs");
    eng.tick(&prog, true).unwrap();
    let hash1 = file_hash(&eng, "src/a.rs");
    eng.tick(&prog, true).unwrap();
    let hash2 = file_hash(&eng, "src/a.rs");

    assert_eq!(hash0, hash1, "an unchanged file's stored hash must not move tick 1 -> 2");
    assert_eq!(hash1, hash2, "an unchanged file's stored hash must not move tick 2 -> 3");
}
