//! IS SALSA RESIDENT, AND DOES IT EAT THE BUDGET? Measured, on the real crate.
//!
//!   cargo run --release --example salsa_ram -- rows   1000 10000   # memoize the rows
//!   cargo run --release --example salsa_ram -- digest 1000 10000   # memoize a digest
//!
//! Same inputs, same computation, same number of derived facts. The ONLY difference
//! is what the tracked query RETURNS — and therefore what salsa keeps resident:
//!   rows   -> Vec<u64> of length m   (salsa caches every fact)  => RSS scales w/ FACTS
//!   digest -> one u64                (rows would go to the cascade) => RSS FLAT in facts
//!
//! Salsa is resident either way. This shows the residency is a consequence of the
//! design choice, not of the framework. Drive both with examples/ram.sh.

use reactor_lab::{peak_rss_mb, Db};

#[salsa::input]
struct Src {
    seed: u64,
    m: u32,
}

/// ROWS strategy: return every derived fact. This is the natural way to write it, and
/// it makes salsa's memo hold m u64s per file — O(total facts) resident.
#[salsa::tracked]
fn rows(db: &dyn salsa::Database, s: Src) -> Vec<u64> {
    let seed = *s.seed(db);
    (0..*s.m(db) as u64).map(|i| seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(i)).collect()
}

/// DIGEST strategy: fold the same m facts into one u64. The rows are computed and
/// dropped (in the real system they are written to the sqlite cascade); salsa's memo
/// holds 8 bytes per file — O(rels) resident, independent of facts.
#[salsa::tracked]
fn digest(db: &dyn salsa::Database, s: Src) -> u64 {
    let seed = *s.seed(db);
    (0..*s.m(db) as u64).fold(0u64, |a, i| a ^ seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(i))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("rows");
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let m: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10000);
    let facts = n * m as usize;

    let base = peak_rss_mb();
    let db = Db::default();
    let srcs: Vec<Src> = (0..n).map(|i| Src::new(&db, i as u64 + 1, m)).collect();

    // query every file so every memo is populated and held resident.
    let mut sink = 0u64;
    match mode {
        "rows" => {
            for &s in &srcs {
                sink ^= rows(&db, s).iter().fold(0, |a, b| a ^ b);
            }
        }
        "digest" => {
            for &s in &srcs {
                sink ^= digest(&db, s);
            }
        }
        other => {
            eprintln!("mode must be rows|digest, got {other:?}");
            return;
        }
    }
    std::hint::black_box(sink);

    // keep db (and its memo table) alive across the measurement.
    let rss = peak_rss_mb();
    println!(
        "{:>6}  n={:>5} files  m={:>6} facts/file  = {:>9} facts   peakRSS {:>7.1} MB   (memo grew ~{:.1} MB)",
        mode, n, m, facts, rss, rss - base
    );
    std::hint::black_box(&db);
}
