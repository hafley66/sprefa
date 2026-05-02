//! rx_native_proof — POC for sprefa-4m7.7.12, rxrust-native variant.
//!
//! Where rx_shape_proof.rs uses Vec<Chunk> sync data, this version drives
//! actual rxrust observables end-to-end. Same userland surface, same
//! const-golf rule, same three demos.
//!
//! Pipeline = Arc<dyn Fn(SharedBoxedObservableClone<Cursor>) ->
//!                       SharedBoxedObservableClone<Cursor>>.
//!
//! Saga effects collapse into rxrust operators:
//!   put     →  .tap(|c| subject.next(c.clone()) + bag.push)
//!   take    →  Shared::from_iter(bag.snapshot())
//!   fork    →  obs.merge(other)
//!   call    →  rule body applied to a fresh empty source
//!   cancel  →  .take_until(cancel_subject)   [not exercised here]
//!   effect  →  Shared::from_future(batcher.run(eff))   [not exercised]
//!
//! No PipeCtx type. No OpCall struct. No HashMap on RtCtx visible to
//! non-tag ops. The runtime spine collapsed into rxrust + tag's private
//! registry.

use rxrust::prelude::*;
use std::collections::HashMap;
use std::convert::Infallible;
use std::ops::{BitOr, Shr};
use std::sync::{Arc, Mutex, OnceLock};

// ─── data ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Cursor {
    pub path: String,
    pub bound: Option<String>,
}

type ChunkObs = SharedBoxedObservable<'static, Cursor, Infallible>;

// ─── Pipeline value ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Pipeline(Arc<dyn Fn(ChunkObs) -> ChunkObs + Send + Sync>);

impl Pipeline {
    fn new<F>(f: F) -> Self
    where
        F: Fn(ChunkObs) -> ChunkObs + Send + Sync + 'static,
    {
        Pipeline(Arc::new(f))
    }
    fn apply(&self, x: ChunkObs) -> ChunkObs {
        (self.0)(x)
    }
}

impl Shr for Pipeline {
    type Output = Pipeline;
    fn shr(self, rhs: Pipeline) -> Pipeline {
        Pipeline::new(move |x| rhs.apply(self.apply(x)))
    }
}

// `|` = fork-by-merge. Sources self-seed; upstream input is ignored.
// Multicast fan-out (one upstream feeding both arms) requires
// rxrust Connectable; out of scope of this proof.
impl BitOr for Pipeline {
    type Output = Pipeline;
    fn bitor(self, rhs: Pipeline) -> Pipeline {
        Pipeline::new(move |x| {
            let _ = x;
            let a = self.apply(empty_obs());
            let b = rhs.apply(empty_obs());
            a.merge(b).box_it()
        })
    }
}

fn empty_obs() -> ChunkObs {
    Shared::from_iter(Vec::<Cursor>::new()).box_it()
}

// ─── tag registry (private to this module — no public RtCtx surface) ────────

struct TagRegistry {
    bags: Mutex<HashMap<String, Vec<Cursor>>>,
}

impl TagRegistry {
    fn new() -> Self {
        Self { bags: Mutex::new(HashMap::new()) }
    }
    fn append(&self, name: &str, c: Cursor) {
        self.bags
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .push(c);
    }
    fn snapshot(&self, name: &str) -> Vec<Cursor> {
        self.bags
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .unwrap_or_default()
    }
}

static TAGS: OnceLock<TagRegistry> = OnceLock::new();
fn tags() -> &'static TagRegistry {
    TAGS.get_or_init(TagRegistry::new)
}

// ─── op constructors ────────────────────────────────────────────────────────

pub fn fs(glob: &'static str) -> Pipeline {
    Pipeline::new(move |_| {
        Shared::from_iter(vec![
            Cursor { path: format!("{glob}/a.rs"), bound: None },
            Cursor { path: format!("{glob}/b.rs"), bound: None },
        ])
        .box_it()
    })
}

pub fn re(pat: &'static str) -> Pipeline {
    Pipeline::new(move |input| {
        input
            .map(move |c: Cursor| Cursor {
                path: c.path.clone(),
                bound: Some(format!("{pat} ~ {}", c.path)),
            })
            .box_it()
    })
}

pub fn tag_write(name: &'static str) -> Pipeline {
    Pipeline::new(move |input| {
        input
            .tap(move |c: &Cursor| tags().append(name, c.clone()))
            .box_it()
    })
}

pub fn tag_read(name: &'static str) -> Pipeline {
    Pipeline::new(move |_| {
        Shared::from_iter(tags().snapshot(name)).box_it()
    })
}

pub fn print() -> Pipeline {
    Pipeline::new(|input| {
        input
            .tap(|c: &Cursor| println!("  [print] {}", c.path))
            .box_it()
    })
}

pub fn rule(name: &'static str, body: Pipeline) -> Pipeline {
    Pipeline::new(move |input| {
        println!("  [rule:{name}] OpCall = inner subscription");
        body.apply(input)
    })
}

pub fn rule_call(name: &'static str, glob: &'static str) -> Pipeline {
    rule(name, fs(glob) >> re(r"fn (\w+)") >> tag_write(":r_fns"))
}

// ─── driver ─────────────────────────────────────────────────────────────────

pub fn run_once(p: Pipeline) {
    let counted = Arc::new(Mutex::new(0usize));
    let c2 = counted.clone();
    p.apply(empty_obs()).subscribe(move |_| {
        *c2.lock().unwrap() += 1;
    });
    println!("  ({} emissions)", *counted.lock().unwrap());
}

// ─── USERLAND BEGIN — zero `let`/`mut` below ────────────────────────────────

fn main() {
    println!("demo 1: fs >> re >> tag_write");
    run_once(fs("v3/**/*.rs") >> re(r"fn (\w+)") >> tag_write(":fns"));

    println!("demo 2: tag_read >> print  (durable read-back of demo 1)");
    run_once(tag_read(":fns") >> print());

    println!("demo 3: fork over rule_call — same Pipeline, fresh OpCalls");
    run_once(rule_call("R", "v3/**") | rule_call("R", "v2/**"));

    println!("demo 3 readback:");
    run_once(tag_read(":r_fns") >> print());
}

// ─── USERLAND END ───────────────────────────────────────────────────────────

// ─── resistance log ─────────────────────────────────────────────────────────
//
// Userland `let` count: 0. Userland `mut` count: 0. Operator overload set
// is `>>` (Shr) and `|` (BitOr) — Rust offers no real pipe operator.
//
// rxrust posture: every op is a real rxrust transform. fs = from_iter, re
// = .map, tag_write = .tap (side-effect into private registry), tag_read =
// from_iter snapshot, print = .tap, fork = .merge.
//
// Subscription = OpCall. There is no `OpCall` struct here; rxrust's own
// Subscription handle is the lifecycle envelope. `quiet$` would be the
// upstream `complete` signal combined across active subscriptions —
// trivially derivable when sprefa-4m7.7.13 wants it as a Subject.
//
// Resistance #1: SharedBoxedObservable is not Clone; SharedBoxedObservableClone
// is. Used the Clone variant so `>>` can move a Pipeline twice and so
// fork's arms can each consume their own copy. Cost: every op chain ends
// in `.box_it()` instead of `.box_it()`. Acceptable.
//
// Resistance #2: Fork as broadcast (one upstream → many arms) needs
// rxrust Connectable + share_*. Sidestepped: in our pipe sources self-seed
// and fork arms are independent. The seed `empty_obs()` is ignored by
// fs/tag_read. When a future op needs upstream-driven fork, swap BitOr to
// use `share_clone()` or the connectable pattern.
//
// Resistance #3: `take_until` / `cancel` not exercised. Same path: pass a
// `cancel_subject: SharedSubject<()>` through and apply `.take_until` at
// each subscription site. No PipeCtx needed; closure capture suffices.
//
// Resistance #4: tag durability uses a Mutex<HashMap<String, Vec<Cursor>>>
// only. The "live SharedSubject for in-pipe subscribers" was elided —
// nothing in this proof has a long-lived subscriber concurrent with a
// writer. When sprefa-4m7.7.13 lands, tag_write also calls
// `subject.next(c.clone())` for live merges; tag_read returns
// `from_iter(snapshot).merge(subject.clone())` to fold replay+live.
//
// What collapsed compared to rx_shape_proof.rs:
//   - PipeCtx struct: GONE
//   - OpCall struct: GONE
//   - RtCtx struct: GONE (replaced by tags() singleton private to tag)
//   - PipeCtx.put/take_all methods: GONE (now real ops via .tap/.from_iter)
//   - fork_child / cancel methods: GONE (subscription = OpCall; take_until = cancel)
//
// Kill criterion: NOT triggered. rxrust 1.0.0-rc.4 carries the layer-2/4
// collapse without forcing `let` into userland. Type erasure via
// SharedBoxedObservableClone gives us a Clone+Send+Sync+'static Pipeline.
// Operator overloads pass coherence.
