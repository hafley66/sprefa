//! `dl daemon gc` — GC for the `_strings` intern dictionary.
//!
//! Ticks one small program through the real `Engine` (so the interned rows
//! come from the genuine `sprf_sym_intern` write path, not hand-built rows),
//! plants one deliberately orphaned `_strings` row — the exact shape the
//! task describes (a leftover from a since-removed id-derivation scheme,
//! present in `_strings` but referenced by no live rel column) — and asserts
//! `gc::sweep` removes exactly that row.
//!
//! The second test is a MUTATION check: it re-runs the identical sweep with
//! the `seen` rel's `RelDecl` dropped from the reachability set (simulating
//! a bug where column discovery misses a rel) and asserts the live string
//! now gets WRONGLY deleted. A GC test suite that only ever asserts "the
//! planted orphan is gone" would still pass against a sweep that deletes
//! everything; this second test is what proves the first test's green is
//! actually contingent on correct reachability, not incidental.
//!
//! The third test is the fix for the incident where the first version of
//! `gc::sweep` deleted 626 live rows on the real sprefa root: it holds a
//! `_strings` id ONLY in a table that declares `REFERENCES _strings(id)` —
//! present in no `rel_*` table at all, the exact shape `_call_owner.
//! fact_digest_sid` had in production — and asserts the sweep leaves it
//! alone. Every fixture below also plants one such FK-declaring table
//! (`ensure_fk_carrier`) referencing an already-live id, purely so the
//! sweep's "zero FK columns found -> refuse" guard does not fire on a
//! fixture too minimal to otherwise exercise it.
//!
//! The fourth test is the fix for a SECOND incident, found only by an
//! end-to-end run against the real sprefa/smashy/instant roots after the FK
//! fix above landed: `sweep` joined every root into one `UNION ALL` and ran
//! it as a single `INSERT`, which SQLite rejects past
//! `SQLITE_MAX_COMPOUND_SELECT` (default 500) terms. `instant` fit at 437
//! terms; `sprefa` and `smashy` (more declared rels) did not, and the
//! collector was inoperable on the two real dbs whose orphan counts
//! motivated this module. This test builds a fixture with 600
//! programmatically-generated interned rel columns (well past the ceiling)
//! and asserts the sweep still completes with the correct orphan count, then
//! compares that count against an equivalent small (10-column) fixture to
//! prove chunking does not change the result.

use sprefa_v5::ast::{Col, RelDecl, Type};
use sprefa_v5::daemon::gc;
use sprefa_v5::db::{self, Db};
use sprefa_v5::engine::{all_builtin_decls, Engine};
use sprefa_v5::lower;
use sprefa_v5::spine::StringId;
use sprefa_v5::{lex, parse};
use std::fs;
use std::path::{Path, PathBuf};

/// One interned-text rel, populated by two bare fact literals — no scan/
/// match needed, so the only strings that ever enter `_strings` are "alpha"
/// and "beta" (plus the id=0 empty-string sentinel every db carries).
const PROG: &str = r#"
rel seen(name: text).
seen("alpha").
seen("beta").
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("strings_gc_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Tick `PROG` against a fresh db at `db_path`, then drop the `Engine` (and
/// its connection) before returning — a later `db::open` on the same file
/// starts clean, mirroring the real shape: `dl daemon gc` runs in a
/// SEPARATE process/connection from whatever ticked the db.
fn tick_fixture(db_path: &Path, root: &Path) {
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.to_path_buf());
    eng.tick(&prog, true).unwrap();
}

fn seen_decl() -> RelDecl {
    RelDecl {
        name: "seen".to_string(),
        cols: vec![Col::plain("name".to_string(), Type::Text)],
        ..Default::default()
    }
}

/// The full reachability universe `run_for_root` would build: every built-in
/// rel (the engine unconditionally interns a few of these — `repo.root`,
/// `rev.id`/`rev.oid` — even for a program with no scan/match) plus `seen`.
fn full_decls() -> Vec<RelDecl> {
    let mut decls = all_builtin_decls();
    decls.push(seen_decl());
    decls
}

/// Plant one `_strings` row no rel column references — the salted-id
/// leftover shape from the task's incident description.
fn plant_orphan(db: &Db, text: &str) -> i64 {
    let id = StringId::of(text).sqlite();
    db.exec_params(
        "_strings",
        "INSERT OR IGNORE INTO _strings (id, content) VALUES (?1, ?2)",
        &[id.into(), text.into()],
    )
    .unwrap();
    id
}

fn string_row_exists(db: &Db, id: i64) -> bool {
    db.query_one(
        "_strings",
        "SELECT EXISTS(SELECT 1 FROM _strings WHERE id = ?1)",
        &[id.into()],
        |row| Ok(row.get::<_, i64>(0)? != 0),
    )
    .unwrap()
}

/// Create a table declaring `REFERENCES _strings(id)` on `col` (mirroring
/// the real shape of `_call_owner`/`_call_def`/etc., `src/storage/call.rs`)
/// and insert one row holding `text`'s id. Every fixture in this file plants
/// at least one such table (see `sweep_removes_only_true_orphans`'s and
/// `broken_reachability_wrongly_deletes_a_live_string`'s calls with an
/// already-live text) purely to satisfy `gc::sweep`'s "zero FK columns
/// found" refusal — a fixture with NO FK-declaring table anywhere is exactly
/// the schema shape the refusal exists to catch, so tests that are not
/// specifically testing that refusal must not present it. Returns the
/// planted id.
fn ensure_fk_carrier(db: &Db, table: &str, text: &str) -> i64 {
    db.execute_batch_on(
        table,
        &format!(
            "CREATE TABLE IF NOT EXISTS \"{table}\" (sid INTEGER NOT NULL REFERENCES _strings(id))"
        ),
    )
    .unwrap();
    // `plant_orphan`'s insert (idempotent via OR IGNORE): the referenced
    // `_strings` row must exist before the FK row does, whether or not the
    // engine already interned `text` some other way.
    let id = plant_orphan(db, text);
    db.exec_params(
        table,
        &format!("INSERT INTO \"{table}\" (sid) VALUES (?1)"),
        &[id.into()],
    )
    .unwrap();
    id
}

#[test]
fn sweep_removes_only_true_orphans() {
    let dir = sandbox("live");
    let db_path = dir.join("db.sqlite");
    tick_fixture(&db_path, &dir);

    let db = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let alpha_id = StringId::of("alpha").sqlite();
    let beta_id = StringId::of("beta").sqlite();
    assert!(
        string_row_exists(&db, alpha_id),
        "tick should have interned 'alpha' via rel_seen.name"
    );
    assert!(
        string_row_exists(&db, beta_id),
        "tick should have interned 'beta' via rel_seen.name"
    );
    ensure_fk_carrier(&db, "_fixture_call_like", "fk-carrier-anchor-string");

    let orphan_id = plant_orphan(&db, "salted-leftover-orphan");
    assert!(
        string_row_exists(&db, orphan_id),
        "the planted orphan must be present before the sweep"
    );

    let decls = full_decls();

    // Dry run: report the orphan, delete nothing.
    let dry = gc::sweep(&db, &decls, false).unwrap();
    assert!(!dry.applied, "dry run must not apply: {dry:?}");
    assert_eq!(
        dry.orphans, 1,
        "dry run should see exactly the planted orphan: {dry:?}"
    );
    assert!(
        string_row_exists(&db, orphan_id),
        "dry run must not delete anything"
    );
    assert!(string_row_exists(&db, alpha_id));
    assert!(string_row_exists(&db, beta_id));

    // Apply: exactly the orphan goes, both live strings survive.
    let applied = gc::sweep(&db, &decls, true).unwrap();
    assert!(
        applied.applied,
        "apply run must report applied=true: {applied:?}"
    );
    assert_eq!(
        applied.orphans, 1,
        "apply should have deleted exactly one row: {applied:?}"
    );
    assert!(
        !string_row_exists(&db, orphan_id),
        "the true orphan must be gone after --apply"
    );
    assert!(
        string_row_exists(&db, alpha_id),
        "'alpha' is referenced by rel_seen.name and must survive"
    );
    assert!(
        string_row_exists(&db, beta_id),
        "'beta' is referenced by rel_seen.name and must survive"
    );

    // Idempotent: a second sweep over the now-clean db finds nothing to do.
    let clean = gc::sweep(&db, &decls, true).unwrap();
    assert_eq!(
        clean.orphans, 0,
        "a second sweep should find no orphans left: {clean:?}"
    );
}

/// Mutation check: with the `seen` decl dropped from the reachability set
/// (simulating a column-discovery bug that skips a rel), the sweep must
/// wrongly delete 'alpha' — a value a LIVE rel_seen.name row still holds. If
/// this assertion fails (the string survives despite the broken decl set),
/// `gc::sweep` is not actually depending on its `decls` argument for
/// reachability, and `sweep_removes_only_true_orphans` above would pass
/// even against a sweep that always deletes everything or nothing.
#[test]
fn broken_reachability_wrongly_deletes_a_live_string() {
    let dir = sandbox("mutation");
    let db_path = dir.join("db.sqlite");
    tick_fixture(&db_path, &dir);

    let db = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let alpha_id = StringId::of("alpha").sqlite();
    assert!(
        string_row_exists(&db, alpha_id),
        "tick should have interned 'alpha' via rel_seen.name"
    );
    ensure_fk_carrier(&db, "_fixture_call_like", "fk-carrier-anchor-string");

    // The mutation: every built-in decl is present (so the fixed core-spine
    // strings like `repo.root` still survive, isolating the effect to
    // `seen`), but `seen`'s own RelDecl is absent — as if the discovery loop
    // that walks `prepare_paths(...).0.items` skipped it.
    let broken_decls: Vec<RelDecl> = all_builtin_decls();
    let broken = gc::sweep(&db, &broken_decls, true).unwrap();
    assert!(
        broken.applied,
        "the broken sweep should still report applied=true: {broken:?}"
    );
    assert!(
        !string_row_exists(&db, alpha_id),
        "mutation check failed to trigger: 'alpha' survived a sweep run with \
         its only owning rel's decl removed from the reachability set. That \
         means `gc::sweep` is not deriving reachability from `decls` the way \
         this test suite assumes, so `sweep_removes_only_true_orphans` is not \
         actually exercising the reachability logic it claims to cover."
    );
}

/// The incident this arc exists to fix: a `_strings` id held ONLY by a table
/// that declares `REFERENCES _strings(id)` — no `rel_*` table anywhere
/// carries it — is exactly the shape `_call_owner.fact_digest_sid` had on
/// the live sprefa root (626 rows deleted by the pre-fix sweep). The id
/// planted here is deliberately absent from `rel_seen`, so the ONLY thing
/// that can keep it alive is `gc::sweep`'s schema-derived FK discovery
/// (`fk_roots`, `pragma_foreign_key_list`), not the decl-driven path the
/// other two tests exercise.
#[test]
fn fk_declared_root_survives_the_sweep() {
    let dir = sandbox("fk_root");
    let db_path = dir.join("db.sqlite");
    tick_fixture(&db_path, &dir);

    let db = db::open(Some(db_path.to_str().unwrap())).unwrap();
    // The generic carrier every fixture plants, referencing an already-live
    // id (satisfies the "some FK column exists" refusal guard).
    ensure_fk_carrier(&db, "_fixture_call_like", "fk-carrier-anchor-string");
    // The actual case under test: an id that is NOT in rel_seen, NOT in
    // _where_bytes/_embeddings/_node_embeddings — reachable ONLY through a
    // declared FK on a second, distinct table.
    let fk_only_id = ensure_fk_carrier(&db, "_fixture_call_owner_like", "fk-only-live-string");
    assert!(
        string_row_exists(&db, fk_only_id),
        "the FK-referenced string must be present before the sweep"
    );

    let decls = full_decls();
    let applied = gc::sweep(&db, &decls, true).unwrap();
    assert!(
        applied.fk_columns_scanned >= 2,
        "expected at least the two planted FK columns: {applied:?}"
    );
    assert!(
        string_row_exists(&db, fk_only_id),
        "a string referenced only by a table declaring REFERENCES _strings(id) \
         must survive the sweep — this is the exact shape of the incident \
         where the pre-fix sweep deleted 626 live `_call_owner.fact_digest_sid` \
         rows on the real sprefa root"
    );
}

/// Build a fixture with `n` programmatically-generated interned rels
/// (`stress_0..stress_{n-1}`, each one text column `val` holding a distinct
/// live string `stress-live-{i}`), plus the standard FK carrier and one
/// planted orphan unique to `tag`. Returns the open db, the combined decl
/// set (`full_decls()` plus the `n` stress decls), and the orphan's id.
fn build_stress_fixture(tag: &str, n: usize) -> (Db, Vec<RelDecl>, i64) {
    let dir = sandbox(tag);
    let db_path = dir.join("db.sqlite");
    tick_fixture(&db_path, &dir);

    let db = db::open(Some(db_path.to_str().unwrap())).unwrap();
    ensure_fk_carrier(&db, "_fixture_call_like", "fk-carrier-anchor-string");

    let mut decls = full_decls();
    let mut batch = String::new();
    for i in 0..n {
        let name = format!("stress_{i}");
        let table = lower::tbl(&name);
        let text = format!("stress-live-{i}");
        let id = StringId::of(&text).sqlite();
        batch.push_str(&format!(
            "CREATE TABLE IF NOT EXISTS \"{table}\" (val INTEGER, __src TEXT DEFAULT '');\n\
             INSERT OR IGNORE INTO _strings (id, content) VALUES ({id}, '{text}');\n\
             INSERT INTO \"{table}\" (val) VALUES ({id});\n"
        ));
        decls.push(RelDecl {
            name,
            cols: vec![Col::plain("val".to_string(), Type::Text)],
            ..Default::default()
        });
    }
    db.execute_batch_on("_stress_fixture", &batch).unwrap();

    let orphan_id = plant_orphan(&db, &format!("salted-leftover-orphan-{tag}"));
    (db, decls, orphan_id)
}

/// The second incident's fix: a fixture with 600 interned root columns (well
/// past `SQLITE_MAX_COMPOUND_SELECT`'s default 500) must still sweep
/// correctly, and the resulting orphan count must be identical to an
/// equivalent 10-root fixture — chunking is purely a statement-shape change.
#[test]
fn chunked_inserts_scan_every_root_past_the_compound_select_ceiling() {
    let (small_db, small_decls, small_orphan) = build_stress_fixture("chunk_small", 10);
    let small_report = gc::sweep(&small_db, &small_decls, false).unwrap();
    assert_eq!(
        small_report.orphans, 1,
        "small fixture: exactly the one planted orphan: {small_report:?}"
    );

    let (large_db, large_decls, large_orphan) = build_stress_fixture("chunk_large", 600);
    let large_report = gc::sweep(&large_db, &large_decls, false).unwrap();
    assert!(
        large_report.decl_columns_scanned > 500,
        "fixture must actually exceed SQLITE_MAX_COMPOUND_SELECT's default 500 to exercise chunking: {large_report:?}"
    );
    assert_eq!(
        large_report.orphans, 1,
        "large fixture: exactly the one planted orphan: {large_report:?}"
    );

    assert_eq!(
        small_report.orphans, large_report.orphans,
        "chunking must not change the orphan count: small={small_report:?} large={large_report:?}"
    );

    // End-to-end past the ceiling under --apply too, not just a dry-run count:
    // the true orphan goes, a sample of the 600 live stress strings survives.
    let applied = gc::sweep(&large_db, &large_decls, true).unwrap();
    assert_eq!(
        applied.orphans, 1,
        "apply on the large fixture should delete exactly one row: {applied:?}"
    );
    assert!(
        !string_row_exists(&large_db, large_orphan),
        "the large fixture's orphan must be gone after --apply"
    );
    assert!(
        string_row_exists(&large_db, StringId::of("stress-live-599").sqlite()),
        "a live stress string must survive --apply past the compound-select ceiling"
    );

    // Small fixture untouched by the above (separate db); confirm its orphan
    // is still there since this branch never applied against it.
    assert!(string_row_exists(&small_db, small_orphan));
}
