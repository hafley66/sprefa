//! Storage diet, Direction 5 (2026-07-18, plans/2026-07-18-storage-diet.md,
//! step 2 — index audit): `create_auto_indexes` is now a demand-aware
//! reconcile, not a monotonic-add-only `CREATE INDEX IF NOT EXISTS` loop. On
//! the sprefa root db, `CREATE INDEX IF NOT EXISTS` alone let every `.dl`
//! program ever served leave its join-key indexes behind forever — the
//! measured incident db carried 755 auto-created `idx_<rel>_<col>` indexes
//! (309.8MB, 34% of a 921.9MB db) built up across every program run against
//! that root over its history, not just the currently-discovered set.
//!
//! Direction 5 residual (this arc): the reconcile's DEMAND side is now
//! planner-honest. Raw join-key co-occurrence over-demanded — 771 idx_
//! indexes (357.6MB) still stood on the live root under the reconcile alone.
//! Three filters trim demand to what SQLite's planner actually needs (each
//! backed by EQP + timing receipts on the live-root snapshot):
//!   - PK-prefix on a rowid table: a candidate column that leads the rel's
//!     PRIMARY KEY is never demanded (sqlite_autoindex already provides the
//!     seek) — `covered_prefix_join_key_is_never_demanded`;
//!   - tiny rel: under `TINY_REL_FLOOR` (1024) rows a scan beats a B-tree —
//!     `tiny_rel_join_key_is_dropped_once_measured`;
//!   - constant column: exactly one distinct value cannot discriminate —
//!     `constant_join_key_is_dropped_once_measured`, which also carries the
//!     determinism gate (dropping an index never changes rows). Anything
//!     with >= 2 distinct values stays (`two_value_join_key_stays_demanded`:
//!     value skew defeated the broader distinct-count filter in timing).
//!
//! The original reconcile pins still hold, on a fixture large and selective
//! enough to clear the new demand filters:
//!   - a join-key index gets created when a rule needs it
//!     (`join_key_index_is_created_when_a_rule_needs_it`);
//!   - the SAME index gets dropped on the very next tick of a DIFFERENT
//!     program that no longer joins that column
//!     (`stale_join_key_index_is_pruned_when_no_longer_needed`);
//!   - the index comes back the next time a program needs it again
//!     (`pruned_index_is_recreated_when_needed_again`);
//!   - a genuine join key survives the stats probe on a re-run of the SAME
//!     program (`genuine_join_key_stays_demanded_after_probe`);
//!   - the sweep only ever touches the `idx_<rel>_<col>` naming convention
//!     (`hand_authored_index_survives_the_sweep`);
//!   - pruning reclaims real bytes (`pruning_reclaims_measurable_index_bytes`).
//!
//! Reverse-traversal residual (2026-07-20): raw co-occurrence never proposes
//! the REVERSE column of an edge relation read by a closure or a
//! self-referential recursive rule (`head(seed, to) <- head(seed, from),
//! edge(from, to).` binds `to` exactly once per rule body) — measured on the
//! live root's `rel_map_edge` at ~4,000x for a reverse walk with no index
//! (`tests/it/reverse_traversal_plan.rs` pins the query-plan evidence
//! deterministically; the ratio itself is reported there as advisory, not
//! asserted, per the noise caveat in that file's header).
//!   - a self-referential recursive rule's edge atom gets its reverse
//!     column indexed (`reverse_column_of_a_recursive_walk_is_indexed`);
//!   - a PK-prefix duplicate is swept even if it predates the policy —
//!     i.e. was hand-inserted, standing in for a legacy index —
//!     confirming the sweep has no "grandfather" exception
//!     (`stale_pk_prefix_duplicate_index_is_swept_even_when_hand_inserted`).

use rusqlite::Connection;
use sprefa_v5::spine::StringId;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
// Hermetic: an ad-hoc `dl` run otherwise ingests `~/.config/sprefa/config.toml`
// (the ambient-config friction item in CLAUDE.md) and scans real repos.
const HERMETIC_CONFIG: &str = "/nonexistent/sprefa-hermetic.toml";

/// One above `TINY_REL_FLOOR` (src/engine/declare.rs): fixtures that must
/// KEEP their join-key index need at least this many extracted rows, or the
/// tiny-rel demand filter (correctly) drops it.
const BIG_FIXTURE_ROWS: usize = 1100;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("storage_diet_index_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// A source file with `n` distinct struct declarations: enough rows to clear
/// the tiny-rel floor, and a distinct-per-row `name` column that clears the
/// low-selectivity cutoff.
fn struct_src(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!("struct Struct{i:05};\n"));
    }
    src
}

/// A chain of `n` distinct edges `nNNNNN -> nNNNNN+1`: `n` rows, `n` distinct
/// `from` values, `n` distinct `to` values (all disjoint from each other),
/// enough to clear both the tiny-rel floor and the low-selectivity cutoff on
/// EITHER column. Trailing `seed n00000` line feeds the walk's base case.
fn edge_chain_src(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!("edge n{i:05} n{:05}\n", i + 1));
    }
    src.push_str("seed n00000\n");
    src
}

fn run(dir: &Path, db: &Path, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", db.to_str().unwrap()])
        .env("SPREFA_CONFIG", HERMETIC_CONFIG)
        .current_dir(dir)
        .output()
        .expect("run dl");
    assert!(
        out.status.success(),
        "dl failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn index_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1")
        .unwrap();
    stmt.query_map([table], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

// Two source rels joined on `name` (a Rust struct name shared by two match
// rules over the same files), plus a derived rel that performs the join —
// the classic auto_indexes() shape: the shared variable `name` occupies
// position 1 in both `symbol` and `other` body atoms of the `joined` rule,
// so both get an `idx_<rel>_name` index. `name` is deliberately NOT the
// first declared column: the default PK is the full row in declared order,
// and a PK-prefix column is (correctly) never demanded anymore.
const WITH_JOIN: &str = r#"
rel symbol(path: file, name: text).
symbol(path, name) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel other(path: file, name: text).
other(path, name) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel joined(name: text, path1: file, path2: file).
joined(name, path1, path2) <- symbol(path1, name), other(path2, name).
? joined(name, path1, path2).
"#;

// Same two source rels, no join: `name` occupies only one atom position per
// rule body, so auto_indexes() no longer wants idx_symbol_name/idx_other_name.
const WITHOUT_JOIN: &str = r#"
rel symbol(path: file, name: text).
symbol(path, name) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel other(path: file, name: text).
other(path, name) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
? symbol(path, name).
"#;

#[test]
fn join_key_index_is_created_when_a_rule_needs_it() {
    let d = sandbox("create");
    fs::write(d.join("src/a.rs"), struct_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);

    let conn = Connection::open(&db).unwrap();
    let symbol_idx = index_names(&conn, "rel_symbol");
    let other_idx = index_names(&conn, "rel_other");
    assert!(
        symbol_idx.iter().any(|n| n == "idx_symbol_name"),
        "expected idx_symbol_name, got {symbol_idx:?}"
    );
    assert!(
        other_idx.iter().any(|n| n == "idx_other_name"),
        "expected idx_other_name, got {other_idx:?}"
    );
}

#[test]
fn stale_join_key_index_is_pruned_when_no_longer_needed() {
    let d = sandbox("prune");
    fs::write(d.join("src/a.rs"), struct_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);
    {
        let conn = Connection::open(&db).unwrap();
        assert!(index_names(&conn, "rel_symbol")
            .iter()
            .any(|n| n == "idx_symbol_name"));
        assert!(index_names(&conn, "rel_other")
            .iter()
            .any(|n| n == "idx_other_name"));
    }

    // A different program served against the SAME db: the join is gone, so
    // neither rel needs its idx_<rel>_name anymore. Pre-fix (CREATE INDEX IF
    // NOT EXISTS with no reconcile), both indexes survive forever — this is
    // the exact accumulation shape that grew the incident db to 755 indexes.
    run(&d, &db, WITHOUT_JOIN);

    let conn = Connection::open(&db).unwrap();
    let symbol_idx = index_names(&conn, "rel_symbol");
    let other_idx = index_names(&conn, "rel_other");
    assert!(
        !symbol_idx.iter().any(|n| n == "idx_symbol_name"),
        "stale index must be pruned once no rule joins on it: {symbol_idx:?}"
    );
    assert!(
        !other_idx.iter().any(|n| n == "idx_other_name"),
        "stale index must be pruned once no rule joins on it: {other_idx:?}"
    );
}

#[test]
fn pruned_index_is_recreated_when_needed_again() {
    let d = sandbox("recreate");
    fs::write(d.join("src/a.rs"), struct_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);
    run(&d, &db, WITHOUT_JOIN); // prunes idx_symbol_name / idx_other_name
    run(&d, &db, WITH_JOIN); // the join comes back

    // The third run probes real stats (the rel has a digest now): the fixture
    // clears the tiny-rel floor and `name` is distinct per row, so the index
    // is re-demanded — the reconcile is non-destructive, not a one-way ratchet.
    let conn = Connection::open(&db).unwrap();
    assert!(index_names(&conn, "rel_symbol")
        .iter()
        .any(|n| n == "idx_symbol_name"));
    assert!(index_names(&conn, "rel_other")
        .iter()
        .any(|n| n == "idx_other_name"));
}

#[test]
fn genuine_join_key_stays_demanded_after_probe() {
    let d = sandbox("genuine");
    fs::write(d.join("src/a.rs"), struct_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN); // cold: index created before extraction fills rows
    run(&d, &db, WITH_JOIN); // warm: stats probe runs against the filled rel

    // 1100 rows, 1100 distinct names: above the floor, above the selectivity
    // cutoff — the planner genuinely needs this index and demand keeps it.
    let conn = Connection::open(&db).unwrap();
    assert!(index_names(&conn, "rel_symbol")
        .iter()
        .any(|n| n == "idx_symbol_name"));
    assert!(index_names(&conn, "rel_other")
        .iter()
        .any(|n| n == "idx_other_name"));
}

// The join variable sits at position 0 of both body atoms — the FIRST column
// of each rel's (default, full-row) PRIMARY KEY. The PK B-tree already serves
// an equality seek on its leading column, so demanding idx_<rel>_name here
// duplicates 137.9MB worth of access paths on the measured root db.
// Fail-pre-fix: the raw co-occurrence heuristic created both indexes.
const COVERED_PREFIX_JOIN: &str = r#"
rel first_key(name: text, path: file).
first_key(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel first_key_twin(name: text, path: file).
first_key_twin(name, path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /struct (?<name>\w+)/, line).
rel first_join(name: text, path1: file, path2: file).
first_join(name, path1, path2) <- first_key(name, path1), first_key_twin(name, path2).
? first_join(name, path1, path2).
"#;

#[test]
fn covered_prefix_join_key_is_never_demanded() {
    let d = sandbox("covered");
    fs::write(d.join("src/a.rs"), struct_src(8)).unwrap();
    let db = d.join("db");
    run(&d, &db, COVERED_PREFIX_JOIN);

    let conn = Connection::open(&db).unwrap();
    let first_key_idx = index_names(&conn, "rel_first_key");
    let twin_idx = index_names(&conn, "rel_first_key_twin");
    assert!(
        !first_key_idx.iter().any(|n| n == "idx_first_key_name"),
        "a PK-prefix column must never be demanded (the PK B-tree covers the seek): {first_key_idx:?}"
    );
    assert!(
        !twin_idx.iter().any(|n| n == "idx_first_key_twin_name"),
        "a PK-prefix column must never be demanded (the PK B-tree covers the seek): {twin_idx:?}"
    );
}

/// A data file whose `kind` column carries exactly ONE value across
/// `BIG_FIXTURE_ROWS` lines while `tag` is distinct per line.
fn constant_column_src(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!("item kind_a tag_{i:05}\n"));
    }
    src.push_str("label kind_a alpha\n");
    src
}

// `kind` is the join variable, at a non-PK-prefix position of `item`, over a
// rel that clears the tiny floor — but it is CONSTANT (one distinct value
// across 1100 rows). An equality seek on it matches the whole table, so the
// B-tree cannot discriminate and demand drops it (the measured root carried
// idx_df_node_repo_rev_repo, 10.2MB over a repo column with one value). The
// kind_label side's `kind` is its PK prefix, so only rel_item grows a
// candidate here.
const CONSTANT_COLUMN_JOIN: &str = r#"
rel item(path: file, kind: text, tag: text).
item(path, kind, tag) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /item (?<kind>kind_[ab]) (?<tag>tag_\w+)/, line).
rel kind_label(kind: text, label: text).
kind_label(kind, label) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /label (?<kind>kind_[ab]) (?<label>\w+)/, line).
rel labeled(tag: text, label: text).
labeled(tag, label) <- item(path, kind, tag), kind_label(kind, label).
? labeled(tag, label).
"#;

#[test]
fn constant_join_key_is_dropped_once_measured() {
    let d = sandbox("constcol");
    fs::write(d.join("src/data.rs"), constant_column_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");

    // Cold tick: no digest yet, so demand stays (the index is created empty,
    // before extraction, and protects the first fixpoint).
    let out_with_index = run(&d, &db, CONSTANT_COLUMN_JOIN);
    {
        let conn = Connection::open(&db).unwrap();
        let item_idx = index_names(&conn, "rel_item");
        assert!(
            item_idx.iter().any(|n| n == "idx_item_kind"),
            "cold tick keeps demand (no stats yet): {item_idx:?}"
        );
    }

    // Warm tick of the SAME program: the stats probe sees 1100 rows / 1
    // distinct kind and drops demand; the reconcile sweeps the index.
    // Fail-pre-fix: the index survived every re-run.
    let out_without_index = run(&d, &db, CONSTANT_COLUMN_JOIN);
    let conn = Connection::open(&db).unwrap();
    let item_idx = index_names(&conn, "rel_item");
    assert!(
        !item_idx.iter().any(|n| n == "idx_item_kind"),
        "a constant join key on a 1100-row rel must lose its index: {item_idx:?}"
    );

    // Determinism gate: the first run answered the query WITH idx_item_kind
    // in place, the second WITHOUT it. Same program, same facts — an index
    // must never change results, byte for byte (`?` output is ordered).
    assert_eq!(
        out_with_index, out_without_index,
        "dropping an index must never change query output"
    );
    assert!(
        out_without_index.contains("tag_00000") && out_without_index.contains("alpha"),
        "sanity: the join actually produced rows:\n{out_without_index}"
    );
}

/// The skew lesson, pinned: a 2-distinct-value column KEEPS its index. On the
/// measured root, dropping enum-like columns (kind, 23 distinct; pos, 13
/// distinct) regressed rare-value probes by up to +110ms — distinct-count
/// selectivity reasoning is only safe at exactly one value, where an equality
/// seek provably equals the scan.
#[test]
fn two_value_join_key_stays_demanded() {
    let d = sandbox("twovalue");
    let mut src = String::new();
    for i in 0..BIG_FIXTURE_ROWS {
        let kind = if i % 2 == 0 { "kind_a" } else { "kind_b" };
        src.push_str(&format!("item {kind} tag_{i:05}\n"));
    }
    src.push_str("label kind_a alpha\nlabel kind_b beta\n");
    fs::write(d.join("src/data.rs"), src).unwrap();
    let db = d.join("db");

    run(&d, &db, CONSTANT_COLUMN_JOIN); // cold: created
    run(&d, &db, CONSTANT_COLUMN_JOIN); // warm: probe sees 2 distinct -> keep

    let conn = Connection::open(&db).unwrap();
    let item_idx = index_names(&conn, "rel_item");
    assert!(
        item_idx.iter().any(|n| n == "idx_item_kind"),
        "a 2-distinct-value join key must KEEP its index (skew makes it planner-needed): {item_idx:?}"
    );
}

#[test]
fn tiny_rel_join_key_is_dropped_once_measured() {
    let d = sandbox("tiny");
    // 20 items: far under TINY_REL_FLOOR. `tag` is distinct per row, so ONLY
    // the tiny-rel filter (not selectivity) can be the reason it drops.
    let mut src = String::new();
    for i in 0..20 {
        src.push_str(&format!("item kind_a tag_{i:05}\n"));
        src.push_str(&format!("pair tag_{i:05} peer_{i:05}\n"));
    }
    fs::write(d.join("src/data.rs"), src).unwrap();
    let db = d.join("db");
    // `tag` joins at non-prefix positions of both rels.
    let prog = r#"
rel item(path: file, kind: text, tag: text).
item(path, kind, tag) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /item (?<kind>kind_[ab]) (?<tag>tag_\w+)/, line).
rel pair(path: file, tag: text, peer: text).
pair(path, tag, peer) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /pair (?<tag>tag_\w+) (?<peer>peer_\w+)/, line).
rel matched(tag: text, peer: text).
matched(tag, peer) <- item(path1, kind, tag), pair(path2, tag, peer).
? matched(tag, peer).
"#;

    run(&d, &db, prog); // cold: created empty (no stats yet)
    run(&d, &db, prog); // warm: 20 rows < TINY_REL_FLOOR -> demand dropped

    let conn = Connection::open(&db).unwrap();
    let item_idx = index_names(&conn, "rel_item");
    let pair_idx = index_names(&conn, "rel_pair");
    assert!(
        !item_idx.iter().any(|n| n == "idx_item_tag"),
        "a 20-row rel must not keep a persistent join-key index: {item_idx:?}"
    );
    assert!(
        !pair_idx.iter().any(|n| n == "idx_pair_tag"),
        "a 20-row rel must not keep a persistent join-key index: {pair_idx:?}"
    );
}

#[test]
fn hand_authored_index_survives_the_sweep() {
    let d = sandbox("hand_index");
    fs::write(d.join("src/a.rs"), struct_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN);
    {
        let conn = Connection::open(&db).unwrap();
        // A name that does NOT start with the `idx_` auto-created prefix —
        // the reconcile sweep must never touch it, even though it indexes
        // the same table the sweep manages.
        conn.execute(
            "CREATE INDEX hand_rolled_symbol_path ON rel_symbol(path)",
            [],
        )
        .unwrap();
    }

    // Switch to the no-join program: prunes idx_symbol_name, must leave the
    // hand-authored index alone.
    run(&d, &db, WITHOUT_JOIN);

    let conn = Connection::open(&db).unwrap();
    let symbol_idx = index_names(&conn, "rel_symbol");
    assert!(
        symbol_idx.iter().any(|n| n == "hand_rolled_symbol_path"),
        "hand-authored index must survive the idx_ sweep: {symbol_idx:?}"
    );
    assert!(!symbol_idx.iter().any(|n| n == "idx_symbol_name"));
}

fn object_bytes(conn: &Connection, name: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name = ?1",
        [name],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Receipt test: a fixture where `rel_symbol`/`rel_other` carry enough rows
/// to give their join-key indexes real weight loses those index bytes when
/// the joining rule drops out of the served program. Not a byte-count
/// regression pin (page layout drifts with SQLite/rusqlite versions); it
/// asserts the delta is positive and prints it for the receipt.
#[test]
fn pruning_reclaims_measurable_index_bytes() {
    const ROWS: usize = 4000;
    let d = sandbox("bytes");
    fs::write(d.join("src/a.rs"), struct_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, WITH_JOIN); // creates schema + idx_symbol_name/idx_other_name

    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        {
            let mut str_stmt = conn
                .prepare("INSERT OR IGNORE INTO _strings (id, content) VALUES (?1, ?2)")
                .unwrap();
            let mut symbol_stmt = conn
                .prepare("INSERT OR IGNORE INTO rel_symbol (path, name, __src) VALUES (?1, ?2, '')")
                .unwrap();
            let mut other_stmt = conn
                .prepare("INSERT OR IGNORE INTO rel_other (path, name, __src) VALUES (?1, ?2, '')")
                .unwrap();
            for i in 0..ROWS {
                let name = format!("Bulk{i:04}");
                let path = format!("src/module_{i:04}/file_{i:04}.rs");
                let name_id = StringId::of(&name).sqlite();
                let path_id = StringId::of(&path).sqlite();
                str_stmt.execute(rusqlite::params![name_id, name]).unwrap();
                str_stmt.execute(rusqlite::params![path_id, path]).unwrap();
                symbol_stmt
                    .execute(rusqlite::params![path_id, name_id])
                    .unwrap();
                other_stmt
                    .execute(rusqlite::params![path_id, name_id])
                    .unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        conn.execute_batch("VACUUM").unwrap();
    }

    let (before_symbol_idx, before_other_idx) = {
        let conn = Connection::open(&db).unwrap();
        (
            object_bytes(&conn, "idx_symbol_name"),
            object_bytes(&conn, "idx_other_name"),
        )
    };
    assert!(
        before_symbol_idx > 0,
        "fixture's join-key index must carry real bytes pre-prune"
    );
    assert!(
        before_other_idx > 0,
        "fixture's join-key index must carry real bytes pre-prune"
    );

    run(&d, &db, WITHOUT_JOIN); // the join drops out; both indexes should prune

    let (after_symbol_idx, after_other_idx, db_before, db_after) = {
        let conn = Connection::open(&db).unwrap();
        let before_bytes: i64 = conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute_batch("VACUUM").unwrap();
        let after_bytes: i64 = conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |r| r.get(0),
            )
            .unwrap();
        (
            object_bytes(&conn, "idx_symbol_name"),
            object_bytes(&conn, "idx_other_name"),
            before_bytes,
            after_bytes,
        )
    };
    assert_eq!(
        after_symbol_idx, 0,
        "idx_symbol_name object must be gone post-VACUUM"
    );
    assert_eq!(
        after_other_idx, 0,
        "idx_other_name object must be gone post-VACUUM"
    );

    let index_delta = (before_symbol_idx + before_other_idx) - (after_symbol_idx + after_other_idx);
    let db_delta = db_before - db_after;
    eprintln!(
        "[storage-diet receipt] idx_symbol_name+idx_other_name before={} after=0 delta={} bytes; \
         db before={db_before} after={db_after} delta={db_delta} bytes; rows={ROWS}",
        before_symbol_idx + before_other_idx,
        index_delta,
    );
    assert!(index_delta > 0, "pruning must reclaim index bytes");
    assert!(
        db_delta > 0,
        "pruning must reclaim measurable db bytes on VACUUM"
    );
}

// `edge` is read by a SELF-REFERENTIAL recursive rule in its canonical
// shape: `walk(to) <- walk(from), edge(from, to).` binds `from` twice
// (walk's own recursive atom, position 1; edge's position 0) but `to` only
// once (edge's position 1) — raw co-occurrence alone proposes at most
// idx_edge_from, never idx_edge_to, no matter how the fixture is sized.
const REVERSE_TRAVERSAL_WALK: &str = r#"
rel edge(from: text, to: text).
edge(from, to) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /edge (?<from>\w+) (?<to>\w+)/, line).
rel seed_of(node: text).
seed_of(node) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /seed (?<node>\w+)/, line).
rel walk(node: text).
walk(node) <- seed_of(node).
walk(to) <- walk(from), edge(from, to).
? walk(node).
"#;

#[test]
fn reverse_column_of_a_recursive_walk_is_indexed() {
    let d = sandbox("rev_walk");
    fs::write(d.join("src/a.rs"), edge_chain_src(BIG_FIXTURE_ROWS)).unwrap();
    let db = d.join("db");
    run(&d, &db, REVERSE_TRAVERSAL_WALK); // cold: index created before extraction fills rows
    run(&d, &db, REVERSE_TRAVERSAL_WALK); // warm: stats probe runs against the filled rel

    let conn = Connection::open(&db).unwrap();
    let edge_idx = index_names(&conn, "rel_edge");
    assert!(
        edge_idx.iter().any(|n| n == "idx_edge_to"),
        "a self-referential recursive rule's edge atom must get its REVERSE \
         column indexed too — a graph walk is queried from either endpoint, \
         but raw co-occurrence only ever sees the forward `from` variable in \
         this canonical recursive shape: {edge_idx:?}"
    );
    assert!(
        !edge_idx.iter().any(|n| n == "idx_edge_from"),
        "`from` is the PK's leading column on this ROWID table (no key() \
         narrowing, full-row PK) — the PK autoindex already covers it, so \
         adding the reverse column as a candidate must not smuggle the \
         forward one past the existing PK-prefix filter: {edge_idx:?}"
    );
    assert!(
        object_bytes(&conn, "idx_edge_to") > 0,
        "the created reverse index must carry real bytes, not an empty shell"
    );
}

#[test]
fn stale_pk_prefix_duplicate_index_is_swept_even_when_hand_inserted() {
    // Task-3 finding: `create_auto_indexes`'s reconcile computes
    // `wanted_names` fresh every tick from the CURRENT program and stats —
    // it never trusts what already exists on disk — so a PK-prefix
    // duplicate that predates today's filter (declare.rs filter #1, or any
    // future policy) is swept exactly like every other stale idx_ entry,
    // regardless of how it got there. This is a receipt for that existing
    // behavior, not a fix: `idx_first_key_name` was ALREADY correctly
    // absent under `covered_prefix_join_key_is_never_demanded` (this file);
    // this test additionally proves the sweep removes one even when it is
    // hand-inserted (standing in for a legacy index minted before the
    // filter existed), which the "never demanded" test alone cannot show.
    let d = sandbox("stale_prefix");
    fs::write(d.join("src/a.rs"), struct_src(8)).unwrap();
    let db = d.join("db");
    run(&d, &db, COVERED_PREFIX_JOIN);
    {
        let conn = Connection::open(&db).unwrap();
        conn.execute("CREATE INDEX idx_first_key_name ON rel_first_key(name)", [])
            .unwrap();
        assert!(
            index_names(&conn, "rel_first_key")
                .iter()
                .any(|n| n == "idx_first_key_name"),
            "setup: the hand-inserted index must exist before the next tick"
        );
    }

    // Re-tick the SAME program: the reconcile sweep runs again regardless
    // of whether anything in the program changed.
    run(&d, &db, COVERED_PREFIX_JOIN);

    let conn = Connection::open(&db).unwrap();
    let first_key_idx = index_names(&conn, "rel_first_key");
    assert!(
        !first_key_idx.iter().any(|n| n == "idx_first_key_name"),
        "a hand-inserted PK-prefix duplicate must be swept on the next \
         reconcile, the same as any other idx_ entry the current demand \
         set no longer wants: {first_key_idx:?}"
    );
}
