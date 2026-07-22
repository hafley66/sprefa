//! Toy model of the two halves of the v6 runtime, sized so the borrow graph is
//! visible in one screen.
//!
//! CORE  = the batch-relational calculus: a `Family::derive` that reads rows over
//!         BORROWED source text (the ast-grep `Node<'r>` shape) and folds them into
//!         owned facts, fanned out with rayon. This is `_3_extract` / `_5_runtime`
//!         Family::derive in v5 terms.
//! EDGE  = the reactive trigger: watch/clock/git events (all OWNED, `'static`) run
//!         through buffer -> groupBy -> distinctUntilChanged(digest) -> emit-job.
//!         This is the "crude rx" the session wants upgraded to a real pipeline.
//!
//! The claim under test: CORE-as-stream = max pain / low payoff; EDGE-as-stream =
//! bounded pain / max payoff. Each half has a runnable demo; the failing CORE
//! attempt lives in examples/core_frp_attempt.rs behind a feature.

use std::collections::{BTreeMap, BTreeSet, HashMap};

// ===========================================================================
// CORE domain  (borrowed rows — the batch half)
// ===========================================================================

/// A source file. The caller owns the text; everything downstream BORROWS from it.
pub struct File {
    pub path: String,
    pub text: String,
}

/// One extracted fact, zero-copy out of a `File`'s text. This is the ast-grep
/// `Node<'r>` shape: the `&'r str` ties the hit to the buffer it came from. In v5
/// the extraction ops (scan/regex/ast/sg/json) all hand back exactly this.
#[derive(Clone, Copy, Debug)]
pub struct Hit<'r> {
    pub caller: &'r str,
    pub callee: &'r str,
    pub line: u32,
}

/// An OWNED derived fact — the projection boundary. Batch code lands here per-file
/// then rayon-reduces (the rx-x-ast-grep law: "project to owned Hit at the source").
/// Note there is no lifetime here: once you own, the source buffer is free to drop.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

/// Extraction op: borrow `caller callee` pairs out of the file text, one per line.
/// Returns `Hit<'r>` — the return lifetime is welded to `&'r File`. This weld is
/// the whole story: it is trivially fine inside a batch scope and radioactive
/// the moment you try to put it on a stream that outlives the buffer.
pub fn extract(file: &File) -> Vec<Hit<'_>> {
    file.text
        .lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let mut it = raw.split_whitespace();
            let caller = it.next()?;
            let callee = it.next()?;
            Some(Hit { caller, callee, line: i as u32 + 1 })
        })
        .collect()
}

/// Own one file's edges at the source (project the borrowed `Hit<'r>` to `Edge`).
/// Factored out so both the full recompute and the delta path share it.
pub fn edges_of(file: &File) -> Vec<Edge> {
    extract(file)
        .into_iter()
        .map(|hit| Edge { from: hit.caller.to_string(), to: hit.callee.to_string() })
        .collect()
}

/// Family::derive, BATCH form. rayon fans the per-file extract across the pool;
/// each file projects its borrowed hits to owned `Edge`s AT the source (inside the
/// closure, where `&file` is alive), then the results reduce into one owned set.
///
/// The borrowed `Hit<'r>` never leaves the closure. rayon gets a whole `&[File]`
/// slab to steal-schedule. This compiles, runs, and is fast. It is the baseline.
pub fn derive_family_batch(files: &[File]) -> BTreeSet<Edge> {
    use rayon::prelude::*;
    files
        .par_iter()
        // borrow lives and dies inside edges_of; only owned Edges cross the reduce.
        .map(edges_of)
        .flatten()
        .collect()
}

// ===========================================================================
// The break point: full recompute vs delta
// ===========================================================================
//
// `derive_family_batch` re-reads EVERY file to answer any change — O(corpus) per
// edit. That is the terrible part, and it is real. Below are the two ways to make
// it incremental, and the honest result of each: naive-set delta is a CORRECTNESS
// bug; Z-set (weighted) delta is correct and cheap — and is the store lab's weight
// cascade, still owned batch-per-tick, still NOT a stream of rows through rxRust.

/// WRONG delta. Keeps a plain `BTreeSet<Edge>`. On a file change it removes that
/// file's OLD edges and inserts its NEW ones. This double-frees any edge that TWO
/// files both produce: dropping one file deletes a fact the other still asserts.
/// Included to be shown failing.
#[derive(Default)]
pub struct FamilyNaiveSet {
    pub edges: BTreeSet<Edge>,
    per_file: HashMap<String, Vec<Edge>>,
}

impl FamilyNaiveSet {
    pub fn upsert(&mut self, file: &File) {
        if let Some(old) = self.per_file.remove(&file.path) {
            for e in old {
                self.edges.remove(&e); // BUG: e may still be asserted by another file
            }
        }
        let fresh = edges_of(file);
        for e in &fresh {
            self.edges.insert(e.clone());
        }
        self.per_file.insert(file.path.clone(), fresh);
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(old) = self.per_file.remove(path) {
            for e in old {
                self.edges.remove(&e); // same BUG on delete: drops still-asserted edges
            }
        }
    }
}

/// CORRECT delta. Edge -> multiplicity (how many files currently assert it). A
/// shared edge survives until its LAST source drops. This is a Z-set — the store
/// lab's weight cascade in miniature. Delta cost = O(edges in the changed file),
/// independent of corpus size. Owned throughout; no lifetime, no stream, no rayon
/// shattered — just a per-tick map update.
#[derive(Default)]
pub struct FamilyZSet {
    pub weight: BTreeMap<Edge, i64>,
    per_file: HashMap<String, Vec<Edge>>,
}

impl FamilyZSet {
    pub fn upsert(&mut self, file: &File) {
        if let Some(old) = self.per_file.remove(&file.path) {
            for e in old {
                if let Some(w) = self.weight.get_mut(&e) {
                    *w -= 1;
                    if *w == 0 {
                        self.weight.remove(&e);
                    }
                }
            }
        }
        let fresh = edges_of(file);
        for e in &fresh {
            *self.weight.entry(e.clone()).or_default() += 1;
        }
        self.per_file.insert(file.path.clone(), fresh);
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(old) = self.per_file.remove(path) {
            for e in old {
                if let Some(w) = self.weight.get_mut(&e) {
                    *w -= 1;
                    if *w == 0 {
                        self.weight.remove(&e);
                    }
                }
            }
        }
    }

    pub fn edges(&self) -> BTreeSet<Edge> {
        self.weight.keys().cloned().collect()
    }

    /// O(1) live-edge count — materializing `edges()` at 10M just to count is itself
    /// an O(n) trap the scale test must not fall into.
    pub fn live_count(&self) -> usize {
        self.weight.len()
    }
}

// ===========================================================================
// EDGE domain  (owned events — the reactive half)
// ===========================================================================

/// A source event. Every variant is OWNED and `'static`: paths, digests, refs.
/// Nothing borrows a file buffer, so these ride any stream, any scheduler, freely.
#[derive(Clone, Debug)]
pub enum Event {
    /// A watcher saw a path change. `digest` is content hash = the distinct key.
    FileChanged { path: String, digest: u64 },
    /// A clock/interval boundary (ghcacher's `clock(5,_)`) — flushes the buffer.
    Tick,
    /// git HEAD moved. Owned ref string.
    GitHead(String),
}

/// Which derive family a path routes to (v5 stratifies by family so ast-grep can
/// max out rayon). Toy routing: by extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    Rust,
    Ts,
    Other,
}

pub fn family_of(path: &str) -> Family {
    match path.rsplit('.').next() {
        Some("rs") => Family::Rust,
        Some("ts") | Some("tsx") => Family::Ts,
        _ => Family::Other,
    }
}

/// The unit of work the trigger hands to the core: "re-derive this family over
/// this coalesced path set." One job per family per tick — never per row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeriveJob {
    pub family: Family,
    pub paths: Vec<String>,
    pub head: Option<String>,
}

/// The EDGE trigger, as a real `futures::Stream` pipeline. This is the upgrade the
/// session called for: not "groupBy + immediate rerun" but the proper formula
///
///   events
///     -> buffer(tick)                 // ready_chunks split on Tick
///     -> groupBy(family)              // HashMap<Family, set>
///     -> distinctUntilChanged(digest) // drop a path whose content digest is unchanged
///     -> mergeMap(emit DeriveJob)     // one job per family, owned paths
///
/// Every operator works on OWNED events, so the whole graph is `'static + Send`:
/// no lifetime rides along, no clone-to-satisfy-the-checker, no rayon shattered.
/// The core never appears here — the trigger only decides WHAT to derive and WHEN.
pub async fn run_trigger<S>(mut events: S) -> Vec<DeriveJob>
where
    S: futures::Stream<Item = Event> + Unpin,
{
    use futures::StreamExt;

    // distinctUntilChanged(digest): resident last-seen content hash per path.
    let mut last_digest: HashMap<String, u64> = HashMap::new();
    // groupBy(family): the current tick's coalesced buffer. BTreeMap so the
    // per-tick flush order is deterministic (Family's Ord), not hash-random.
    let mut pending: BTreeMap<Family, BTreeSet<String>> = BTreeMap::new();
    let mut head: Option<String> = None;
    let mut jobs: Vec<DeriveJob> = Vec::new();

    while let Some(ev) = events.next().await {
        match ev {
            Event::FileChanged { path, digest } => {
                // distinctUntilChanged: unchanged content = no-op, the buffer never
                // grows, the core never wakes. (Docker-layer skip at the edge.)
                if last_digest.get(&path) == Some(&digest) {
                    continue;
                }
                last_digest.insert(path.clone(), digest);
                pending.entry(family_of(&path)).or_default().insert(path);
            }
            Event::GitHead(ref_) => head = Some(ref_),
            // buffer(tick): the clock boundary flushes the coalesced buffer into
            // one job per touched family. mergeMap over the grouped families.
            Event::Tick => {
                for (family, paths) in std::mem::take(&mut pending) {
                    jobs.push(DeriveJob {
                        family,
                        paths: paths.into_iter().collect(),
                        head: head.clone(),
                    });
                }
            }
        }
    }
    jobs
}
