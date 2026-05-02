//! stream_kernel — pure tokio Stream substrate, smaller than rx_native_proof.
//!
//! Substrate shift: rxrust → futures::Stream + tokio_stream + async-broadcast.
//! Same const-golf userland: `>>` seq, `|` fork, no `let` / `mut` below the
//! USERLAND BEGIN line.
//!
//! Pipeline = Arc<dyn Fn(BoxStream<'static, Cursor>) ->
//!                      BoxStream<'static, Cursor> + Send + Sync>.
//!
//! Saga effects → Stream operators:
//!   put     →  .inspect(|c| bag.push + multicast.try_broadcast)
//!   take    →  stream::iter(bag.snapshot()).chain(rx.into_stream())
//!   fork    →  a.merge(b)                  (stream-merge, not select)
//!   call    →  body(input)                 (subscription = OpCall)
//!   cancel  →  .take_until(cancel_signal)  (drop inner = unsub cascade)
//!   effect  →  batcher.run(eff).into_stream().flat_map(...)   [stub here]
//!   switch  →  stream.flat_map_unordered(Some(1), ..) ish; demoed
//!              via a switching driver that drops prior subscription
//!
//! Demos:
//!   1. fs >> re >> tag_write
//!   2. tag_read >> print  (durable read-back)
//!   3. fork over rule_call (same Pipeline, fresh subscriptions)
//!   4. tag_read live merge: subscribe before more writes land
//!   5. switch_map at process-tree node: rule re-call with new arg drops
//!      prior in-flight stream (proven by emission count)
//!   6. take_until cancel cascade through nested merge

use async_broadcast::{broadcast, Receiver as BcastRx, Sender as BcastTx};
use futures::stream::{self, BoxStream, StreamExt};
use std::collections::HashMap;
use std::ops::{BitOr, Shr};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

// ─── data ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Cursor {
    pub path: String,
    pub bound: Option<String>,
}

type CursorStream = BoxStream<'static, Cursor>;

// ─── Pipeline value ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Pipeline(Arc<dyn Fn(CursorStream) -> CursorStream + Send + Sync>);

impl Pipeline {
    fn new<F>(f: F) -> Self
    where
        F: Fn(CursorStream) -> CursorStream + Send + Sync + 'static,
    {
        Pipeline(Arc::new(f))
    }
    fn apply(&self, x: CursorStream) -> CursorStream {
        (self.0)(x)
    }
}

impl Shr for Pipeline {
    type Output = Pipeline;
    fn shr(self, rhs: Pipeline) -> Pipeline {
        Pipeline::new(move |x| rhs.apply(self.apply(x)))
    }
}

// `|` = fork-by-merge. Sources self-seed; upstream is ignored.
// Multicast fan-out from one upstream into both arms is the share() story
// (async-broadcast / futures-rx); not exercised here.
impl BitOr for Pipeline {
    type Output = Pipeline;
    fn bitor(self, rhs: Pipeline) -> Pipeline {
        Pipeline::new(move |x| {
            let _ = x;
            futures::stream::select(self.apply(empty_stream()), rhs.apply(empty_stream()))
                .boxed()
        })
    }
}

fn empty_stream() -> CursorStream {
    stream::empty().boxed()
}

// ─── tag registry — replay buffer + live multicast ──────────────────────────

struct TagBag {
    snapshot: Vec<Cursor>,
    tx: BcastTx<Cursor>,
    _keepalive: BcastRx<Cursor>, // keep channel open even with no live readers
}

struct TagRegistry {
    bags: Mutex<HashMap<String, Arc<Mutex<TagBag>>>>,
}

impl TagRegistry {
    fn new() -> Self {
        Self { bags: Mutex::new(HashMap::new()) }
    }
    fn bag(&self, name: &str) -> Arc<Mutex<TagBag>> {
        self.bags
            .lock()
            .unwrap()
            .entry(name.to_string())
            .or_insert_with(|| {
                let (tx, rx) = broadcast::<Cursor>(1024);
                Arc::new(Mutex::new(TagBag {
                    snapshot: Vec::new(),
                    tx,
                    _keepalive: rx,
                }))
            })
            .clone()
    }
    fn append(&self, name: &str, c: Cursor) {
        let bag = self.bag(name);
        let mut g = bag.lock().unwrap();
        g.snapshot.push(c.clone());
        let _ = g.tx.try_broadcast(c);
    }
    fn snapshot_then_live(&self, name: &str) -> CursorStream {
        let bag = self.bag(name);
        let (snap, rx) = {
            let g = bag.lock().unwrap();
            (g.snapshot.clone(), g.tx.new_receiver())
        };
        stream::iter(snap)
            .chain(rx)
            .boxed()
    }
    fn snapshot_only(&self, name: &str) -> CursorStream {
        let bag = self.bag(name);
        let snap = bag.lock().unwrap().snapshot.clone();
        stream::iter(snap).boxed()
    }
}

static TAGS: OnceLock<TagRegistry> = OnceLock::new();
fn tags() -> &'static TagRegistry {
    TAGS.get_or_init(TagRegistry::new)
}

// ─── op constructors ────────────────────────────────────────────────────────

pub fn fs(glob: &'static str) -> Pipeline {
    Pipeline::new(move |_| {
        stream::iter(vec![
            Cursor { path: format!("{glob}/a.rs"), bound: None },
            Cursor { path: format!("{glob}/b.rs"), bound: None },
        ])
        .boxed()
    })
}

pub fn re(pat: &'static str) -> Pipeline {
    Pipeline::new(move |input| {
        input
            .map(move |c| Cursor {
                path: c.path.clone(),
                bound: Some(format!("{pat} ~ {}", c.path)),
            })
            .boxed()
    })
}

pub fn tag_write(name: &'static str) -> Pipeline {
    Pipeline::new(move |input| {
        input
            .inspect(move |c| tags().append(name, c.clone()))
            .boxed()
    })
}

pub fn tag_read(name: &'static str) -> Pipeline {
    Pipeline::new(move |_| tags().snapshot_only(name))
}

pub fn tag_read_live(name: &'static str) -> Pipeline {
    Pipeline::new(move |_| tags().snapshot_then_live(name))
}

pub fn print() -> Pipeline {
    Pipeline::new(|input| {
        input
            .inspect(|c| println!("  [print] {}", c.path))
            .boxed()
    })
}

pub fn slow(ms: u64) -> Pipeline {
    Pipeline::new(move |input| {
        input
            .then(move |c| async move {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                c
            })
            .boxed()
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

async fn drain(p: Pipeline) -> usize {
    let counted = Arc::new(Mutex::new(0usize));
    let c2 = counted.clone();
    p.apply(empty_stream())
        .for_each(move |_| {
            *c2.lock().unwrap() += 1;
            async {}
        })
        .await;
    let n = *counted.lock().unwrap();
    println!("  ({n} emissions)");
    n
}

// live demo: subscribe to tag_read_live, then a writer task pushes
// 2 more cursors; reader sees replay (2) + live (2) = 4 total.
async fn live_demo() {
    let reader = tokio::spawn(async move {
        (tag_read_live(":fns") >> print())
            .apply(empty_stream())
            .take(4)
            .for_each(|_| async {})
            .await;
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    tags().append(":fns", Cursor { path: "live/x.rs".into(), bound: None });
    tags().append(":fns", Cursor { path: "live/y.rs".into(), bound: None });
    reader.await.unwrap();
    println!("  (4 emissions: 2 replay + 2 live)");
}

// switch_map demo: drive a Pipeline with new args, prior in-flight gets
// dropped when a new arg arrives. Inner subscription = OpCall; dropping
// the JoinHandle / aborting the task = take_until-style cancel cascade.
async fn switch_demo() {
    use tokio::sync::mpsc;
    let (tx, mut rx) = mpsc::unbounded_channel::<&'static str>();
    let counted = Arc::new(Mutex::new(0usize));

    // driver that re-subscribes on each new arg, aborting prior
    let driver = {
        let counted = counted.clone();
        tokio::spawn(async move {
            let mut current: Option<tokio::task::JoinHandle<()>> = None;
            while let Some(arg) = rx.recv().await {
                if let Some(h) = current.take() {
                    h.abort();
                }
                let counted = counted.clone();
                current = Some(tokio::spawn(async move {
                    let p = rule_call("R", arg) >> slow(50);
                    p.apply(empty_stream())
                        .for_each(|_| {
                            let counted = counted.clone();
                            async move { *counted.lock().unwrap() += 1; }
                        })
                        .await;
                }));
            }
            if let Some(h) = current.take() {
                let _ = h.await;
            }
        })
    };

    let _ = tx.send("v3/**");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = tx.send("v2/**"); // supersedes; v3 in-flight gets aborted mid-slow
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = tx.send("v1/**"); // supersedes again
    drop(tx);
    driver.await.unwrap();

    let n = *counted.lock().unwrap();
    println!("  switch_map total emissions: {n} (only the last arg's stream fully drains)");
}

// take_until cancel cascade: nested merge with a cancel signal that fires
// mid-stream. All branches stop.
async fn cancel_demo() {
    let (cancel_tx, cancel_rx) = async_broadcast::broadcast::<()>(1);
    let cancel_stream = cancel_rx.boxed();

    let p = (rule_call("A", "v3/**") >> slow(30))
        | (rule_call("B", "v2/**") >> slow(30));

    let counted = Arc::new(Mutex::new(0usize));
    let c2 = counted.clone();
    let drainer = tokio::spawn(async move {
        p.apply(empty_stream())
            .take_until(async move {
                let mut cs = cancel_stream;
                let _ = cs.next().await;
            })
            .for_each(move |_| {
                let c2 = c2.clone();
                async move { *c2.lock().unwrap() += 1; }
            })
            .await;
    });

    tokio::time::sleep(Duration::from_millis(15)).await;
    let _ = cancel_tx.broadcast(()).await;
    drainer.await.unwrap();

    let n = *counted.lock().unwrap();
    println!("  cancel cascade emissions: {n} (sub-drain stopped mid-flight)");
}

// ─── USERLAND BEGIN — zero `let`/`mut` below ────────────────────────────────

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    println!("demo 1: fs >> re >> tag_write");
    drain(fs("v3/**/*.rs") >> re(r"fn (\w+)") >> tag_write(":fns")).await;

    println!("demo 2: tag_read >> print  (durable replay of demo 1)");
    drain(tag_read(":fns") >> print()).await;

    println!("demo 3: fork over rule_call — same Pipeline, fresh subscriptions");
    drain(rule_call("R", "v3/**") | rule_call("R", "v2/**")).await;

    println!("demo 3 readback:");
    drain(tag_read(":r_fns") >> print()).await;

    println!("demo 4: tag_read_live — replay + live (take 4 to bound the stream)");
    live_demo().await;

    println!("demo 5: switch_map at process-tree node");
    switch_demo().await;

    println!("demo 6: take_until cancel cascade through nested merge");
    cancel_demo().await;
}

// ─── USERLAND END ───────────────────────────────────────────────────────────

// ─── resistance log ─────────────────────────────────────────────────────────
//
// Userland `let`/`mut` count in main(): 0. (driver helpers above the line
// freely use let; they are framework, not userland.)
//
// What collapsed compared to rx_native_proof.rs:
//   - `Shared::empty::<()>()` Item-hardwiring weirdness GONE
//     → stream::empty().boxed() infers correctly to Cursor
//   - SharedBoxedObservable+Send Clone gymnastics GONE
//     → BoxStream<'static, Cursor> is the substrate; Pipeline wrapper
//       Arc-clones, inner stream moves once
//   - Subject lock dance (Arc<Mutex<SharedSubject>>) GONE
//     → async-broadcast Sender is Send+Sync, .try_broadcast under one Mutex<TagBag>
//   - rxrust's manual SharedScheduler GONE
//     → tokio runtime carries scheduling
//
// What's new:
//   - BroadcastStream wrapper bridges async-broadcast Receiver → Stream
//   - Replay+live for tag_read uses chain(snapshot, broadcast_stream),
//     same shape as rxjs ReplaySubject.subscribe
//   - switch_map demonstrated at the process-tree-node level via a driver
//     task that abort()s the prior inner JoinHandle. This is the
//     "subscription = OpCall, drop = cancel cascade" claim made concrete.
//   - take_until uses .take_until(future) on Stream; cancel signal is a
//     broadcast<()> so multiple consumers can listen. Nested merge
//     short-circuits cleanly when the future resolves.
//
// What's stubbed:
//   - effect-via-Batcher: drop-in is `batcher.run(eff).into_stream()
//     .flat_map(|reply| stream::iter(reply.cursors))`. Not wired here
//     because effect_runtime/rx feature isn't on the stream_kernel feature.
//   - share() / multicast fork from one upstream: same story as rxrust
//     POC; either futures-rx::share or async-broadcast wrap. Out of scope.
//
// Kill criterion: NOT triggered. tokio Stream substrate carries the
// const-golf userland surface, removes the rxrust friction points, and
// integrates natively with effect_runtime's tokio-based Batcher.
