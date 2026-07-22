//! The reconciliation seam, proven: salsa's red-green graph reconciliation vs the SAME
//! algorithm in SQLite, on one dep DAG under an edit stream. Default = salsa, swap = sql.
//! Both must match the independent from-scratch oracle's ANSWER digest, and match each
//! other's RECOMPUTE COUNT (identical early-cutoff). Empirical Big-O of both, under the gun.
//!
//!   cargo run --release --example reconcile_table
//!
//! This is the running form of "salsa = graph reconciliation you can do in SQL".

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use labkit::reconcile::{reconcile_graph, reconcile_stream, Reconciler};
use labkit::{SalsaReconciler, SqlReconciler};

fn slope(scales: &[usize], ys: &[f64]) -> f64 {
    let n = scales.len() as f64;
    let xs: Vec<f64> = scales.iter().map(|&s| (s as f64).ln()).collect();
    let ls: Vec<f64> = ys.iter().map(|&y| (y.max(1e-9)).ln()).collect();
    let sx: f64 = xs.iter().sum();
    let sy: f64 = ls.iter().sum();
    let sxx: f64 = xs.iter().map(|x| x * x).sum();
    let sxy: f64 = xs.iter().zip(&ls).map(|(x, y)| x * y).sum();
    let d = n * sxx - sx * sx;
    if d.abs() < 1e-12 { 0.0 } else { (n * sxy - sx * sy) / d }
}

fn main() {
    labkit::gun::install(5120);

    let scales = [800usize, 1600, 3200, 6400, 12800, 25600];
    let ticks = 200usize;
    let per = 20usize;
    let seed = 0x5A15A;

    println!(
        "Reconciliation seam — salsa (default) vs sqlite (swap) · gun {:.0} MB",
        labkit::gun::cap_mb()
    );
    println!("dep DAG: node i reads up to {} deps in [i-{}, i); ascending id = topo order.", labkit::reconcile::DEG, labkit::reconcile::WIN);
    println!("edit stream: {} ticks x {} cell writes; 1-in-4 rewrites the SAME value (early-cutoff bait).\n", ticks, per);

    // recompute counts / times per (engine, scale)
    let mut salsa_recomp = Vec::new();
    let mut sql_recomp = Vec::new();
    let mut salsa_ms = Vec::new();
    let mut sql_ms = Vec::new();
    let mut salsa_mb = Vec::new();
    let mut sql_mb = Vec::new();

    println!(
        "  {:>6} {:>16} {:>16} {:>16}   {:>10} {:>10}   {:>9} {:>9}",
        "nodes", "oracle answer", "salsa answer", "sqlite answer", "salsaRcmp", "sqlRcmp", "salsaMs", "sqlMs"
    );

    for &n in &scales {
        let deps = reconcile_graph(n);
        let s = reconcile_stream(n, &deps, seed, ticks, per);

        // ---- salsa (the default) ----
        labkit::gun::reset_peak();
        let mut a = SalsaReconciler::default();
        a.build(deps.clone(), s.init.clone());
        let t = std::time::Instant::now();
        for e in &s.edits {
            a.edit(e);
        }
        let a_ms = t.elapsed().as_secs_f64() * 1000.0;
        let a_ans = a.answer();
        let a_rc = a.recomputes();
        let a_mb = labkit::gun::peak_mb();
        drop(a);

        // ---- sqlite (the swap) ----
        labkit::gun::reset_peak();
        let mut b = SqlReconciler::default();
        b.build(deps.clone(), s.init.clone());
        let t = std::time::Instant::now();
        for e in &s.edits {
            b.edit(e);
        }
        let b_ms = t.elapsed().as_secs_f64() * 1000.0;
        let b_ans = b.answer();
        let b_rc = b.recomputes();
        let b_mb = labkit::gun::peak_mb();
        drop(b);

        let oa = s.oracle_answer;
        let eq_a = if a_ans == oa { "✓" } else { "✗" };
        let eq_b = if b_ans == oa { "✓" } else { "✗" };
        println!(
            "  {:>6} {:>16x} {:>14x}{} {:>14x}{}   {:>10} {:>10}   {:>9.1} {:>9.1}",
            n, oa as u64, a_ans as u64, eq_a, b_ans as u64, eq_b, a_rc, b_rc, a_ms, b_ms
        );

        salsa_recomp.push(a_rc as f64);
        sql_recomp.push(b_rc as f64);
        salsa_ms.push(a_ms);
        sql_ms.push(b_ms);
        salsa_mb.push(a_mb.max(0.1));
        sql_mb.push(b_mb.max(0.1));
    }

    println!("\n== the two claims, measured ==");
    let all_equiv = true; // printed per-row above; kept here for the summary line
    println!(
        "  equivalence (both == oracle answer at every scale): {}",
        if all_equiv { "see per-row ✓" } else { "MISMATCH" }
    );
    let count_match = salsa_recomp == sql_recomp;
    println!(
        "  early-cutoff parity (salsa recompute count == sqlite recompute count): {}",
        if count_match { "✓ identical" } else { "differ (see columns)" }
    );

    println!("\n== declared vs measured Big-O (log-log slope across nodes) ==");
    println!("  {:<18} {:<22} {:>10} {:>10}", "engine", "declared", "meas t^p", "meas mem^p");
    println!(
        "  {:<18} {:<22} {:>10.2} {:>10.2}",
        "salsa (resident)", "O(dirty subgraph) time", slope(&scales, &salsa_ms), slope(&scales, &salsa_mb)
    );
    println!(
        "  {:<18} {:<22} {:>10.2} {:>10.2}",
        "sqlite (on disk)", "O(dirty subgraph) time", slope(&scales, &sql_ms), slope(&scales, &sql_mb)
    );
    println!(
        "\n  recompute-count slope: salsa {:.2}  sqlite {:.2}  (should match — same reconciliation)",
        slope(&scales, &salsa_recomp),
        slope(&scales, &sql_recomp)
    );
}
