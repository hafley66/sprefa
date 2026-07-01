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
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
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
