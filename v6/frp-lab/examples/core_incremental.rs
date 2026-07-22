//! The break point of `derive_family_batch`, measured.
//!
//!   cargo run --release --example core_incremental
//!
//! Three findings, in order:
//!   1. Full recompute is O(corpus) PER EDIT — the "family is terrible" claim, with
//!      timings. Changing one line costs the same as building the world.
//!   2. Naive-set delta is a CORRECTNESS bug on shared edges.
//!   3. Z-set (weighted) delta is correct and O(changed file), not O(corpus) — and
//!      it is the store lab's weight cascade, still owned batch, still not a stream.

use frp_lab::{derive_family_batch, Edge, FamilyNaiveSet, FamilyZSet, File};
use std::time::Instant;

/// A synthetic corpus: `n` files, each a chain of `caller callee` lines. File i's
/// symbols are namespaced so most edges are unique, but a few overlap across files
/// (the shared-edge case delta has to get right).
fn corpus(n: usize, lines_per_file: usize) -> Vec<File> {
    (0..n)
        .map(|i| {
            let mut text = String::new();
            for j in 0..lines_per_file {
                // most edges unique to this file...
                text.push_str(&format!("f{i}_s{j} f{i}_s{}\n", j + 1));
            }
            // ...plus one edge every file shares, so recompute and delta can disagree.
            text.push_str("shared_root shared_leaf\n");
            File { path: format!("f{i}.rs"), text }
        })
        .collect()
}

fn main() {
    // ---- Finding 1: full recompute is O(corpus) per edit --------------------
    println!("== Finding 1: full recompute cost is independent of change size ==");
    for &n in &[1_000usize, 10_000, 100_000] {
        let files = corpus(n, 8);
        let t = Instant::now();
        let edges = derive_family_batch(&files);
        let build = t.elapsed();

        // now "edit" ONE file and ask the SAME function for the answer again.
        let mut files2 = files;
        files2[0] = File { path: "f0.rs".into(), text: "f0_s0 f0_changed\nshared_root shared_leaf\n".into() };
        let t = Instant::now();
        let _ = derive_family_batch(&files2);
        let one_edit = t.elapsed();

        println!(
            "  n={:>7} files: build {:>8.2?} ({} edges) | one-file edit re-derive {:>8.2?}  <- same order",
            n, build, edges.len(), one_edit
        );
    }

    // ---- Finding 2: naive-set delta is WRONG on a shared edge ----------------
    println!("\n== Finding 2: naive-set delta corrupts shared facts ==");
    let a = File { path: "a.rs".into(), text: "shared_root shared_leaf\n".into() };
    let b = File { path: "b.rs".into(), text: "shared_root shared_leaf\n".into() };
    let shared = Edge { from: "shared_root".into(), to: "shared_leaf".into() };

    let mut naive = FamilyNaiveSet::default();
    naive.upsert(&a);
    naive.upsert(&b); // both assert `shared_root -> shared_leaf`
    // now `a` changes and no longer emits the shared edge:
    let a2 = File { path: "a.rs".into(), text: "shared_root other_leaf\n".into() };
    naive.upsert(&a2);
    let naive_has = naive.edges.contains(&shared);
    println!("  after a stops emitting it, naive set still has shared edge? {naive_has}  (b still asserts it -> should be TRUE)");
    assert!(!naive_has, "demonstrating the bug: naive delta WRONGLY dropped it");
    println!("  -> naive-set delta returned WRONG (dropped a fact b still asserts).");

    // ---- Finding 3: Z-set delta is correct AND cheap -------------------------
    println!("\n== Finding 3: Z-set (weighted) delta is correct and O(changed file) ==");
    let mut z = FamilyZSet::default();
    z.upsert(&a);
    z.upsert(&b);
    z.upsert(&a2); // a drops the shared edge; b still holds it (weight 2 -> 1)
    let z_has = z.edges().contains(&shared);
    println!("  after a stops emitting it, Z-set still has shared edge? {z_has}  (correct: b holds it)");
    assert!(z_has, "Z-set must keep an edge until its last source drops");

    // cost: rebuild the same 100k-file corpus incrementally, then time ONE edit.
    let files = corpus(100_000, 8);
    let mut z = FamilyZSet::default();
    for f in &files {
        z.upsert(f);
    }
    let edit = File { path: "f0.rs".into(), text: "f0_s0 f0_changed\nshared_root shared_leaf\n".into() };
    let t = Instant::now();
    z.upsert(&edit);
    let one_edit = t.elapsed();
    println!(
        "  Z-set delta for one file over a 100k-file corpus: {:>8.2?}  ({} live edges)",
        one_edit,
        z.edges().len()
    );

    println!(
        "\nVerdict: the family FULL RECOMPUTE is the terrible part (O(corpus)/edit). The fix\n\
         is a delta, and the delta that is CORRECT is a Z-set weight map — the store lab's\n\
         cascade. It is owned, batch-per-tick, no rxRust. Streaming the rows does not appear."
    );
}
