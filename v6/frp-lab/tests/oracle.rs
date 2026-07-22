//! The oracle I was missing. Hand-picked asserts ("this shared edge survives") are
//! not a spec — they only test cases I already thought of. The real spec for an
//! incremental engine is DIFFERENTIAL:
//!
//!   for every reachable corpus state S, incremental.edges() == batch_recompute(S)
//!
//! i.e. the delta engine must AT ALL TIMES equal a from-scratch recompute of the
//! same files. This is the store lab's "4-engine byte-identical" check. Below: drive
//! a random edit stream (add / modify / delete over a small path+symbol pool so
//! shared edges and multiplicities actually occur) and cross-check after EVERY step.
//!
//! Two tests, and the second is the point: it proves the oracle has TEETH by showing
//! the naive-set delta fails it on its own, with no case hand-crafted by me.

use frp_lab::{derive_family_batch, Edge, FamilyNaiveSet, FamilyZSet, File};
use std::collections::{BTreeSet, HashMap};

/// The one operation set an incremental engine must support to be cross-checkable.
trait IncEngine {
    fn upsert(&mut self, file: &File);
    fn remove(&mut self, path: &str);
    fn edges(&self) -> BTreeSet<Edge>;
}
impl IncEngine for FamilyZSet {
    fn upsert(&mut self, file: &File) {
        FamilyZSet::upsert(self, file)
    }
    fn remove(&mut self, path: &str) {
        FamilyZSet::remove(self, path)
    }
    fn edges(&self) -> BTreeSet<Edge> {
        FamilyZSet::edges(self)
    }
}
impl IncEngine for FamilyNaiveSet {
    fn upsert(&mut self, file: &File) {
        FamilyNaiveSet::upsert(self, file)
    }
    fn remove(&mut self, path: &str) {
        FamilyNaiveSet::remove(self, path)
    }
    fn edges(&self) -> BTreeSet<Edge> {
        self.edges.clone()
    }
}

/// Deterministic PRNG (LCG) so a failure is reproducible from the seed — no dep.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

/// A file body drawn from a SMALL symbol pool, so different files routinely emit the
/// SAME edge (the multiplicity case a plain set gets wrong). 0..3 lines.
fn random_body(rng: &mut Lcg) -> String {
    const SYMS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
    let n = rng.next() % 4;
    let mut text = String::new();
    for _ in 0..n {
        let caller = SYMS[(rng.next() % 6) as usize];
        let callee = SYMS[(rng.next() % 6) as usize];
        text.push_str(caller);
        text.push(' ');
        text.push_str(callee);
        text.push('\n');
    }
    text
}

/// The ORACLE: ground truth is a from-scratch recompute over the current corpus.
fn batch_recompute(truth: &HashMap<String, String>) -> BTreeSet<Edge> {
    let files: Vec<File> =
        truth.iter().map(|(p, t)| File { path: p.clone(), text: t.clone() }).collect();
    derive_family_batch(&files)
}

/// Drive one random edit stream through `eng`, cross-checking against the oracle after
/// every step. Returns the step of first divergence, or None if it tracked for all.
fn run_stream<E: IncEngine>(eng: &mut E, seed: u64, steps: usize) -> Option<usize> {
    let mut rng = Lcg::new(seed);
    let mut truth: HashMap<String, String> = HashMap::new();
    for step in 0..steps {
        // small path pool -> repeated files, deletes hit live paths, edges overlap.
        let path = format!("f{}.rs", rng.next() % 12);
        let delete = rng.next() % 4 == 0 && truth.contains_key(&path);
        if delete {
            truth.remove(&path);
            eng.remove(&path);
        } else {
            let text = random_body(&mut rng);
            truth.insert(path.clone(), text.clone());
            eng.upsert(&File { path, text });
        }
        if eng.edges() != batch_recompute(&truth) {
            return Some(step);
        }
    }
    None
}

#[test]
fn zset_equals_batch_oracle_over_random_edits() {
    // The Z-set delta must match the from-scratch oracle at every step, across many
    // independent random streams. If it ever diverges, the seed+step localizes it.
    for seed in 0..50u64 {
        let mut z = FamilyZSet::default();
        let diverged = run_stream(&mut z, seed, 400);
        assert_eq!(diverged, None, "Z-set diverged from oracle on seed {seed}");
    }
}

#[test]
fn naive_set_is_caught_by_the_oracle() {
    // The point of the oracle: it flags the naive delta WITHOUT me hand-crafting the
    // failing case. Across 50 streams it must diverge at least once (in practice fast,
    // as soon as two live files share an edge and one is removed/modified).
    let mut first: Option<(u64, usize)> = None;
    for seed in 0..50u64 {
        let mut naive = FamilyNaiveSet::default();
        if let Some(step) = run_stream(&mut naive, seed, 400) {
            first.get_or_insert((seed, step));
        }
    }
    assert!(first.is_some(), "oracle has no teeth: naive delta was never caught");
    eprintln!("oracle caught the naive delta first at seed/step {:?}", first.unwrap());
}
