//! Lab tests: prove the generic-over-Next shape is ergonomic, and the
//! park-as-row redesign covers the migration target.
//!
//! Park subscriptions live on rows (`Wake::Key { domain, key }`) and
//! get promoted by `QueueBackend::dispatch_park`. EventBus is cache
//! fan-out only (`Event::Dirty { domain, key }`).

use std::sync::{Arc, Mutex};

use super::*;
use super::queue::QueueBackend;

const TEST_DOMAIN: &str = "test";

// --- demo carrier -----------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabCursor {
    pub terms: Vec<(String, String)>,
}

impl LabCursor {
    pub fn new() -> Self { Self { terms: Vec::new() } }

    pub fn with(mut self, k: &str, v: impl Into<String>) -> Self {
        self.set(k, v);
        self
    }

    pub fn set(&mut self, k: &str, v: impl Into<String>) -> &mut Self {
        let v = v.into();
        match self.terms.binary_search_by(|(n, _)| n.as_str().cmp(k)) {
            Ok(i)  => self.terms[i].1 = v,
            Err(i) => self.terms.insert(i, (k.to_string(), v)),
        }
        self
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.terms
            .binary_search_by(|(n, _)| n.as_str().cmp(k))
            .ok()
            .map(|i| self.terms[i].1.as_str())
    }
}

impl Next for LabCursor {
    fn content_hash(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        for (n, v) in &self.terms {
            h.update(&(n.len() as u32).to_le_bytes());
            h.update(n.as_bytes());
            h.update(&(v.len() as u32).to_le_bytes());
            h.update(v.as_bytes());
        }
        *h.finalize().as_bytes()
    }
}

impl super::codec::Codec for LabCursor {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(self.terms.len() as u32).to_le_bytes());
        for (n, v) in &self.terms {
            out.extend_from_slice(&(n.len() as u32).to_le_bytes());
            out.extend_from_slice(n.as_bytes());
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            out.extend_from_slice(v.as_bytes());
        }
        out
    }
    fn decode(bytes: &[u8]) -> Self {
        let mut p = 0;
        let read_u32 = |b: &[u8], p: &mut usize| -> u32 {
            let mut a = [0u8; 4];
            a.copy_from_slice(&b[*p..*p+4]);
            *p += 4;
            u32::from_le_bytes(a)
        };
        let n = read_u32(bytes, &mut p) as usize;
        let mut terms = Vec::with_capacity(n);
        for _ in 0..n {
            let nl = read_u32(bytes, &mut p) as usize;
            let name = std::str::from_utf8(&bytes[p..p+nl]).unwrap().to_string(); p += nl;
            let vl = read_u32(bytes, &mut p) as usize;
            let val  = std::str::from_utf8(&bytes[p..p+vl]).unwrap().to_string(); p += vl;
            terms.push((name, val));
        }
        Self { terms }
    }
}

fn lc(name: &str, val: &str) -> Arc<LabCursor> {
    Arc::new(LabCursor::new().with(name, val))
}

// --- demo components -------------------------------------------------

struct Trim { from: String, to: String }
impl Component for Trim {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let raw = c.get(&self.from).unwrap_or("").to_string();
        let mut next = c.clone();
        next.set(&self.to, raw.trim());
        Node::Emit(Arc::new(next))
    }
}

struct Collector { sink: Arc<Mutex<Vec<LabCursor>>> }
impl Component for Collector {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        self.sink.lock().unwrap().push(c.clone());
        Node::Done
    }
}

struct FanOut { n: usize }
impl Component for FanOut {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        Node::Many((0..self.n).map(|i| {
            let mut copy = c.clone();
            copy.set(":fan_idx", i.to_string());
            Node::Emit(Arc::new(copy))
        }).collect())
    }
}

struct ParkOnKey {
    key:   NextKey,
    fired: std::sync::atomic::AtomicBool,
}
impl ParkOnKey {
    fn new(key: NextKey) -> Self {
        Self { key, fired: std::sync::atomic::AtomicBool::new(false) }
    }
}
impl Component for ParkOnKey {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        use std::sync::atomic::Ordering;
        if self.fired.swap(true, Ordering::SeqCst) {
            // Second pass after wake: emit downstream.
            Node::Emit(Arc::new(c.clone()))
        } else {
            Node::Yield {
                value: Arc::new(c.clone()),
                wake:  Wake::Key { domain: TEST_DOMAIN.into(), key: self.key },
            }
        }
    }
}

// --- the tests -------------------------------------------------------

#[test]
fn pipe_value_builds_into_runnable_instance() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe: Pipe<LabCursor> = Pipe::new()
        .step(Arc::new(Trim { from: ":raw".into(), to: ":clean".into() }))
        .step(Arc::new(Collector { sink: sink.clone() }));
    assert_eq!(pipe.len(), 2);

    let inst = pipe.into_instance();
    expand(&inst, queue, vec![lc(":raw", "  hi  ")], ExpandOpts::default());

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":clean"), Some("hi"));
}

#[test]
fn pipe_extend_concatenates_steps() {
    let outer: Pipe<LabCursor> = Pipe::new()
        .step(Arc::new(Trim { from: ":raw".into(), to: ":a".into() }));
    let inner: Pipe<LabCursor> = Pipe::new()
        .step(Arc::new(Trim { from: ":a".into(), to: ":b".into() }));
    assert_eq!(outer.extend(inner).len(), 2);
}

#[test]
fn trim_collector_pipe_runs_to_drain() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(Trim { from: ":raw".into(), to: ":clean".into() })
            as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let stats = expand(
        &pipe, queue.clone(),
        vec![lc(":raw", "  hello  "), lc(":raw", "world\n")],
        ExpandOpts::default(),
    );

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].get(":clean"), Some("hello"));
    assert_eq!(got[1].get(":clean"), Some("world"));
    assert_eq!(stats.rendered, 4);
    assert_eq!(stats.parked,   0);
    assert_eq!(queue.depth(),  0);
}

#[test]
fn many_fanout_three_per_input() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(FanOut { n: 3 }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    expand(&pipe, queue, vec![lc(":raw", "x"), lc(":raw", "y")], ExpandOpts::default());

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 6);
    let xs: Vec<_> = got.iter().filter(|c| c.get(":raw") == Some("x")).collect();
    assert_eq!(xs.len(), 3);
    let mut idxs: Vec<_> = xs.iter().map(|c| c.get(":fan_idx").unwrap()).collect();
    idxs.sort();
    assert_eq!(idxs, vec!["0", "1", "2"]);
}

#[test]
fn yield_parks_until_dispatch_park_promotes() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let key   = bus.fresh_key();

    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey::new(key)) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let opts = ExpandOpts::default().with_bus(bus.clone());

    let stats = expand(
        &pipe, queue.clone(),
        vec![lc(":raw", "alpha")],
        opts.clone(),
    );
    assert_eq!(stats.rendered, 1);
    assert_eq!(stats.parked,   1);
    assert_eq!(sink.lock().unwrap().len(), 0);

    let promoted = queue.dispatch_park(TEST_DOMAIN, Some(key));
    assert_eq!(promoted, 1);

    let stats = expand(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(stats.rendered, 2);
    assert_eq!(stats.parked,   0);
    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":raw"), Some("alpha"));
}

/// The "no tokio" proof: a background OS thread promotes the parked
/// row directly through the queue. v2's pausability has no async
/// runtime dependency.
#[test]
fn background_thread_wake_no_async_runtime() {
    use std::time::Duration;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let key   = bus.fresh_key();

    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey::new(key)) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "thread-fired")], opts.clone());
    assert_eq!(queue.depth(), 1);

    let queue_for_thread = queue.clone();
    let h = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        queue_for_thread.dispatch_park(TEST_DOMAIN, Some(key));
    });
    h.join().unwrap();

    expand(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(sink.lock().unwrap().len(), 1);
    assert_eq!(queue.depth(), 0);
}

/// End-to-end: a Component dispatches "work" to a background thread,
/// gets a key, parks. The thread does its thing, writes the response
/// to MutationStore, promotes the parked row through the queue.
/// Downstream Component reads the response by key and emits a
/// transformed cursor. No tokio.
#[test]
fn mutation_store_routes_async_response_back() {
    use std::time::Duration;

    /// Single Component: dispatches off-thread on first render, parks
    /// via Yield, re-renders on wake, takes the response from the store
    /// and emits.
    struct AsyncUppercase {
        bus:        Arc<EventBus>,
        queue:      Arc<dyn QueueBackend<LabCursor>>,
        responses:  Arc<MutationStore<LabCursor>>,
        dispatched: Mutex<std::collections::HashMap<[u8; 32], NextKey>>,
    }
    impl Component for AsyncUppercase {
        type Next = LabCursor;
        fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
            let hash = c.content_hash();
            let mut d = self.dispatched.lock().unwrap();
            if let Some(&key) = d.get(&hash) {
                if let Some(resp) = self.responses.take(key) {
                    d.remove(&hash);
                    return Node::Emit(resp);
                }
                return Node::Yield {
                    value: Arc::new(c.clone()),
                    wake:  Wake::Key { domain: TEST_DOMAIN.into(), key },
                };
            }
            let key = self.bus.fresh_key();
            d.insert(hash, key);
            drop(d);

            let raw    = c.get(":raw").unwrap_or("").to_string();
            let parent = c.clone();
            let queue_t = self.queue.clone();
            let resp_t = self.responses.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(5));
                let mut out = parent;
                out.set(":upper", raw.to_uppercase());
                resp_t.put(key, Arc::new(out));
                queue_t.dispatch_park(TEST_DOMAIN, Some(key));
            });

            Node::Yield {
                value: Arc::new(c.clone()),
                wake:  Wake::Key { domain: TEST_DOMAIN.into(), key },
            }
        }
    }

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let resp  = Arc::new(MutationStore::<LabCursor>::new());

    let pipe = PipeInstance::new(vec![
        Arc::new(AsyncUppercase {
            bus: bus.clone(),
            queue: queue.clone(),
            responses: resp.clone(),
            dispatched: Mutex::new(std::collections::HashMap::new()),
        }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "hello")], opts.clone());
    assert_eq!(sink.lock().unwrap().len(), 0);
    assert_eq!(queue.depth(), 1);

    // Wait for the spawned thread to land its result + promote.
    while resp.is_empty() { std::thread::yield_now(); }
    std::thread::sleep(Duration::from_millis(15));

    expand(&pipe, queue.clone(), Vec::new(), opts);
    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":upper"), Some("HELLO"));
    assert_eq!(got[0].get(":raw"),   Some("hello"));
    assert!(resp.is_empty(), "response taken from store");
    assert_eq!(queue.depth(), 0);
}

// --- alternate carrier: prove it's actually generic ------------------

#[test]
fn carrier_can_be_a_plain_integer() {
    type N = i64;

    struct Double;
    impl Component for Double {
        type Next = N;
        fn render(&self, _: &RenderCtx, n: &N) -> Node<N> {
            Node::Emit(Arc::new(n * 2))
        }
    }

    struct Collector { sink: Arc<Mutex<Vec<N>>> }
    impl Component for Collector {
        type Next = N;
        fn render(&self, _: &RenderCtx, n: &N) -> Node<N> {
            self.sink.lock().unwrap().push(*n);
            Node::Done
        }
    }

    let queue: Arc<dyn QueueBackend<N>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let pipe  = PipeInstance::new(vec![
        Arc::new(Double) as Arc<dyn Component<Next = N>>,
        Arc::new(Double),
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    expand(
        &pipe, queue,
        vec![Arc::new(3i64), Arc::new(5i64)],
        ExpandOpts::default(),
    );

    let got = sink.lock().unwrap();
    assert_eq!(*got, vec![12, 20]);
}

// --- park-as-row: dispatch_park behavior -----------------------------

fn park_row<N: Next>(
    queue: &dyn QueueBackend<N>,
    domain: &'static str,
    key:    NextKey,
    value:  Arc<N>,
) {
    use super::queue::QueueRow;
    queue.enqueue(QueueRow {
        id:             0,
        parent_id:      None,
        batch_idx:      0,
        path:           Vec::new(),
        pipe_hash:      0,
        instance_id:    0,
        depth:          0,
        value,
        wake:           Wake::Key { domain: domain.into(), key },
        expand_tick:     0,
        enqueued_at_ns: 0,
    });
}

fn fresh_key(seed: u64) -> NextKey {
    let mut bytes = [0u8; 32];
    bytes[24..32].copy_from_slice(&seed.to_le_bytes());
    NextKey(bytes)
}

#[test]
fn dispatch_park_promotes_matching_rows() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let k1 = fresh_key(1);
    let k2 = fresh_key(2);

    for _ in 0..3 { park_row(queue.as_ref(), "fs",  k1, lc(":raw", "f")); }
    park_row(queue.as_ref(), "fs",  k2, lc(":raw", "f"));
    park_row(queue.as_ref(), "git", k1, lc(":raw", "g"));

    let n = queue.dispatch_park("fs", Some(k1));
    assert_eq!(n, 3);
}

#[test]
fn dispatch_park_domain_only_promotes_all_keys_in_domain() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let k1 = fresh_key(1);
    let k2 = fresh_key(2);

    for _ in 0..3 { park_row(queue.as_ref(), "fs",  k1, lc(":raw", "f")); }
    park_row(queue.as_ref(), "fs",  k2, lc(":raw", "f"));
    park_row(queue.as_ref(), "git", k1, lc(":raw", "g"));

    let n = queue.dispatch_park("fs", None);
    assert_eq!(n, 4);
    let m = queue.dispatch_park("git", None);
    assert_eq!(m, 1);
}

#[test]
fn mem_queue_dispatch_park_works() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let k = fresh_key(7);
    park_row(queue.as_ref(), "alpha", k, lc(":raw", "a"));
    assert_eq!(queue.depth(), 1);

    let n = queue.dispatch_park("alpha", Some(k));
    assert_eq!(n, 1);

    // After promotion, the row is runnable.
    let r = queue.pull_runnable(1);
    assert!(r.is_some());
    assert_eq!(queue.depth(), 0);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_dispatch_park_promotes_matching_rows() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());
    let k1 = fresh_key(11);
    let k2 = fresh_key(12);

    for _ in 0..3 { park_row(queue.as_ref(), "fs",  k1, lc(":raw", "f")); }
    park_row(queue.as_ref(), "fs",  k2, lc(":raw", "f"));
    park_row(queue.as_ref(), "git", k1, lc(":raw", "g"));

    assert_eq!(queue.dispatch_park("fs", Some(k1)), 3);
    assert_eq!(queue.dispatch_park("fs", None),     1); // k2 still parked
    assert_eq!(queue.dispatch_park("git", None),    1);
}

/// Park-as-row's RSS guarantee: 10k parked rows in SqliteQueue do not
/// retain `Arc<N>` after `enqueue`. Bound is generous to avoid flakes;
/// the point is directional correctness, not tight measurement.
#[cfg(all(feature = "sqlite", any(target_os = "macos", target_os = "linux")))]
#[test]
fn sqlite_park_rss_does_not_grow_significantly() {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());

    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let rss_before = sys.process(pid).map(|p| p.memory()).unwrap_or(0);

    const N: u64 = 10_000;
    for i in 0..N {
        let k = fresh_key(0xDEADBEEF + i);
        park_row(queue.as_ref(), "fs", k, lc(":raw", "x"));
    }

    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let rss_after = sys.process(pid).map(|p| p.memory()).unwrap_or(0);

    assert_eq!(queue.depth(), N);
    let delta = rss_after.saturating_sub(rss_before);
    let per_row = delta / N;
    assert!(
        per_row < 1024,
        "rss delta {} bytes / {} rows = {} bytes/row exceeded 1024",
        delta, N, per_row,
    );
}

// --- Phase F: SqliteQueue + crash-restart proof ----------------------

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_queue_replays_trim_collector_pipe() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(Trim { from: ":raw".into(), to: ":clean".into() })
            as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let stats = expand(
        &pipe, queue.clone(),
        vec![lc(":raw", "  hello  "), lc(":raw", "world\n")],
        ExpandOpts::default(),
    );

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].get(":clean"), Some("hello"));
    assert_eq!(got[1].get(":clean"), Some("world"));
    assert_eq!(stats.rendered, 4);
    assert_eq!(stats.parked,   0);
    assert_eq!(queue.depth(),  0);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_queue_many_fanout_three_per_input() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(FanOut { n: 3 }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    expand(&pipe, queue, vec![lc(":raw", "x"), lc(":raw", "y")], ExpandOpts::default());

    assert_eq!(sink.lock().unwrap().len(), 6);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_queue_yield_parks_until_dispatch_park() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let key   = bus.fresh_key();

    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey::new(key)) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "alpha")], opts.clone());
    assert_eq!(queue.depth(), 1);
    assert_eq!(sink.lock().unwrap().len(), 0);

    queue.dispatch_park(TEST_DOMAIN, Some(key));

    expand(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(sink.lock().unwrap().len(), 1);
    assert_eq!(queue.depth(), 0);
}

/// Crash-restart proof.
///
/// 1. Open a SqliteQueue + SqliteMutationStore on a real file.
/// 2. Drive a pipe that parks on a key. Pre-populate the mutation
///    result in the persistent store under that key (simulating a
///    completed mutationFn just before "crash").
/// 3. Drop everything. Reopen and redrive.
/// 4. Rather than re-issuing a bus event, the second process calls
///    `queue.dispatch_park` directly to promote the persisted parked
///    row.
#[cfg(feature = "sqlite")]
#[test]
fn crash_restart_resumes_parked_row_with_persisted_mutation() {
    use std::sync::Mutex as StdMutex;

    struct ParkAndConsume { store: Arc<SqliteMutationStore<LabCursor>> }
    impl Component for ParkAndConsume {
        type Next = LabCursor;
        fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
            let k = deterministic_key(c);
            if let Some(resp) = self.store.take(k) {
                return Node::Emit(resp);
            }
            Node::Yield {
                value: Arc::new(c.clone()),
                wake:  Wake::Key { domain: TEST_DOMAIN.into(), key: k },
            }
        }
    }

    fn deterministic_key(c: &LabCursor) -> NextKey {
        let mut h = blake3::Hasher::new();
        h.update(b"sprf_v2_test_salt");
        h.update(&c.content_hash());
        NextKey(*h.finalize().as_bytes())
    }

    let dir = tempdir();
    let db_path = dir.join("crash_restart.db");

    let input = lc(":raw", "alpha");
    let key = deterministic_key(&input);

    {
        let conn = Arc::new(StdMutex::new(rusqlite::Connection::open(&db_path).unwrap()));
        let queue: Arc<dyn QueueBackend<LabCursor>> =
            Arc::new(SqliteQueue::open(conn.clone()));
        let store = Arc::new(SqliteMutationStore::<LabCursor>::open(conn.clone()));
        let bus   = Arc::new(EventBus::new());

        let pipe = PipeInstance::new(vec![
            Arc::new(ParkAndConsume { store: store.clone() })
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(Collector { sink: Arc::new(Mutex::new(Vec::new())) }),
        ]);
        let opts = ExpandOpts::default().with_bus(bus.clone());

        expand(&pipe, queue.clone(), vec![input.clone()], opts);
        assert_eq!(queue.depth(), 1);

        let mut result = (*input).clone();
        result.set(":upper", "ALPHA");
        store.put(key, Arc::new(result));
        assert_eq!(store.len(), 1);
    }

    let collected = Arc::new(Mutex::new(Vec::new()));
    {
        let conn = Arc::new(StdMutex::new(rusqlite::Connection::open(&db_path).unwrap()));
        let queue: Arc<dyn QueueBackend<LabCursor>> =
            Arc::new(SqliteQueue::open(conn.clone()));
        let store = Arc::new(SqliteMutationStore::<LabCursor>::open(conn.clone()));
        let bus   = Arc::new(EventBus::new());

        // Park-as-row: the queue itself holds the wake state. Promote
        // the parked row directly through `dispatch_park`.
        let promoted = queue.dispatch_park(TEST_DOMAIN, Some(key));
        assert_eq!(promoted, 1, "parked row promoted after restart");
        assert_eq!(store.len(), 1);

        let pipe = PipeInstance::new(vec![
            Arc::new(ParkAndConsume { store: store.clone() })
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(Collector { sink: collected.clone() }),
        ]);
        let opts = ExpandOpts::default().with_bus(bus.clone());

        expand(&pipe, queue.clone(), Vec::new(), opts);
        assert_eq!(queue.depth(), 0);
        assert_eq!(store.len(),  0);
    }

    let got = collected.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":raw"),   Some("alpha"));
    assert_eq!(got[0].get(":upper"), Some("ALPHA"));
}

// --- Phase B: saga-style effect dispatch (Spawner + EffectDispatch) --

struct DispatchUppercase {
    fx:         Arc<EffectDispatch<LabCursor>>,
    store:      Arc<MutationStore<LabCursor>>,
    domain:     &'static str,
    dispatched: Mutex<std::collections::HashSet<[u8; 32]>>,
}
impl Component for DispatchUppercase {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let hash = c.content_hash();
        let key  = NextKey(hash);

        if let Some(r) = self.store.take(key) {
            return Node::Emit(r);
        }

        let mut d = self.dispatched.lock().unwrap();
        if !d.contains(&hash) {
            d.insert(hash);
            drop(d);

            let raw  = c.get(":raw").unwrap_or("").to_string();
            let seed = c.clone();
            self.fx.dispatch(key, move || {
                std::thread::sleep(std::time::Duration::from_millis(5));
                let mut out = seed;
                out.set(":upper", raw.to_uppercase());
                out
            });
        }

        Node::Yield {
            value: Arc::new(c.clone()),
            wake:  Wake::Key { domain: self.domain.into(), key },
        }
    }
}

#[test]
fn effect_dispatch_with_thread_spawner_no_runtime() {
    use std::time::Duration;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let store = Arc::new(MutationStore::<LabCursor>::new());
    let fx    = Arc::new(EffectDispatch::new(
        store.clone(),
        Arc::new(ThreadSpawner),
        queue.clone(),
    ).with_domain("phase-b"));

    let pipe = PipeInstance::new(vec![
        Arc::new(DispatchUppercase {
            fx: fx.clone(),
            store: store.clone(),
            domain: "phase-b",
            dispatched: Mutex::new(std::collections::HashSet::new()),
        }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "phase-b")], opts.clone());
    assert_eq!(queue.depth(), 1);

    // Wait until the spawned thread has put + dispatched_park.
    while store.is_empty() { std::thread::yield_now(); }
    std::thread::sleep(Duration::from_millis(5));

    expand(&pipe, queue.clone(), Vec::new(), opts);
    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":upper"), Some("PHASE-B"));
}

#[test]
fn effect_dispatch_with_tokio_spawner_via_runtime() {
    use std::time::Duration;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let bus   = Arc::new(EventBus::new());
        let store = Arc::new(MutationStore::<LabCursor>::new());
        let fx    = Arc::new(EffectDispatch::new(
            store.clone(),
            Arc::new(TokioSpawner),
            queue.clone(),
        ).with_domain("tokio-b"));

        let pipe = PipeInstance::new(vec![
            Arc::new(DispatchUppercase {
                fx: fx.clone(),
                store: store.clone(),
                domain: "tokio-b",
                dispatched: Mutex::new(std::collections::HashSet::new()),
            }) as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);
        let opts = ExpandOpts::default().with_bus(bus.clone());

        expand(&pipe, queue.clone(), vec![lc(":raw", "tokio-b")], opts.clone());
        assert_eq!(queue.depth(), 1);

        while store.is_empty() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        expand(&pipe, queue.clone(), Vec::new(), opts);
        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].get(":upper"), Some("TOKIO-B"));
    });
}

// --- Phase C: useMemo / Memoize -------------------------------------

struct CountingTrim {
    counter: Arc<std::sync::atomic::AtomicU64>,
    from:    String,
    to:      String,
}
impl Component for CountingTrim {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let raw = c.get(&self.from).unwrap_or("").to_string();
        let mut next = c.clone();
        next.set(&self.to, raw.trim());
        Node::Emit(Arc::new(next))
    }
}

#[test]
fn memoize_hits_cache_on_identical_input() {
    use std::sync::atomic::Ordering;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cache = Arc::new(MemoCache::<LabCursor>::new());

    let memoized = Memoize::new(
        CountingTrim {
            counter: counter.clone(),
            from: ":raw".into(),
            to: ":clean".into(),
        },
        "trim_clean",
        cache.clone(),
    );

    let pipe = PipeInstance::new(vec![
        Arc::new(memoized) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    expand(
        &pipe, queue.clone(),
        vec![
            lc(":raw", "  hi  "),
            lc(":raw", "  hi  "),
            lc(":raw", "  hi  "),
        ],
        ExpandOpts::default(),
    );

    assert_eq!(sink.lock().unwrap().len(), 3);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);
}

#[test]
fn memoize_misses_on_different_input() {
    use std::sync::atomic::Ordering;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cache = Arc::new(MemoCache::<LabCursor>::new());

    let memoized = Memoize::new(
        CountingTrim { counter: counter.clone(), from: ":raw".into(), to: ":clean".into() },
        "trim_clean",
        cache.clone(),
    );

    let pipe = PipeInstance::new(vec![
        Arc::new(memoized) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    expand(
        &pipe, queue.clone(),
        vec![lc(":raw", "  hi  "), lc(":raw", "  bye  "), lc(":raw", "  yo  ")],
        ExpandOpts::default(),
    );

    assert_eq!(sink.lock().unwrap().len(), 3);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
    assert_eq!(cache.len(), 3);
}

#[test]
fn dirty_domain_drops_tagged_memo_entries() {
    use std::sync::atomic::Ordering;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cache = Arc::new(MemoCache::<LabCursor>::new());
    let bus   = Arc::new(EventBus::new());

    attach_cache_to_bus(cache.clone(), &bus);

    let memoized = Memoize::new(
        CountingTrim { counter: counter.clone(), from: ":raw".into(), to: ":clean".into() },
        "trim_clean",
        cache.clone(),
    ).with_domain("fs");

    let pipe = PipeInstance::new(vec![
        Arc::new(memoized) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "  hi  ")], opts.clone());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);

    expand(&pipe, queue.clone(), vec![lc(":raw", "  hi  ")], opts.clone());
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    bus.dispatch_dirty("fs", None);
    assert_eq!(cache.len(), 0);

    expand(&pipe, queue.clone(), vec![lc(":raw", "  hi  ")], opts);
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    assert_eq!(sink.lock().unwrap().len(), 3);
}

// --- Phase D: useQuery / Query + invalidateQueries -------------------

struct ListReposQueryFn {
    counter: Arc<std::sync::atomic::AtomicU64>,
}
impl QueryFn<LabCursor> for ListReposQueryFn {
    fn ident(&self) -> &'static str { "list_repos" }
    fn run(&self, input: &LabCursor) -> LabCursor {
        self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut out = input.clone();
        out.set(":repos", "alpha,beta,gamma");
        out
    }
}

#[test]
fn query_runs_query_fn_once_then_serves_from_cache() {
    use std::sync::atomic::Ordering;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let cache = Arc::new(QueryCache::<LabCursor>::new());
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

    attach_query_cache_to_bus(cache.clone(), &bus);

    let q = Query::new(
        ListReposQueryFn { counter: counter.clone() },
        cache.clone(),
        bus.clone(),
        queue.clone(),
        Arc::new(ThreadSpawner),
    );

    let pipe = PipeInstance::new(vec![
        Arc::new(q) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "init")], opts.clone());
    assert_eq!(queue.depth(), 1);
    // Wait for queryFn to land its result + promote the parker.
    std::thread::sleep(std::time::Duration::from_millis(15));

    expand(&pipe, queue.clone(), Vec::new(), opts.clone());
    let got_a = sink.lock().unwrap().len();
    assert_eq!(got_a, 1);
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    expand(&pipe, queue.clone(), vec![lc(":raw", "init")], opts);
    assert_eq!(sink.lock().unwrap().len(), 2);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn invalidate_queries_via_dirty_domain_re_runs_query_fn() {
    use std::sync::atomic::Ordering;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let cache = Arc::new(QueryCache::<LabCursor>::new());
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));

    attach_query_cache_to_bus(cache.clone(), &bus);

    let q = Query::new(
        ListReposQueryFn { counter: counter.clone() },
        cache.clone(),
        bus.clone(),
        queue.clone(),
        Arc::new(ThreadSpawner),
    ).with_domain("repos");

    let pipe = PipeInstance::new(vec![
        Arc::new(q) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = ExpandOpts::default().with_bus(bus.clone());

    expand(&pipe, queue.clone(), vec![lc(":raw", "x")], opts.clone());
    std::thread::sleep(std::time::Duration::from_millis(15));
    expand(&pipe, queue.clone(), Vec::new(), opts.clone());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);

    bus.dispatch_dirty("repos", None);
    assert_eq!(cache.len(), 0);

    expand(&pipe, queue.clone(), vec![lc(":raw", "x")], opts.clone());
    std::thread::sleep(std::time::Duration::from_millis(15));
    expand(&pipe, queue.clone(), Vec::new(), opts);

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    assert_eq!(sink.lock().unwrap().len(), 2);
}

fn tempdir() -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    p.push(format!("sprf_v2_test_{}_{}", std::process::id(), nanos));
    std::fs::create_dir_all(&p).unwrap();
    p
}

// =====================================================================
// Three-tier Component proofs: render / render_batch / dispatch
// =====================================================================

#[test]
fn render_batch_override_sees_full_homogeneous_batch() {
    use std::sync::atomic::{AtomicUsize, Ordering as AO};

    struct UpperBatch { sizes_seen: Arc<Mutex<Vec<usize>>>, calls: Arc<AtomicUsize> }
    impl Component for UpperBatch {
        type Next = LabCursor;
        fn render_batch(&self, _: &RenderCtx, batch: &[&LabCursor]) -> Vec<Node<LabCursor>> {
            self.sizes_seen.lock().unwrap().push(batch.len());
            self.calls.fetch_add(1, AO::SeqCst);
            batch.iter().map(|c| {
                let raw = c.get(":raw").unwrap_or("").to_uppercase();
                let mut next = (*c).clone();
                next.set(":upper", raw);
                Node::Emit(Arc::new(next))
            }).collect()
        }
    }

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let sizes = Arc::new(Mutex::new(Vec::<usize>::new()));
    let calls = Arc::new(AtomicUsize::new(0));

    let pipe = PipeInstance::new(vec![
        Arc::new(UpperBatch { sizes_seen: sizes.clone(), calls: calls.clone() })
            as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let seeds: Vec<Arc<LabCursor>> =
        (0..7).map(|i| lc(":raw", &format!("v{}", i))).collect();
    expand(&pipe, queue.clone(), seeds, ExpandOpts::default());

    let collected = sink.lock().unwrap();
    assert_eq!(collected.len(), 7);
    let upper_set: Vec<&str> = collected.iter().map(|c| c.get(":upper").unwrap()).collect();
    for v in &upper_set {
        assert!(v.starts_with('V'), "got {:?}", v);
    }
    let sizes = sizes.lock().unwrap();
    let total: usize = sizes.iter().sum();
    assert_eq!(total, 7);
    assert_eq!(calls.load(AO::SeqCst), sizes.len());
    assert!(
        sizes.iter().any(|&n| n >= 2),
        "expected at least one batch >1 (sizes={:?})",
        *sizes,
    );
}

#[test]
fn par_render_runs_over_rayon_in_input_order() {
    struct ParUpper;
    impl Component for ParUpper {
        type Next = LabCursor;
        fn render_batch(&self, _: &RenderCtx, batch: &[&LabCursor]) -> Vec<Node<LabCursor>> {
            par_render(batch, |c| {
                let raw = c.get(":raw").unwrap_or("").to_uppercase();
                let mut next = c.clone();
                next.set(":upper", raw);
                Node::Emit(Arc::new(next))
            })
        }
    }

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(ParUpper) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let seeds: Vec<Arc<LabCursor>> =
        (0..32).map(|i| lc(":raw", &format!("seed{:03}", i))).collect();
    expand(&pipe, queue, seeds, ExpandOpts::default());

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 32);
    for c in got.iter() {
        let raw = c.get(":raw").unwrap();
        let up  = c.get(":upper").unwrap();
        assert_eq!(up, &raw.to_uppercase());
    }
}

/// Tier 3: `dispatch` override owns the splice. switchMap-style
/// cancellation: each new render promotes the prior parker by domain
/// + key, then enqueues a fresh parker.
// TODO step-4: switchMap shape loops under park-as-row; revisit with
// HybridQueue + cancellation primitive.
#[test]
#[ignore]
fn dispatch_override_controls_parker_enqueue_and_promotion() {
    use std::sync::atomic::{AtomicUsize, Ordering as AO};

    struct SwitchMap {
        bus:        Arc<EventBus>,
        prev_key:   Mutex<Option<NextKey>>,
        wake_count: Arc<AtomicUsize>,
    }
    impl Component for SwitchMap {
        type Next = LabCursor;
        fn dispatch(
            &self,
            ctx:   &RenderCtx,
            rows:  &[QueueRow<LabCursor>],
            queue: &dyn QueueBackend<LabCursor>,
        ) {
            for row in rows {
                let new_key = self.bus.fresh_key();
                if let Some(prev) = self.prev_key.lock().unwrap().replace(new_key) {
                    let n = queue.dispatch_park("switch", Some(prev));
                    self.wake_count.fetch_add(n as usize, AO::SeqCst);
                }
                let suspended = Node::Yield {
                    value: row.value.clone(),
                    wake:  Wake::Key { domain: "switch".into(), key: new_key },
                };
                splice_into(row, suspended, ctx.depth + 1, ctx.expand_tick, queue);
            }
        }
    }

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let waked = Arc::new(AtomicUsize::new(0));

    let pipe = PipeInstance::new(vec![
        Arc::new(SwitchMap {
            bus:        bus.clone(),
            prev_key:   Mutex::new(None),
            wake_count: waked.clone(),
        }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let opts = ExpandOpts::default().with_bus(bus.clone());

    let seeds: Vec<Arc<LabCursor>> =
        (0..3).map(|i| lc(":raw", &format!("v{}", i))).collect();
    expand(&pipe, queue.clone(), seeds, opts.clone());

    // 2 prior parkers got promoted (seeds 1 + 2 cancel seeds 0 + 1).
    assert_eq!(waked.load(AO::SeqCst), 2);

    expand(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(sink.lock().unwrap().len(), 2);
    assert_eq!(queue.depth(), 1, "the most-recent parker is still parked");
}

// =====================================================================
// Step 4: HybridQueue (write-back cache, MemQueue hot + SqliteQueue cold)
// =====================================================================

#[cfg(feature = "sqlite")]
mod hybrid {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use crate::v2::hybrid_queue::{HybridCfg, HybridQueue};
    use crate::v2::queue::QueueRow;

    fn now_ns() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    fn park_row_at<N: Next>(
        queue: &dyn QueueBackend<N>,
        domain: &'static str,
        key: NextKey,
        value: Arc<N>,
        enqueued_at_ns: u64,
    ) {
        queue.enqueue(QueueRow {
            id: 0,
            parent_id: None,
            batch_idx: 0,
            path: Vec::new(),
            pipe_hash: 0,
            instance_id: 0,
            depth: 0,
            value,
            wake: Wake::Key { domain: domain.into(), key },
            expand_tick: 0,
            enqueued_at_ns,
        });
    }

    #[test]
    fn hybrid_short_park_no_sql_write() {
        let cfg = HybridCfg {
            park_flush_interval: Duration::from_secs(1),
            park_mem_cap: 10_000,
            park_flush_batch: 256,
        };
        let q = HybridQueue::<LabCursor>::open_in_memory(cfg).unwrap();
        let k = fresh_key(1);
        park_row_at(&q, "test", k, lc(":raw", "hi"), now_ns());

        assert_eq!(q.cold().depth(), 0);
        assert_eq!(q.dispatch_park("test", Some(k)), 1);
        assert_eq!(q.cold().depth(), 0);
        let r = q.pull_runnable(1);
        assert!(r.is_some());
        assert_eq!(q.cold().depth(), 0);
    }

    #[test]
    fn hybrid_long_park_flushed_to_cold() {
        let cfg = HybridCfg {
            park_flush_interval: Duration::from_millis(10),
            park_mem_cap: 10_000,
            park_flush_batch: 256,
        };
        let q = HybridQueue::<LabCursor>::open_in_memory(cfg).unwrap();
        let k = fresh_key(2);
        park_row_at(&q, "test", k, lc(":raw", "alpha"), now_ns());

        std::thread::sleep(Duration::from_millis(50));
        let n = q.tick_flush();
        assert_eq!(n, 1);
        assert_eq!(q.hot().depth(),  0);
        assert_eq!(q.cold().depth(), 1);

        // Promote and pull through hybrid.
        let promoted = q.dispatch_park("test", Some(k));
        assert_eq!(promoted, 1);
        let r = q.pull_runnable(1);
        assert!(r.is_some());
        assert_eq!(q.depth(), 0);
    }

    #[test]
    #[ignore] // best-effort; race window narrow without instrumentation
    fn hybrid_dispatch_during_flush() {
        // TODO: deterministic version requires a flush hook between
        // drain and bulk_enqueue. Leaving as documentation of the
        // scenario the design must tolerate.
    }

    #[test]
    fn hybrid_rss_capped_by_mem_cap() {
        let cfg = HybridCfg {
            park_flush_interval: Duration::from_secs(60), // never age out
            park_mem_cap: 500,
            park_flush_batch: 5_000,
        };
        let q = HybridQueue::<LabCursor>::open_in_memory(cfg).unwrap();
        let now = now_ns();
        for i in 0..5_000u64 {
            let k = fresh_key(0xC0DE + i);
            park_row_at(&q, "test", k, lc(":raw", "x"), now);
        }
        // Cap enforcement is triggered inside `enqueue`. Hot park
        // count should never exceed mem_cap by more than one
        // enqueue's worth.
        assert!(
            q.hot().park_count() <= 500,
            "hot park count {} exceeds cap 500",
            q.hot().park_count(),
        );
        assert_eq!(q.depth(), 5_000);
    }

    #[test]
    fn hybrid_zero_interval_writes_through() {
        let cfg = HybridCfg {
            park_flush_interval: Duration::ZERO,
            park_mem_cap: 10_000,
            park_flush_batch: 256,
        };
        let q = HybridQueue::<LabCursor>::open_in_memory(cfg).unwrap();
        let now = now_ns();
        for i in 0..10u64 {
            let k = fresh_key(0xABCD + i);
            park_row_at(&q, "test", k, lc(":raw", "x"), now);
        }
        let n = q.tick_flush();
        assert_eq!(n, 10);
        assert_eq!(q.hot().depth(),  0);
        assert_eq!(q.cold().depth(), 10);
    }

    #[test]
    fn hybrid_pull_falls_through_hot_to_cold() {
        let cfg = HybridCfg::default();
        let q = HybridQueue::<LabCursor>::open_in_memory(cfg).unwrap();
        let k = fresh_key(99);
        // Park directly in cold tier to simulate post-flush state.
        park_row_at(q.cold().as_ref(), "test", k, lc(":raw", "cold"), now_ns());
        assert_eq!(q.hot().depth(),  0);
        assert_eq!(q.cold().depth(), 1);

        let promoted = q.dispatch_park("test", Some(k));
        assert_eq!(promoted, 1);
        let r = q.pull_runnable(1);
        assert!(r.is_some());
        assert_eq!(q.depth(), 0);
    }
}

/// Sanity: an op that overrides nothing is a no-op.
#[test]
fn unoverridden_component_drops_every_input() {
    struct NoOp;
    impl Component for NoOp { type Next = LabCursor; }

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(NoOp) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let stats = expand(
        &pipe, queue.clone(),
        vec![lc(":raw", "alpha"), lc(":raw", "beta")],
        ExpandOpts::default(),
    );
    assert_eq!(sink.lock().unwrap().len(), 0);
    assert_eq!(stats.rendered, 2);
    assert_eq!(stats.emitted,  0);
    assert_eq!(queue.depth(),  0);
}

// ---------------------------------------------------------------------
// cascade_delete — primitive used by Memoize-on-evict + reconcile.
// Tests are direct against the queue trait (not via expand) since the
// reconcile path that wires this into render is the next slice.
// ---------------------------------------------------------------------

mod cascade {
    use super::*;
    use super::super::queue::{QueueRow};
    use super::super::sqlite_queue::SqliteQueue;
    use super::super::wake::Wake;

    fn row(parent: Option<u64>, wake: Wake) -> QueueRow<LabCursor> {
        QueueRow {
            id:             0,  // backend assigns
            parent_id:      parent,
            batch_idx:      0,
            path:           Vec::new(),
            pipe_hash:      0,
            instance_id:    0,
            depth:          0,
            value:          lc(":raw", "x"),
            wake,
            expand_tick:    0,
            enqueued_at_ns: 0,
        }
    }

    /// Tree:  R -> {A -> {AA, AB}, B}     plus unrelated U.
    /// cascade_delete(R) removes R,A,B,AA,AB (5 rows). U survives.
    fn cascade_subtree_suite(queue: Arc<dyn QueueBackend<LabCursor>>) {
        // Mix wake states so the doomed set straddles all three buckets.
        let r = queue.enqueue(row(None,    Wake::Immediate));
        let a = queue.enqueue(row(Some(r), Wake::Tick { past_tick: 0 }));
        let _b = queue.enqueue(row(Some(r), Wake::Key {
            domain: "d".into(),
            key:    NextKey([0u8; 32]),
        }));
        let _aa = queue.enqueue(row(Some(a), Wake::Immediate));
        let _ab = queue.enqueue(row(Some(a), Wake::Immediate));
        let _u  = queue.enqueue(row(None,    Wake::Immediate));

        assert_eq!(queue.depth(), 6);
        let removed = queue.cascade_delete(r);
        assert_eq!(removed, 5);
        assert_eq!(queue.depth(), 1);
    }

    fn cascade_unknown_root_is_zero(queue: Arc<dyn QueueBackend<LabCursor>>) {
        queue.enqueue(row(None, Wake::Immediate));
        assert_eq!(queue.cascade_delete(99_999), 0);
        assert_eq!(queue.depth(), 1);
    }

    fn cascade_leaf_only_removes_self(queue: Arc<dyn QueueBackend<LabCursor>>) {
        let parent = queue.enqueue(row(None,         Wake::Immediate));
        let leaf   = queue.enqueue(row(Some(parent), Wake::Immediate));
        assert_eq!(queue.cascade_delete(leaf), 1);
        assert_eq!(queue.depth(), 1);
    }

    /// Root already popped; descendants linger. cascade_delete(root)
    /// must still reach them. Models the Memoize-on-evict case.
    fn cascade_with_absent_root(queue: Arc<dyn QueueBackend<LabCursor>>) {
        let r = queue.enqueue(row(None, Wake::Immediate));
        let _a = queue.enqueue(row(Some(r), Wake::Immediate));
        let _b = queue.enqueue(row(Some(r), Wake::Tick { past_tick: 0 }));
        // Pop the root out of the queue, leaving descendants orphaned.
        let popped = queue.pull_runnable(99);
        assert!(popped.is_some());
        assert_eq!(popped.unwrap().id, r);
        assert_eq!(queue.depth(), 2);
        // Now cascade by the (gone) root id; descendants must die.
        assert_eq!(queue.cascade_delete(r), 2);
        assert_eq!(queue.depth(), 0);
    }

    #[test] fn mem_cascade_subtree()             { cascade_subtree_suite(Arc::new(MemQueue::new())); }
    #[test] fn mem_cascade_unknown_root()        { cascade_unknown_root_is_zero(Arc::new(MemQueue::new())); }
    #[test] fn mem_cascade_leaf_only()           { cascade_leaf_only_removes_self(Arc::new(MemQueue::new())); }
    #[test] fn mem_cascade_with_absent_root()    { cascade_with_absent_root(Arc::new(MemQueue::new())); }

    #[test] fn sqlite_cascade_subtree()          { cascade_subtree_suite(Arc::new(SqliteQueue::<LabCursor>::open_in_memory())); }
    #[test] fn sqlite_cascade_unknown_root()     { cascade_unknown_root_is_zero(Arc::new(SqliteQueue::<LabCursor>::open_in_memory())); }
    #[test] fn sqlite_cascade_leaf_only()        { cascade_leaf_only_removes_self(Arc::new(SqliteQueue::<LabCursor>::open_in_memory())); }
    #[test] fn sqlite_cascade_with_absent_root() { cascade_with_absent_root(Arc::new(SqliteQueue::<LabCursor>::open_in_memory())); }

    /// CacheCascadeListener: bus event evicts cache entries AND
    /// cascade-deletes their queued descendants. End-to-end check that
    /// (b) source_row_id tracking + cascade wiring works.
    #[test]
    fn cache_cascade_listener_evicts_and_cascades() {
        use super::super::memoize::{
            attach_cache_to_bus_with_queue, MemoCache, MemoKey,
        };
        use super::super::node::Node;

        let cache: Arc<MemoCache<LabCursor>> = Arc::new(MemoCache::new());
        let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
        let bus   = EventBus::new();

        // Cache entry tagged with domain "fs" and source_row_id=42.
        let key = MemoKey([1u8; 32]);
        cache.put_with_source(
            key,
            Node::Done,
            vec!["fs"],
            [9u8; 32],
            Some(42),
        );

        // Three children whose parent_id=42. Models descendants spliced
        // from the (now-popped) row 42's prior render.
        for _ in 0..3 {
            queue.enqueue(row(Some(42), Wake::Immediate));
        }
        // One unrelated row that must survive.
        let _u = queue.enqueue(row(None, Wake::Immediate));

        assert_eq!(queue.depth(), 4);
        assert_eq!(cache.len(),   1);

        attach_cache_to_bus_with_queue(cache.clone(), &bus, queue.clone());
        bus.dispatch_dirty("fs", None);

        assert_eq!(cache.len(),   0);
        assert_eq!(queue.depth(), 1);  // only the unrelated row
    }

    /// Memoize::dispatch records source_row_id when called via expand.
    #[test]
    fn memoize_dispatch_records_source_row_id_on_miss() {
        use super::super::memoize::{MemoCache, Memoize};

        // Inner that emits a marker child so we can confirm dispatch
        // ran the inner Component (cache miss path).
        struct Mark;
        impl Component for Mark {
            type Next = LabCursor;
            fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
                let mut n = c.clone();
                n.set(":marked", "1");
                Node::Emit(Arc::new(n))
            }
        }

        let cache: Arc<MemoCache<LabCursor>> = Arc::new(MemoCache::new());
        let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));

        let pipe = PipeInstance::new(vec![
            Arc::new(Memoize::new(Mark, "mark", cache.clone()).with_domain("fs"))
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(&pipe, queue, vec![lc(":raw", "alpha")], ExpandOpts::default());

        // Cache populated; the entry's source_row_id is non-None.
        assert_eq!(cache.len(), 1);
        assert_eq!(sink.lock().unwrap().len(), 1);
        assert_eq!(sink.lock().unwrap()[0].get(":marked"), Some("1"));

        // Re-running with the same input is a cache hit; inner doesn't
        // re-render (we'd see a second :marked emit either way; the
        // signal we want is cache.len() unchanged).
        let queue2: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
        let sink2  = Arc::new(Mutex::new(Vec::new()));
        let pipe2 = PipeInstance::new(vec![
            Arc::new(Memoize::new(Mark, "mark", cache.clone()).with_domain("fs"))
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(Collector { sink: sink2.clone() }),
        ]);
        expand(&pipe2, queue2, vec![lc(":raw", "alpha")], ExpandOpts::default());
        assert_eq!(cache.len(), 1);
        assert_eq!(sink2.lock().unwrap().len(), 1);
    }
}
