//! Lattice (`merge(MaxBy(col))`) and choice-domain (`key(...)`) qualifiers.
//!
//! `key(c1, ...)` narrows the conflict target to a column subset = a functional
//! dependency / Soufflé choice-domain (APLAS'21). Without a `merge`, `INSERT OR
//! IGNORE` keeps the first row per key (silent-pick choice). With
//! `merge(MaxBy(col))`, an UPSERT keeps the row whose `col` is greatest
//! (row-selection lattice: one winner per key, the highest-priority rule).
//!
//! These are the language constructs that let an MCP dispatch handler be
//! authored IN dl: a `send` relation keyed on the request `id` with
//! `MaxBy(prio)` picks the highest-priority matching rule's response, and the
//! whole winning row stays intact. Incremental-safe: retract + remerge is
//! monotone, so a warm db converges without stratum recompute.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lattice_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .args(["--db", dir.join("db").to_str().unwrap()])
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `MaxBy(prio)` keeps the highest-priority row per key. Two rules heading the
/// same keyed rel with different priorities: the greater prio wins, the whole
/// row (its `body`) surfaces.
#[test]
fn maxby_keeps_highest_priority_row() {
    let d = sandbox("maxby");
    let (code, out, err) = run(&d, concat!(
        "rel send(id: int, kind: text, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel req(id: int, kind: text).\n",
        "req(1, \"ping\").\n",
        "send(id, kind, \"fast\", 1) <- req(id, kind).\n",
        "send(id, kind, \"slow-but-best\", 5) <- req(id, kind).\n",
        "? send(id, kind, body, prio).\n"));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("slow-but-best"), "MaxBy(prio) must keep the prio=5 row:\n{out}");
    assert!(!out.contains("fast"), "the prio=1 row must lose to prio=5:\n{out}");
    assert!(out.contains("5"), "the winning prio value must surface:\n{out}");
}

/// A tie on the merge column keeps the first-wins row (strict `>` in the WHERE,
/// so an equal-prio challenger does NOT displace the incumbent). Deterministic
/// dispatch needs this: a later rule at the same priority does not flip-flop.
#[test]
fn maxby_tie_keeps_incumbent() {
    let d = sandbox("tie");
    let (code, out, err) = run(&d, concat!(
        "rel send(id: int, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"first\", 3) <- r(id).\n",
        "send(id, \"second\", 3) <- r(id).\n",
        "? send(id, body, prio).\n"));
    assert_eq!(code, 0, "{err}");
    // Exactly one row; the incumbent (first) wins on a tie.
    let rows: Vec<&str> = out.lines().filter(|l| l.contains("first") || l.contains("second")).collect();
    assert_eq!(rows.len(), 1, "one row per key expected, got:\n{out}");
    assert!(rows[0].contains("first"), "tie must keep the first row:\n{out}");
}

/// `key(...)` alone (no merge) is Soufflé choice-domain: silent first-wins via
/// `INSERT OR IGNORE`. Among several rules heading the same keyed rel, the
/// fixpoint inserts whichever fires first; later rows for the same key are
/// ignored. Exactly one row per key survives.
#[test]
fn key_alone_is_choice_domain_first_wins() {
    let d = sandbox("keyonly");
    let (code, out, err) = run(&d, concat!(
        "rel pick(id: int, src: text) key(id).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "pick(id, \"a\") <- r(id).\n",
        "pick(id, \"b\") <- r(id).\n",
        "pick(id, \"c\") <- r(id).\n",
        "? pick(id, src).\n"));
    assert_eq!(code, 0, "{err}");
    // Output is TSV (unquoted); assert on the reported row count, not the
    // literal quoting. Exactly one row per key survives choice-domain.
    assert!(out.contains("(1 rows)"), "key(id) must collapse to one row:\n{out}");
}

/// A multi-column key: the conflict target is the declared subset, so rows with
/// the same key cols but different non-key cols coexist, and MaxBy ranks within
/// each key group.
#[test]
fn composite_key_ranks_within_group() {
    let d = sandbox("comp");
    let (code, out, err) = run(&d, concat!(
        "rel send(id: int, shard: int, body: text, prio: int) key(id, shard) merge(MaxBy(prio)).\n",
        "rel r(id: int, shard: int).\n",
        "r(1, 10).\n",
        "r(1, 20).\n",
        "send(id, shard, \"lo\", 1) <- r(id, shard).\n",
        "send(id, shard, \"hi\", 9) <- r(id, shard).\n",
        "? send(id, shard, body, prio).\n"));
    assert_eq!(code, 0, "{err}");
    // Two key groups (1,10) and (1,20); each keeps the prio=9 "hi" row.
    let hi = out.matches("hi").count();
    let lo = out.matches("lo").count();
    assert_eq!(hi, 2, "both shards keep the hi row:\n{out}");
    assert_eq!(lo, 0, "no lo row survives MaxBy:\n{out}");
}

/// merge without key is a compile-time error: the conflict target is undefined.
#[test]
fn merge_without_key_is_an_error() {
    let d = sandbox("nokey");
    let (code, _out, err) = run(&d, concat!(
        "rel s(id: int, p: int) merge(MaxBy(p)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "s(id, 1) <- r(id).\n",
        "? s(id, p).\n"));
    assert_ne!(code, 0, "merge without key must fail:\n{err}");
    assert!(err.contains("merge") || err.contains("key"), "must mention the missing key:\n{err}");
}

/// The merge column must not also be a key column: it ranks rows WITHIN a key.
#[test]
fn merge_col_cannot_be_a_key_col() {
    let d = sandbox("mergeiskey");
    let (code, _out, err) = run(&d, concat!(
        "rel s(id: int, prio: int) key(id, prio) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "s(id, 5) <- r(id).\n",
        "? s(id, prio).\n"));
    assert_ne!(code, 0, "merge col in key must fail:\n{err}");
}

/// A keyed rel with a non-existent key column is rejected at declare time.
#[test]
fn key_column_must_exist() {
    let d = sandbox("badkey");
    let (code, _out, err) = run(&d, concat!(
        "rel s(id: int, p: int) key(nope).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "s(id, 1) <- r(id).\n",
        "? s(id, p).\n"));
    assert_ne!(code, 0, "unknown key col must fail:\n{err}");
    assert!(err.contains("nope"), "must name the bad column:\n{err}");
}

/// Warm-db incrementality: a second run reuses the cache. A higher-priority row
/// added in the second program must displace the incumbent on the warm db
/// (retract + remerge is monotone; no stratum recompute needed).
#[test]
fn maxby_promotes_on_warm_db() {
    let d = sandbox("warm");
    let p1 = concat!(
        "rel send(id: int, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"v1\", 1) <- r(id).\n",
        "? send(id, body, prio).\n");
    let (code, out, err) = run(&d, p1);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("v1"));

    let p2 = concat!(
        "rel send(id: int, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"v1\", 1) <- r(id).\n",
        "send(id, \"v2\", 9) <- r(id).\n",
        "? send(id, body, prio).\n");
    let (code, out, err) = run(&d, p2);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("v2"), "warm db must promote the prio=9 row:\n{out}");
    assert!(!out.contains("v1"), "the prio=1 incumbent must be displaced:\n{out}");
}

/// Hot-reload wedge regression: ADD a `key(...) merge(...)` to a rel that ran on
/// the warm db WITHOUT a key. The columns are identical, so the column-set
/// migration does not fire — only the PK changes (full-row -> key(id)). Before
/// the key-drift migration, the cached full-row PK stayed and every tick failed
/// "ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint",
/// wedging the daemon. Now `declare` drops+recreates the table when the key set
/// moves, so the lattice upsert works on the warm db.
#[test]
fn add_key_on_warm_db_does_not_wedge() {
    let d = sandbox("addkey");
    // p1: no key => full-row PRIMARY KEY, one seed row establishes the table.
    let p1 = concat!(
        "rel send(id: int, body: text, prio: int).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"seed\", 1) <- r(id).\n",
        "? send(id, body, prio).\n");
    let (code, _out, err) = run(&d, p1);
    assert_eq!(code, 0, "p1 (no key) must succeed:\n{err}");

    // p2: SAME columns, now key(id) merge(MaxBy(prio)). Column set is unchanged,
    // so only the key/PK drifts (the isolated repro of the daemon wedge).
    let p2 = concat!(
        "rel send(id: int, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"lo\", 1) <- r(id).\n",
        "send(id, \"hi\", 9) <- r(id).\n",
        "? send(id, body, prio).\n");
    let (code, out, err) = run(&d, p2);
    assert_eq!(code, 0, "adding key(...) on a warm db must not wedge:\n{err}");
    assert!(!err.contains("ON CONFLICT"), "no ON CONFLICT wedge:\n{err}");
    assert!(out.contains("hi"), "MaxBy(prio) must keep the prio=9 row:\n{out}");
    assert!(!out.contains("lo"), "the prio=1 row must lose after the key is added:\n{out}");
    assert!(out.contains("(1 rows)"), "key(id) must collapse to one row:\n{out}");
}

/// Reverse transition: REMOVE a `key(...) merge(...)` on a warm db. The PK
/// widens from key(id) back to the full row, so rows that the lattice had
/// collapsed to one now coexist as set-semantics rows. Same drop+recreate path.
#[test]
fn remove_key_on_warm_db_widens_pk() {
    let d = sandbox("removekey");
    // p1: key(id) merge => one winning row per id.
    let p1 = concat!(
        "rel send(id: int, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"lo\", 1) <- r(id).\n",
        "send(id, \"hi\", 9) <- r(id).\n",
        "? send(id, body, prio).\n");
    let (code, out, err) = run(&d, p1);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("(1 rows)"), "keyed p1 collapses to one row:\n{out}");

    // p2: SAME columns, no key => full-row PK. Both distinct rows survive.
    let p2 = concat!(
        "rel send(id: int, body: text, prio: int).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"lo\", 1) <- r(id).\n",
        "send(id, \"hi\", 9) <- r(id).\n",
        "? send(id, body, prio).\n");
    let (code, out, err) = run(&d, p2);
    assert_eq!(code, 0, "removing key(...) on a warm db must not wedge:\n{err}");
    assert!(out.contains("(2 rows)"), "full-row PK keeps both rows:\n{out}");
}

/// Key-column CHANGE on a warm db: key(id) -> key(id, shard). The conflict
/// target column set differs, so the table is dropped+recreated; the finer key
/// splits what was one group into two.
#[test]
fn change_key_columns_on_warm_db() {
    let d = sandbox("changekey");
    // p1: key(id) only => both shards collapse into the single id=1 group.
    let p1 = concat!(
        "rel send(id: int, shard: int, body: text, prio: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int, shard: int).\n",
        "r(1, 10).\n",
        "r(1, 20).\n",
        "send(id, shard, \"lo\", 1) <- r(id, shard).\n",
        "send(id, shard, \"hi\", 9) <- r(id, shard).\n",
        "? send(id, shard, body, prio).\n");
    let (code, out, err) = run(&d, p1);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("(1 rows)"), "key(id) collapses both shards to one row:\n{out}");

    // p2: key(id, shard) => one winner per (id, shard) => two rows.
    let p2 = concat!(
        "rel send(id: int, shard: int, body: text, prio: int) key(id, shard) merge(MaxBy(prio)).\n",
        "rel r(id: int, shard: int).\n",
        "r(1, 10).\n",
        "r(1, 20).\n",
        "send(id, shard, \"lo\", 1) <- r(id, shard).\n",
        "send(id, shard, \"hi\", 9) <- r(id, shard).\n",
        "? send(id, shard, body, prio).\n");
    let (code, out, err) = run(&d, p2);
    assert_eq!(code, 0, "changing the key column set on a warm db must not wedge:\n{err}");
    assert!(out.contains("(2 rows)"), "key(id, shard) splits into two groups:\n{out}");
    assert_eq!(out.matches("hi").count(), 2, "each shard keeps its hi row:\n{out}");
}

/// Merge-fn-only change on a warm db: key(id) stays, MaxBy(a) -> MaxBy(b). The
/// PK column set is UNCHANGED, so this fix must NOT drop the table (a merge-fn
/// change is schema-invariant — the upsert SQL is regenerated by `lower_rule`
/// each tick). The decision: no drop, so the tick must not wedge. Because the
/// table is preserved, the warm incumbent survives (a full re-rank across a
/// merge-fn change is governed by the digest-skip path, out of scope for this
/// schema fix); the guaranteed property here is no ON CONFLICT wedge + one row.
#[test]
fn change_merge_fn_only_on_warm_db() {
    let d = sandbox("mergefn");
    // p1: rank by prio => "p-hi" wins (prio 9 > 1).
    let p1 = concat!(
        "rel send(id: int, body: text, prio: int, seq: int) key(id) merge(MaxBy(prio)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"p-hi\", 9, 1) <- r(id).\n",
        "send(id, \"s-hi\", 1, 9) <- r(id).\n",
        "? send(id, body, prio, seq).\n");
    let (code, out, err) = run(&d, p1);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("p-hi"), "p1 ranks by prio:\n{out}");

    // p2: SAME key(id), rank by seq instead. PK unchanged => no drop, no wedge.
    let p2 = concat!(
        "rel send(id: int, body: text, prio: int, seq: int) key(id) merge(MaxBy(seq)).\n",
        "rel r(id: int).\n",
        "r(1).\n",
        "send(id, \"p-hi\", 9, 1) <- r(id).\n",
        "send(id, \"s-hi\", 1, 9) <- r(id).\n",
        "? send(id, body, prio, seq).\n");
    let (code, out, err) = run(&d, p2);
    assert_eq!(code, 0, "merge-fn-only change on a warm db must not wedge:\n{err}");
    assert!(!err.contains("ON CONFLICT"), "merge-fn change is schema-invariant, no wedge:\n{err}");
    assert!(out.contains("(1 rows)"), "still one row per key (table not dropped):\n{out}");
}
