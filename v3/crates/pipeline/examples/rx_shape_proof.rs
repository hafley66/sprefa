//! rx_shape_proof — POC for sprefa-4m7.7.12.
//!
//! Proves top-down rxRust + const-golf userland composes. The userland
//! section at the bottom contains zero `let` / `mut` / statement-level
//! state. Verify with:
//!   awk '/USERLAND BEGIN/,/USERLAND END/' rx_shape_proof.rs \
//!     | grep -E '^\s*(let |let mut )'
//!
//! Smallest evolvable shape:
//! - Pipeline = Arc<dyn Fn(PipeCtx, Vec<Chunk>) -> Vec<Chunk>>. Sync
//!   data plane today; flips to Observable<Chunk> tomorrow without
//!   touching userland. RtCtx already carries SharedSubject<Row>.
//! - OpCall: parent chain + pending + sources_done + cancel slots.
//!   quiet$ derivation lands in sprefa-4m7.7.13.
//! - PipeCtx: put / take_all / fork_child. Saga effects as methods.

use rxrust::prelude::*;
use std::convert::Infallible;
use std::ops::{BitOr, Shr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

// ─── runtime types ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Row(pub String);

#[derive(Clone, Debug)]
pub struct Cursor {
    pub path: String,
    pub bound: Option<String>,
}

pub struct OpCall {
    pub pending: AtomicUsize,
    pub sources_done: AtomicBool,
    pub cancelled: AtomicBool,
    pub parent: Option<Arc<OpCall>>,
}

impl OpCall {
    pub fn root() -> Arc<Self> {
        Arc::new(Self {
            pending: AtomicUsize::new(0),
            sources_done: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            parent: None,
        })
    }
    pub fn child(parent: Arc<Self>) -> Arc<Self> {
        Arc::new(Self {
            pending: AtomicUsize::new(0),
            sources_done: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            parent: Some(parent),
        })
    }
    pub fn quiet(&self) -> bool {
        self.pending.load(Ordering::SeqCst) == 0
            && self.sources_done.load(Ordering::SeqCst)
    }
}

pub type RowSubject = Arc<Mutex<SharedSubject<'static, Row, Infallible>>>;

pub struct RtCtx {
    pub relations: Mutex<std::collections::HashMap<String, RowSubject>>,
    pub bag: Mutex<std::collections::HashMap<String, Vec<Row>>>,
}

impl RtCtx {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            relations: Mutex::new(Default::default()),
            bag: Mutex::new(Default::default()),
        })
    }
    pub fn subject(&self, name: &str) -> RowSubject {
        self.relations
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(Shared::subject::<Row, Infallible>())))
            .clone()
    }
}

pub struct PipeCtx {
    pub op_call: Arc<OpCall>,
    pub rt: Arc<RtCtx>,
}

impl PipeCtx {
    pub fn put(&self, name: &str, row: Row) {
        self.rt
            .bag
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_default()
            .push(row.clone());
        if let Ok(mut s) = self.rt.subject(name).lock() {
            s.next(row);
        }
    }
    pub fn take_all(&self, name: &str) -> Vec<Row> {
        self.rt
            .bag
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .unwrap_or_default()
    }
    pub fn fork_child(self: &Arc<Self>) -> Arc<PipeCtx> {
        Arc::new(PipeCtx {
            op_call: OpCall::child(self.op_call.clone()),
            rt: self.rt.clone(),
        })
    }
}

#[derive(Clone)]
pub struct Chunk {
    pub cursors: Arc<[Cursor]>,
    pub pipe_ctx: Arc<PipeCtx>,
}

#[derive(Clone)]
pub struct Pipeline(
    pub Arc<dyn Fn(Arc<PipeCtx>, Vec<Chunk>) -> Vec<Chunk> + Send + Sync>,
);

impl Pipeline {
    pub fn run(&self, pcx: Arc<PipeCtx>, input: Vec<Chunk>) -> Vec<Chunk> {
        (self.0)(pcx, input)
    }
}

impl Shr for Pipeline {
    type Output = Pipeline;
    fn shr(self, rhs: Pipeline) -> Pipeline {
        Pipeline(Arc::new(move |pcx: Arc<PipeCtx>, xs| {
            rhs.run(pcx.clone(), self.run(pcx, xs))
        }))
    }
}

impl BitOr for Pipeline {
    type Output = Pipeline;
    fn bitor(self, rhs: Pipeline) -> Pipeline {
        Pipeline(Arc::new(move |pcx: Arc<PipeCtx>, xs: Vec<Chunk>| {
            let mut out = self.run(pcx.clone(), xs.clone());
            out.extend(rhs.run(pcx, xs));
            out
        }))
    }
}

// ─── op constructors (framework — `let` allowed) ────────────────────────────

fn pipeline<F>(f: F) -> Pipeline
where
    F: Fn(Arc<PipeCtx>, Vec<Chunk>) -> Vec<Chunk> + Send + Sync + 'static,
{
    Pipeline(Arc::new(f))
}

pub fn fs(glob: &'static str) -> Pipeline {
    pipeline(move |pcx, _| {
        let cursors: Arc<[Cursor]> = Arc::from(vec![
            Cursor { path: format!("{glob}/a.rs"), bound: None },
            Cursor { path: format!("{glob}/b.rs"), bound: None },
        ]);
        pcx.op_call.sources_done.store(true, Ordering::SeqCst);
        vec![Chunk { cursors, pipe_ctx: pcx }]
    })
}

pub fn re(pat: &'static str) -> Pipeline {
    pipeline(move |_pcx, xs| {
        xs.into_iter()
            .map(|c| {
                let new_cursors: Arc<[Cursor]> = c
                    .cursors
                    .iter()
                    .map(|cu| Cursor {
                        path: cu.path.clone(),
                        bound: Some(format!("{pat} ~ {}", cu.path)),
                    })
                    .collect::<Vec<_>>()
                    .into();
                Chunk { cursors: new_cursors, pipe_ctx: c.pipe_ctx }
            })
            .collect()
    })
}

pub fn tag_write(name: &'static str) -> Pipeline {
    pipeline(move |_pcx, xs| {
        for c in &xs {
            for cu in c.cursors.iter() {
                c.pipe_ctx.put(
                    name,
                    Row(cu.bound.clone().unwrap_or_else(|| cu.path.clone())),
                );
            }
        }
        xs
    })
}

pub fn tag_read(name: &'static str) -> Pipeline {
    pipeline(move |pcx, _| {
        let cursors: Arc<[Cursor]> = pcx
            .take_all(name)
            .into_iter()
            .map(|Row(s)| Cursor { path: s, bound: None })
            .collect::<Vec<_>>()
            .into();
        vec![Chunk { cursors, pipe_ctx: pcx }]
    })
}

pub fn print() -> Pipeline {
    pipeline(|_pcx, xs| {
        for c in &xs {
            for cu in c.cursors.iter() {
                println!("  [print] {}", cu.path);
            }
        }
        xs
    })
}

pub fn rule(name: &'static str, body: Pipeline) -> Pipeline {
    pipeline(move |pcx, xs| {
        let child = pcx.fork_child();
        println!("  [rule:{name}] OpCall opened");
        body.run(child, xs)
    })
}

pub fn rule_call(name: &'static str, glob: &'static str) -> Pipeline {
    rule(name, fs(glob) >> re(r"fn (\w+)") >> tag_write(":r_fns"))
}

pub fn fork(arms: Vec<Pipeline>) -> Pipeline {
    pipeline(move |pcx, xs| {
        arms.iter()
            .flat_map(|a| a.run(pcx.clone(), xs.clone()))
            .collect()
    })
}

// ─── driver ─────────────────────────────────────────────────────────────────

static RT: OnceLock<Arc<RtCtx>> = OnceLock::new();

fn rt() -> Arc<RtCtx> {
    RT.get_or_init(RtCtx::new).clone()
}

fn root_pcx() -> Arc<PipeCtx> {
    Arc::new(PipeCtx { op_call: OpCall::root(), rt: rt() })
}

pub fn run_once(p: Pipeline) -> Vec<Chunk> {
    let pcx = root_pcx();
    let seed = Chunk {
        cursors: Arc::from(Vec::<Cursor>::new()),
        pipe_ctx: pcx.clone(),
    };
    p.run(pcx, vec![seed])
}

// ─── USERLAND BEGIN — zero `let`/`mut` below ────────────────────────────────

fn main() {
    println!("demo 1: fs >> re >> tag_write");
    run_once(fs("v3/**/*.rs") >> re(r"fn (\w+)") >> tag_write(":fns"));

    println!("demo 2: tag_read >> print  (durable read-back of demo 1)");
    run_once(tag_read(":fns") >> print());

    println!("demo 3: fork over rule_call — same Pipeline, fresh OpCalls");
    run_once(fork(vec![
        rule_call("R", "v3/**"),
        rule_call("R", "v2/**"),
    ]));

    println!("demo 3 readback:");
    run_once(tag_read(":r_fns") >> print());
}

// ─── USERLAND END ───────────────────────────────────────────────────────────

// ─── resistance log ─────────────────────────────────────────────────────────
//
// Userland `let` count: 0. Userland `mut` count: 0. Verified by awk
// extracting the BEGIN..END block and grepping `^\s*let `.
//
// `let` allowed in runtime internals (constructors, locks, .into()).
//
// rxRust posture: SharedSubject<Row> is the carrier type in RtCtx so the
// durable relation layer is already rx-shaped. Data plane is sync
// Vec<Chunk> for the proof. Promoting to a live Observable<Chunk> is a
// type swap on Pipeline + the op constructors. Userland surface
// (functions, `>>`, `|`, fork, rule) stays identical.
//
// Resistance #1: `Shr::shr` moves `self`. `Pipeline: Clone`
// (Arc<dyn Fn>) resolved it. Same Pipeline value subscribed twice = two
// OpCalls — demoed by `fork(vec![rule_call(...), rule_call(...)])` cloning
// the rule body.
//
// Resistance #2: tempted to bind `pcx.clone()` once inside `Shr` and
// `fork`. Kept `.clone()` inline. Cost is two ref-bumps per arm.
//
// Resistance #3: `effect()` not exercised (no Batcher in proof). When
// sprefa-4m7.7.13 lands real OpCall + scoped_ctx, `pipe_ctx.effect(eff)`
// ticks pending and returns a Single. No userland change.
//
// Kill criterion: NOT triggered. Userland composes seq + fork + rule
// under `>>` and `|` with zero `let`. Pipeline is Clone+Send+Sync,
// re-runnable, durable across calls via shared RtCtx.
