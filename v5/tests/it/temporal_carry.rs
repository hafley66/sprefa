//! `@next` carry relations: state that survives a tick boundary. A `@next` rule
//! evaluates its body over THIS tick's converged relations and stages the head
//! rows into a carry buffer at `tx+1`; the next tick loads them as the live rel
//! before deriving. See docs/research-reactive-effectful-datalog.md §8.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_temporal_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Count rows in a rel table via a fresh read connection to the same db file.
fn rel_count(db_path: &Path, rel: &str) -> i64 {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM rel_{rel}"), [], |r| r.get(0)).unwrap()
}

fn ints(db_path: &Path, rel: &str, col: &str) -> Vec<i64> {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut s = conn.prepare(&format!("SELECT \"{col}\" FROM rel_{rel} ORDER BY \"{col}\"")).unwrap();
    let v: Vec<i64> = s.query_map([], |r| r.get(0)).unwrap().filter_map(|x| x.ok()).collect();
    v
}

/// `echo(v) <- @next ping(v)` lags ping by exactly one tick: empty on tick 1,
/// populated from tick 2 on. The proof that a fact crossed a generation boundary.
#[test]
fn next_carry_lags_by_one_tick() {
    let d = sandbox("echo");
    let dbp = d.join("db");
    fs::write(d.join("p.dl"),
        "rel ping(v: int).\n\
         ping(7).\n\
         rel echo(v: int).\n\
         echo(v) <- @next ping(v).\n").unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();

    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Tick 1 (tx 0 -> 1): carry@0 is empty, so echo is empty; ping staged into
    // carry@1.
    eng.tick(&prog, true).unwrap();
    assert_eq!(rel_count(&dbp, "echo"), 0, "echo must be empty on the first tick");
    assert_eq!(ints(&dbp, "ping", "v"), vec![7]);

    // Tick 2 (tx 1 -> 2): carry@1 = {7} loads into echo.
    eng.tick(&prog, true).unwrap();
    assert_eq!(ints(&dbp, "echo", "v"), vec![7], "echo must carry ping's tick-1 value");

    // Tick 3: still {7} (ping is steady, so the carry is a fixpoint).
    eng.tick(&prog, true).unwrap();
    assert_eq!(ints(&dbp, "echo", "v"), vec![7]);
}

/// A monotone accumulator: `acc` gathers every value `ping` ever held. Proves the
/// carry composes with an ordinary derived rule reading the carried-in rel.
#[test]
fn next_carry_accumulates() {
    let d = sandbox("acc");
    let dbp = d.join("db");
    // acc(v) this tick = whatever was staged last tick; acc_next stages the union
    // of the carried acc and the current ping. (acc is @next-only; acc_next is an
    // ordinary derived union.)
    fs::write(d.join("p.dl"),
        "rel ping(v: int).\n\
         ping(1).\n\
         ping(2).\n\
         rel acc(v: int).\n\
         rel acc_next(v: int).\n\
         acc_next(v) <- ping(v).\n\
         acc_next(v) <- acc(v).\n\
         acc(v) <- @next acc_next(v).\n").unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();

    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    eng.tick(&prog, true).unwrap();           // acc={}, acc_next={1,2}, carry acc@1={1,2}
    assert_eq!(rel_count(&dbp, "acc"), 0);
    eng.tick(&prog, true).unwrap();           // acc={1,2}
    assert_eq!(ints(&dbp, "acc", "v"), vec![1, 2], "acc carries the prior acc_next");
}

/// A carry rel may be written ONLY by @next rules: heading it with a derived rule
/// too is a loud conflict (it would be wiped by rebuild_derived).
#[test]
fn carry_rel_must_be_next_only() {
    let d = sandbox("conflict");
    fs::write(d.join("p.dl"),
        "rel ping(v: int).\n\
         ping(1).\n\
         rel mixed(v: int).\n\
         mixed(v) <- ping(v).\n\
         mixed(v) <- @next ping(v).\n").unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let err = eng.tick(&prog, true).unwrap_err();
    assert!(err.to_string().contains("written only by @next"), "expected a carry-conflict bail: {err}");
}
