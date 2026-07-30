//! Extraction verdict/reason unit tests, relocated from `extract/mod.rs`
//! (decomposition plan step 8). Compiled only under `#[cfg(test)]` via the
//! module declaration in `extract/mod.rs`.

use super::*;
use crate::engine::Engine;

fn engine() -> Engine {
    let conn = crate::db::open(None).unwrap();
    let mut eng = Engine::new(conn, std::path::PathBuf::from("/tmp"));
    // `WORK` is an ALIAS resolved at the scan seam; these fixtures skip the
    // scan, so resolve it directly and use the resulting rev everywhere a row
    // or a family call names the working tree.
    eng.resolve_self_rev().unwrap();
    eng
}

/// The resolved working-tree rev these fixtures write and query under.
fn work_rev() -> String {
    crate::engine::RevId::no_head().text()
}

fn extract_file(repo: &str, path: &str, hash: &str) -> ExtractFile {
    (
        repo.to_string(),
        path.to_string(),
        work_rev(),
        hash.to_string(),
    )
}

#[test]
fn resolution_dependency_digests_are_stable_and_isolated() {
    let eng = engine();
    eng.db.execute_batch_on(
        "rel_scip_ref_txt",
        &format!("CREATE TABLE rel_scip_ref_txt (file TEXT, symbol TEXT, def_file TEXT, repo TEXT); \
         CREATE TABLE rel_module_edge_rev_txt (src TEXT, dst TEXT, rev TEXT); \
         CREATE TABLE rel_module_binding_resolved_rev_txt \
             (file TEXT, local TEXT, source TEXT, dst TEXT, rev TEXT); \
         INSERT INTO rel_scip_ref_txt VALUES \
             ('src/a.rs', 'crate A#', 'src/def.rs', 'self'); \
         INSERT INTO rel_module_edge_rev_txt VALUES \
             ('src/a.rs', 'src/def.rs', '{rev}'); \
         INSERT INTO rel_module_binding_resolved_rev_txt VALUES \
             ('src/a.rs', 'Alias', 'Actual', 'src/def.rs', '{rev}');",
            rev = work_rev()),
    ).unwrap();

    let scip_initial = eng.scip_resolution_dependency_digest(&work_rev());
    let module_initial = eng.module_resolution_dependency_digest(&work_rev());
    assert_eq!(
        scip_initial,
        eng.scip_resolution_dependency_digest(&work_rev())
    );
    assert_eq!(
        module_initial,
        eng.module_resolution_dependency_digest(&work_rev())
    );
    let mut expected_extract = *blake3::hash(format!("call\0{}", work_rev()).as_bytes()).as_bytes();
    for dependency in [scip_initial, module_initial] {
        for (slot, byte) in expected_extract.iter_mut().zip(dependency) {
            *slot ^= byte;
        }
    }
    assert_eq!(
        eng.extract_input_digest("call", &work_rev(), &[], true),
        expected_extract,
        "factoring dependency folds changed the extract digest framing",
    );

    eng.db
        .exec_on(
            "rel_module_edge_rev_txt",
            "UPDATE rel_module_edge_rev_txt SET dst = 'src/other.rs'",
        )
        .unwrap();
    let scip_after_module = eng.scip_resolution_dependency_digest(&work_rev());
    let module_after_module = eng.module_resolution_dependency_digest(&work_rev());
    assert_eq!(
        scip_initial, scip_after_module,
        "module input changed SCIP digest"
    );
    assert_ne!(
        module_initial, module_after_module,
        "module row change was invisible"
    );

    eng.db
        .exec_on(
            "rel_scip_ref_txt",
            "UPDATE rel_scip_ref_txt SET def_file = 'src/other.rs'",
        )
        .unwrap();
    assert_ne!(
        scip_after_module,
        eng.scip_resolution_dependency_digest(&work_rev()),
        "SCIP row change was invisible",
    );
    assert_eq!(
        module_after_module,
        eng.module_resolution_dependency_digest(&work_rev()),
        "SCIP input changed module digest",
    );
    assert_eq!(eng.scip_resolution_dependency_digest("HEAD"), [0; 32]);
}

/// First tick for a family/rev (no prior digest row) always attributes
/// to "first-run", regardless of what the corpus or exe identity look
/// like.
#[test]
fn first_run_wins_over_every_other_category() {
    let eng = engine();
    let files = vec![extract_file("self", "src/a.rs", "hash-a")];
    let reason = eng.extract_rebuild_reason("type", &work_rev(), &files, false, true);
    assert_eq!(reason, "first-run");
}

/// A file with an empty content hash (the `extract_input_digest` nonce
/// signal for an unresolved/newly-appeared file) attributes to
/// "rev-set-changed" even when it is not the first run.
#[test]
fn empty_hash_file_attributes_to_rev_set_changed() {
    let eng = engine();
    let files = vec![
        extract_file("self", "src/a.rs", "hash-a"),
        extract_file("self", "src/b.rs", ""),
    ];
    let reason = eng.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(reason, "rev-set-changed");
}

/// A rebuild with no rev-set/exe/scip signal falls to the honest default
/// bucket, naming the file count so a reader can gauge WHICH corpus
/// moved even without a finer category.
#[test]
fn plain_content_change_falls_to_corpus_changed_with_file_count() {
    let eng = engine();
    let files = vec![
        extract_file("self", "src/a.rs", "hash-a"),
        extract_file("self", "src/b.rs", "hash-b"),
    ];
    let reason = eng.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(reason, "corpus-changed (2 paths)");
}

/// A rebuild with `with_scip` false never attributes to
/// "scip-index-changed" (the category is gated on the scip fold being
/// active for this family/rev in the first place).
#[test]
fn scip_category_is_gated_on_with_scip() {
    let eng = engine();
    let files = vec![extract_file("self", "src/a.rs", "hash-a")];
    let reason = eng.extract_rebuild_reason("call", &work_rev(), &files, false, false);
    assert_ne!(reason, "scip-index-changed");
}

/// Control: an all-resolved corpus (every file carries a real content
/// hash) yields the SAME digest on two consecutive ticks, so the warm-tick
/// skip (`prior == Some(d)` in `moved_extract_revs`) fires and the family
/// does no work. This is the behavior the empty-hash case below breaks.
#[test]
fn resolved_corpus_digest_is_stable_across_ticks() {
    let eng = engine();
    let files = vec![
        extract_file("self", "src/a.rs", "hash-a"),
        extract_file("self", "src/b.rs", "hash-b"),
    ];
    let first = eng.extract_input_digest("type", &work_rev(), &files, false);
    let second = eng.extract_input_digest("type", &work_rev(), &files, false);
    assert_eq!(
        first, second,
        "a fully-resolved corpus must digest identically each tick"
    );
}

/// REGRESSION (daemon CPU storm, 2026-07-12): a file with a persistently
/// empty content hash folds a wall-clock nanosecond NONCE into the digest
/// (`extract_input_digest`, the `hash.is_empty()` branch), so two ticks
/// over an UNCHANGED corpus produce DIFFERENT digests. That defeats the
/// warm-tick skip and forces a full re-extract of every file on every tick,
/// forever: the 5-repo daemon pinned ~22% CPU and grew cache.db to 2GB this
/// way. The nonce is meant to flag a NEWLY-appeared file, but nothing
/// distinguishes "new this tick" from "still unresolved N ticks later", so
/// a path that keeps hashing empty re-nonces every tick. The digest must be
/// a pure function of its inputs; a persistently-empty hash must fold a
/// STABLE token, not the clock.
#[test]
fn empty_hash_file_makes_digest_nondeterministic_the_cpu_storm() {
    let eng = engine();
    let files = vec![
        extract_file("self", "src/a.rs", "hash-a"),
        extract_file("self", "src/b.rs", ""), // persistently unresolved path
    ];
    let first = eng.extract_input_digest("type", &work_rev(), &files, false);
    let second = eng.extract_input_digest("type", &work_rev(), &files, false);
    assert_eq!(
        first, second,
        "extract_input_digest folds a wall-clock nonce for an empty-hash file, so an \
         unchanged corpus digests differently each tick and never skips -> full rebuild \
         every tick (the daemon CPU/battery storm)"
    );
}

/// REGRESSION (daemon CPU storm, 2026-07-16): `exe_identity_changed_since_
/// last_run` used to cache its answer in a process-global `static CHANGED:
/// OnceLock<bool>`. That pinned `true` for the whole process after the first
/// stamp mismatch, so a daemon that survived a binary swap rebuilt every root
/// on every tick forever. The cache must be per-Engine and cleared at tick
/// completion: one real binary swap causes exactly one `exe-identity-changed`
/// rebuild cycle per root.
#[test]
fn exe_identity_changed_reports_true_then_false_after_tick_boundary() {
    let eng = engine();
    eng.ensure_meta().unwrap();
    let files = vec![extract_file("self", "src/a.rs", "hash-a")];
    // Pre-save a mismatched digest to simulate that this db last saw a
    // different binary. `exe_stamp()` itself is not mocked; injecting via the
    // persisted key is race-free across parallel tests.
    let bogus = [0xffu8; 32];
    eng.save_rel_digest("extract:exe-stamp", &bogus).unwrap();

    let first = eng.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(first, "exe-identity-changed");
    let second = eng.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(
        second, "exe-identity-changed",
        "within one tick the cached answer must stay consistent across family lookups"
    );

    // Simulate the tick boundary where `tick` clears the cache.
    eng.clear_exe_identity_cache();
    let third = eng.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(
        third, "corpus-changed (1 paths)",
        "after the tick boundary the saved stamp matches the current binary, so no exe change"
    );
}

/// The second half of the global-cache bug: in one process, two Engines on
/// two distinct roots each have their own db and must evaluate the stamp
/// independently. Engine 1's `true` must not poison engine 2.
#[test]
fn exe_identity_changed_is_isolated_across_engines() {
    let eng1 = engine();
    let eng2 = engine();
    eng1.ensure_meta().unwrap();
    eng2.ensure_meta().unwrap();
    let files = vec![extract_file("self", "src/a.rs", "hash-a")];
    let bogus = [0xffu8; 32];
    eng1.save_rel_digest("extract:exe-stamp", &bogus).unwrap();
    eng2.save_rel_digest("extract:exe-stamp", &bogus).unwrap();

    let reason1 = eng1.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(reason1, "exe-identity-changed");

    let reason2 = eng2.extract_rebuild_reason("type", &work_rev(), &files, false, false);
    assert_eq!(
        reason2, "exe-identity-changed",
        "engine 2 must read its own db, not inherit engine 1's cached answer"
    );
}

/// REGRESSION (deterministic rebuilds, 2026-07-17): `cached_facts_profiled`
/// used to append parsed misses after cache hits, so a warm run (all hits)
/// emitted facts in input order while a cold run (all misses) emitted them
/// in parallel scheduling order. That order leaked into downstream dedup
/// (e.g. df_node ids scoped only by file:line:col), making full rebuilds
/// non-deterministic. Output must follow `files` order regardless of hit/miss.
#[test]
fn cached_facts_profiled_order_is_independent_of_cache_hit_ratio() {
    let cache: FactCache<String> = std::cell::RefCell::new(HashMap::new());
    let parsed = std::cell::Cell::new(0);
    let files = vec![
        extract_file("repo-b", "src/a.rs", "hash-a"),
        extract_file("repo-a", "src/b.rs", "hash-b"),
        extract_file("repo-a", "src/a.rs", "hash-c"),
    ];
    fn parse(repo: &str, path: &str, _rev: &str) -> Option<(String, String)> {
        Some((format!("rid-{repo}"), format!("facts-{path}")))
    }

    let cold = cached_facts_profiled(&cache, &files, &parsed, "test", parse);
    assert_eq!(
        parsed.get(),
        files.len(),
        "cold call should parse every file"
    );
    let cold_order: Vec<_> = cold
        .iter()
        .map(|(rid, path, _rev, _facts)| (rid.clone(), path.clone()))
        .collect();

    parsed.set(0);
    let warm = cached_facts_profiled(&cache, &files, &parsed, "test", parse);
    assert_eq!(parsed.get(), 0, "warm call should parse no files");
    let warm_order: Vec<_> = warm
        .iter()
        .map(|(rid, path, _rev, _facts)| (rid.clone(), path.clone()))
        .collect();

    assert_eq!(
        cold_order, warm_order,
        "cold and warm output order must be identical"
    );
    assert_eq!(
        cold_order,
        vec![
            ("rid-repo-b".to_string(), "src/a.rs".to_string()),
            ("rid-repo-a".to_string(), "src/b.rs".to_string()),
            ("rid-repo-a".to_string(), "src/a.rs".to_string()),
        ],
        "output must follow input order"
    );
}
