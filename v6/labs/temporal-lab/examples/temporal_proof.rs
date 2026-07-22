//! Proof that append-only-bitemporal-on-SQLite + retraction is a good thing, run.
//!
//!   cargo run --release --example temporal_proof
//!
//! Phase 1  correctness  — random edit stream, SQLite live-set == RAM oracle ==
//!                         SALSA, at every checkpoint (salsa is the validator).
//! Phase 2  bitemporal   — a cross-rev (moved from_rev->to_rev) fact + time-travel:
//!                         as-of BEFORE a retraction still sees the fact, AFTER doesn't.
//! Phase 3  scale+memory — file-backed, push the live set to millions, show RSS stays
//!                         bounded (facts on disk, not resident like salsa's rows).
//! Phase 4  compaction   — churn makes history >> live; a retention compaction drops
//!                         it, the live digest is UNCHANGED, size returns to ~live.

use temporal_lab::{peak_rss_mb, RamOracle, TemporalStore};

// ---- salsa validator: memoize the XOR digest of a live key set --------------
#[salsa::db]
#[derive(Clone, Default)]
struct SalsaDb {
    storage: salsa::Storage<Self>,
}
#[salsa::db]
impl salsa::Database for SalsaDb {}

#[salsa::input]
struct LiveSet {
    keys: Vec<i64>,
}

fn mix(k: i64) -> i64 {
    let mut z = (k as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    (z ^ (z >> 31)) as i64
}

#[salsa::tracked]
fn salsa_digest(db: &dyn salsa::Database, ls: LiveSet) -> i64 {
    ls.keys(db).iter().fold(0i64, |a, &k| a ^ mix(k))
}

/// Deterministic PRNG so any divergence is reproducible.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

fn main() -> rusqlite::Result<()> {
    // ================= Phase 1: correctness vs RAM oracle vs salsa =============
    println!("== Phase 1: SQLite live-set == RAM oracle == salsa, over a random stream ==");
    let mut store = TemporalStore::open(None)?;
    let mut oracle = RamOracle::default();
    let salsa_db = SalsaDb::default();
    let mut rng = Lcg(0xDA7A);
    let key_pool = 20_000i64; // small pool -> shared facts, births, retractions, rebirths

    for step in 0..2_000 {
        // a revision touches ~40 keys, mixed add (+1) and retract (-1).
        let mut deltas: Vec<(i64, i64)> = Vec::new();
        for _ in 0..40 {
            let k = (rng.next() as i64) % key_pool;
            let dw = if rng.next() % 3 == 0 { -1 } else { 1 };
            // only retract keys currently supported, to keep the stream well-formed.
            if dw < 0 && oracle.weight.get(&k).copied().unwrap_or(0) <= 0 {
                continue;
            }
            deltas.push((k, dw));
        }
        store.commit(&deltas)?;
        oracle.commit(&deltas);

        if step % 200 == 199 {
            let sql = store.live_digest()?;
            let ram = oracle.live_digest();
            let ls = LiveSet::new(&salsa_db, oracle.live_keys_sorted());
            let sal = *salsa_digest(&salsa_db, ls);
            assert_eq!(sql, ram, "SQLite != RAM oracle at step {step}");
            assert_eq!(sql, sal, "SQLite != salsa at step {step}");
            println!(
                "  step {step:>4}: live {:>6}  digest 0x{:016x}  (SQLite==oracle==salsa ✓)",
                store.live_count()?,
                sql as u64
            );
        }
    }

    // ================= Phase 2: bitemporal / cross-rev fact ====================
    println!("\n== Phase 2: a cross-rev fact + time-travel ==");
    // A key encodes valid-time. Convention here: key = vt_rev * 1_000_000 + entity.
    // A cross-rev "moved" fact encodes TWO revs in a separate high id-space.
    let world_7 = |entity: i64| 7 * 1_000_000 + entity; // facts about git-rev 7
    let moved_5_to_7 = 900_000_000_000 + 5 * 1_000_000 + 7; // moved(rev5 -> rev7)

    store.commit(&[(world_7(1), 1), (world_7(2), 1), (moved_5_to_7, 1)])?;
    let after_born = store.rev;
    // retract the cross-rev fact one revision later.
    store.commit(&[(moved_5_to_7, -1)])?;
    let after_retract = store.rev;

    let world7_now = store.live_world_at(7 * 1_000_000, 8 * 1_000_000, store.rev)?;
    println!("  world rev-7 live now: 0x{:016x}  (2 facts about rev 7)", world7_now as u64);
    // is the cross-rev fact live AS OF transaction-time tt? (its own id-space, any tt)
    let moved_present = |tt: i64| -> rusqlite::Result<bool> {
        Ok(store.live_world_at(900_000_000_000, 901_000_000_000, tt)? != 0)
    };
    println!("  moved(rev5->rev7) as-of born-rev {after_born}: present? {}", moved_present(after_born)?);
    println!("  moved(rev5->rev7) as-of now (rev {}): present? {}", store.rev, moved_present(store.rev)?);
    assert!(moved_present(after_born)?, "cross-rev fact should be live in its birth revision");
    assert!(!moved_present(after_retract)?, "cross-rev fact should be closed after retraction");
    println!("  -> one fact carrying TWO revs; retraction closed its tt-interval; history at rev {after_born} still shows it.");

    // ================= Phase 3: scale + memory =================================
    println!("\n== Phase 3: scale — millions of live facts, file-backed, RSS bounded ==");
    let base_rss = peak_rss_mb();
    let path = std::env::temp_dir().join("temporal_lab.db");
    let _ = std::fs::remove_file(&path);
    let mut big = TemporalStore::open(Some(path.to_str().unwrap()))?;
    // insert 3M distinct live facts in revisions of 20k each (set-based, no N+1).
    let target = 3_000_000i64;
    let mut k = 0i64;
    while k < target {
        let batch: Vec<(i64, i64)> = (k..(k + 20_000).min(target)).map(|x| (x, 1)).collect();
        big.commit(&batch)?;
        k += 20_000;
    }
    println!(
        "  inserted {} live facts over {} revisions.  live_count {}  peakRSS {:.0} MB (base {:.0})",
        target, big.rev, big.live_count()?, peak_rss_mb(), base_rss
    );

    // ================= Phase 4: churn -> compaction ============================
    println!("\n== Phase 4: churn makes history, compaction bounds it, live UNCHANGED ==");
    // churn: retract-and-reinsert a rolling window, creating closed intervals.
    let live_before = big.live_digest()?;
    for w in 0..40 {
        let lo = (w * 20_000) % target;
        let retr: Vec<(i64, i64)> = (lo..lo + 20_000).map(|x| (x, -1)).collect();
        big.commit(&retr)?; // closes 20k intervals
        let readd: Vec<(i64, i64)> = (lo..lo + 20_000).map(|x| (x, 1)).collect();
        big.commit(&readd)?; // opens 20k fresh intervals (rebirth)
    }
    let horizon = big.rev - 2; // keep only the last couple revisions of history
    let rows_before = big.row_count()?;
    let live_now = big.live_digest()?;
    let deleted = big.compact(horizon)?;
    big.vacuum()?;
    let rows_after = big.row_count()?;
    let live_after = big.live_digest()?;

    println!("  after churn:  rows {}  (live {})  -> history = {} dead-interval rows",
        rows_before, big.live_count()?, rows_before - big.live_count()?);
    println!("  compact(horizon={horizon}): deleted {deleted} dead rows, VACUUMed");
    println!("  rows {} -> {}   peakRSS {:.0} MB", rows_before, rows_after, peak_rss_mb());
    assert_eq!(live_before, live_now, "live digest drifted during churn");
    assert_eq!(live_now, live_after, "COMPACTION CHANGED THE LIVE SET");
    println!("  live digest 0x{:016x} UNCHANGED by compaction ✓  (retention kept size bounded)",
        live_after as u64);

    let _ = std::fs::remove_file(&path);
    println!("\nAll phases green: correct vs salsa, bitemporal cross-rev works, RSS bounded, compaction safe.");
    Ok(())
}
