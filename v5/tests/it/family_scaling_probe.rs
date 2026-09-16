//! Corpus-scaling profiling harness for the call-family reactive path
//! (R4/R5 of the retraction-perf arc): does a ONE-FILE-EDIT tick's
//! per-family cost track the CHANGED file count or the TOTAL corpus file
//! count?
//!
//! Deterministic synthetic Rust corpora (fixed content per index, no
//! randomness) are generated at 10/100/1000 files. Each size is measured
//! `WARMUP + REPEATS` times (a fresh temp dir + fresh `Engine` per
//! iteration, so nothing carries state across repeats); the first `WARMUP`
//! iterations are discarded and only `REPEATS` are used for the reported
//! min/mean/max. Every iteration: cold tick, edit exactly `src/file_0000.rs`
//! (retarget its one call site), tick again, then read
//! `Engine::call_router_last_timings()` (`src/engine/family/router.rs`) — the
//! per-family `(name, derive_ns, scan_ns, reconcile_ns, rows_out,
//! cache_hits)` breakdown `FamilyRouter::react_deltas` records on every flip.
//!
//! `Ctx::scan` (`src/engine/family/mod.rs`) issues an unconditional
//! `SELECT {cols} FROM {rel}` on a cache miss — every affected family
//! re-reads its ENTIRE owned table on every tick, not just the touched rows.
//! `scan_ns` at N=1000 vs N=10 is the direct evidence for that: it tracks
//! total file count, not the single file the edit touched. Machine-readable
//! (one JSON object per size, per pass) via `--nocapture`:
//!
//!   cargo test --release --test it family_scaling_probe -- --ignored --nocapture
//!
//! `family_edge_rev_scans_are_served_from_the_same_flip_cache` is NOT
//! ignored — it is the exact (non-flaky) regression guard for the one fix
//! this arc landed: a per-`react_deltas`-flip `ScanCache` (`src/engine/family
//! /mod.rs`) that lets `CallEdgeRev` reuse `CallEdge`'s `_call_owner`/
//! `_call_raw_site`/`_call_resolution` reads instead of re-running them.

use serde_json::json;
use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const WARMUP: usize = 1;
const REPEATS: usize = 3;

const PROGRAM: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/*.rs", path, rev), match(path, rev, /pub fn/, line).

? call_def(repo, sym, kind, file, line, end).
? call_def_rev(repo, sym, kind, file, line, end, rev).
? call_name(sym, name).
? call_edge(caller, callee, kind).
? call_edge_rev(caller, callee, kind, rev).
? call_site(repo, caller, callee, file, line).
? call_kind(fn_sym, kind).
"#;

fn sandbox(file_count: usize, iteration: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "family_scaling_probe_{file_count}_{iteration}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// One deterministic file: `entry_NNNN` calls `helper_a_NNNN` (or
/// `helper_b_NNNN` once `edited`), matching `bench/reactivity/generate_
/// fixture.py`'s helper_a/helper_b/entry shape. Content is a pure function
/// of `(index, edited)` — no RNG, no filesystem sampling.
fn file_source(index: usize, edited: bool) -> String {
    let target = if edited { "helper_b" } else { "helper_a" };
    format!(
        "pub fn helper_a_{index:04}(value: i64) -> i64 {{ value + {index} }}\n\
         pub fn helper_b_{index:04}(value: i64) -> i64 {{ value - {index} }}\n\
         pub fn entry_{index:04}(value: i64) -> i64 {{ {target}_{index:04}(value) }}\n",
    )
}

fn write_all(dir: &Path, file_count: usize) {
    for index in 0..file_count {
        fs::write(
            dir.join("src").join(format!("file_{index:04}.rs")),
            file_source(index, false),
        )
        .unwrap();
    }
}

fn engine_at(dir: &Path, db_name: &str) -> Engine {
    let conn = db::open(Some(dir.join(db_name).to_str().unwrap())).unwrap();
    Engine::new(conn, dir.to_path_buf())
}

/// One iteration's raw measurement: the edit tick's wall time, the router's
/// per-family timing breakdown, and the render-write time
/// (`Storage::retract_rows`/`insert_rows`, `src/storage.rs`) the flip spent
/// writing the 7 public `rel_call_*` tables. `wall_ms - sum(family derive_ns)
/// - write_ms` is whatever is LEFT — extraction (populating `_call_owner`/
/// `_call_raw_site`/`_call_resolution`, `src/engine/extract/call.rs`) plus
/// file read/hash/parse. That remainder is not instrumented by this harness:
/// `extract/call.rs` is outside this arc's file ownership, so no committed
/// measurement of its share exists here.
struct Sample {
    wall_ms: f64,
    timings: Vec<(&'static str, u128, u128, u128, usize, usize)>,
    retract_write_ms: f64,
    insert_write_ms: f64,
}

fn measure_edit_tick(file_count: usize, iteration: usize) -> Sample {
    let dir = sandbox(file_count, iteration);
    write_all(&dir, file_count);
    let mut engine = engine_at(&dir, "db");
    let prog = parse::parse(lex::lex(PROGRAM).unwrap()).unwrap();
    engine.tick(&prog, true).unwrap();
    let _ = sprefa_v5::storage::take_write_ns(); // discard the cold tick's writes

    fs::write(dir.join("src/file_0000.rs"), file_source(0, true)).unwrap();
    let started = Instant::now();
    engine.tick(&prog, true).unwrap();
    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let timings = engine.call_router_last_timings();
    let (retract_ns, insert_ns) = sprefa_v5::storage::take_write_ns();

    let _ = fs::remove_dir_all(&dir);
    Sample {
        wall_ms,
        timings,
        retract_write_ms: retract_ns as f64 / 1e6,
        insert_write_ms: insert_ns as f64 / 1e6,
    }
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn min_max(values: &[f64]) -> (f64, f64) {
    (
        values.iter().cloned().fold(f64::INFINITY, f64::min),
        values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Runs `WARMUP + REPEATS` iterations at `file_count`, discards the warmup
/// samples, and prints one JSON summary line: per-family
/// mean/min/max derive_ms/scan_ms across the measured repeats, plus the edit
/// tick's own wall-clock mean/min/max. `cache_hits` is reported as the
/// (deterministic, non-varying) value from the LAST measured repeat — it
/// does not depend on wall-clock noise, so no spread is needed for it.
fn run_and_report(file_count: usize) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(WARMUP + REPEATS);
    for iteration in 0..(WARMUP + REPEATS) {
        samples.push(measure_edit_tick(file_count, iteration));
    }
    let measured = &samples[WARMUP..];

    let wall_values: Vec<f64> = measured.iter().map(|sample| sample.wall_ms).collect();
    let (wall_min, wall_max) = min_max(&wall_values);

    let mut family_names: Vec<&'static str> =
        measured[0].timings.iter().map(|timing| timing.0).collect();
    family_names.sort_unstable();
    family_names.dedup();

    let mut per_family = Vec::new();
    for name in &family_names {
        let derive_values: Vec<f64> = measured
            .iter()
            .filter_map(|sample| sample.timings.iter().find(|timing| timing.0 == *name))
            .map(|timing| timing.1 as f64 / 1e6)
            .collect();
        let scan_values: Vec<f64> = measured
            .iter()
            .filter_map(|sample| sample.timings.iter().find(|timing| timing.0 == *name))
            .map(|timing| timing.2 as f64 / 1e6)
            .collect();
        let last_cache_hits = measured
            .last()
            .unwrap()
            .timings
            .iter()
            .find(|timing| timing.0 == *name)
            .map(|timing| timing.5);
        let (derive_min, derive_max) = min_max(&derive_values);
        let (scan_min, scan_max) = min_max(&scan_values);
        per_family.push(json!({
            "name": name,
            "derive_ms_mean": mean(&derive_values),
            "derive_ms_min": derive_min,
            "derive_ms_max": derive_max,
            "scan_ms_mean": mean(&scan_values),
            "scan_ms_min": scan_min,
            "scan_ms_max": scan_max,
            "cache_hits": last_cache_hits,
        }));
    }

    let retract_write_values: Vec<f64> = measured
        .iter()
        .map(|sample| sample.retract_write_ms)
        .collect();
    let insert_write_values: Vec<f64> = measured
        .iter()
        .map(|sample| sample.insert_write_ms)
        .collect();
    let family_derive_sum_ms: f64 = per_family
        .iter()
        .map(|entry| entry["derive_ms_mean"].as_f64().unwrap())
        .sum();
    let write_ms_mean = mean(&retract_write_values) + mean(&insert_write_values);
    let unattributed_ms_mean = mean(&wall_values) - family_derive_sum_ms - write_ms_mean;

    println!(
        "{}",
        json!({
            "harness": "family_scaling_probe",
            "fixture_files": file_count,
            "warmup": WARMUP,
            "repeats": REPEATS,
            "edit_tick_wall_ms_mean": mean(&wall_values),
            "edit_tick_wall_ms_min": wall_min,
            "edit_tick_wall_ms_max": wall_max,
            "retract_write_ms_mean": mean(&retract_write_values),
            "insert_write_ms_mean": mean(&insert_write_values),
            "family_derive_sum_ms_mean": family_derive_sum_ms,
            "unattributed_ms_mean": unattributed_ms_mean,
            "unattributed_note": "extraction (src/engine/extract/call.rs) is not instrumented by this harness: outside this arc's file ownership",
            "families": per_family,
        })
    );

    samples
}

#[test]
#[ignore = "corpus-scaling profiling harness: run with `cargo test --release --test it family_scaling_probe -- --ignored --nocapture`"]
fn family_derive_scan_scales_with_corpus_size() {
    for &file_count in &[10usize, 100, 1000] {
        let samples = run_and_report(file_count);
        // The printed JSON is only meaningful if the router actually reacted
        // at every measured repeat; assert that basic shape before trusting
        // the numbers in it.
        assert_eq!(
            samples.len(),
            WARMUP + REPEATS,
            "expected {} total samples (warmup+repeats) at {file_count} files",
            WARMUP + REPEATS
        );
        assert!(
            samples.iter().all(|sample| !sample.timings.is_empty()),
            "call router produced no per-family timings at {file_count} files"
        );
    }
}

/// Non-ignored, exact regression guard for the `ScanCache` fix
/// (`src/engine/family/mod.rs`): `call_edge` and `call_edge_rev` both read
/// `_call_owner`/`_call_raw_site`/`_call_resolution` with byte-identical
/// column lists (`call_edge.rs`, `call_edge_rev.rs`). Registry order is
/// sorted by name, so `call_edge` reruns before `call_edge_rev` in the same
/// `react_deltas` flip and must populate the cache itself (zero hits); the
/// alphabetically-later `call_edge_rev` must serve all three of its scans
/// from that cache. This is a row COUNT, not a wall-clock measurement — it
/// cannot flake on system noise the way a timing threshold would.
#[test]
fn family_edge_rev_scans_are_served_from_the_same_flip_cache() {
    let dir = sandbox(50, 0);
    write_all(&dir, 50);
    let mut engine = engine_at(&dir, "db");
    let prog = parse::parse(lex::lex(PROGRAM).unwrap()).unwrap();
    engine.tick(&prog, true).unwrap();

    // Retargeting file_0000's call site moves `_call_resolution` (a new
    // callee_sid), which is in both `call_edge`'s and `call_edge_rev`'s
    // input_rels footprint, so both rerun this tick.
    fs::write(dir.join("src/file_0000.rs"), file_source(0, true)).unwrap();
    engine.tick(&prog, true).unwrap();

    let timings = engine.call_router_last_timings();
    let call_edge = timings
        .iter()
        .find(|timing| timing.0 == "call_edge")
        .expect("call_edge must rerun when _call_resolution moves");
    let call_edge_rev = timings
        .iter()
        .find(|timing| timing.0 == "call_edge_rev")
        .expect("call_edge_rev must rerun when _call_resolution moves");

    assert_eq!(
        call_edge.5, 0,
        "call_edge is alphabetically first ('call_edge' < 'call_edge_rev'), so it must be the one \
         that populates the shared flip cache, not one that reads from it: {call_edge:?}"
    );
    assert_eq!(
        call_edge_rev.5, 3,
        "call_edge_rev issues the same 3 scans as call_edge (_call_owner, _call_raw_site, \
         _call_resolution, identical columns) and must serve all 3 from the cache call_edge just \
         populated: {call_edge_rev:?}"
    );

    let _ = fs::remove_dir_all(&dir);
}
