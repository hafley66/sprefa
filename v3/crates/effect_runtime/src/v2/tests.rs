//! Lab tests: prove the generic-over-Next shape is ergonomic, and the
//! EventBus surface (key/path/domain) covers the migration target.

use std::sync::{Arc, Mutex};

use super::*;
use super::queue::QueueBackend;

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

struct ParkOnKey { key: NextKey }
impl Component for ParkOnKey {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        Node::Suspense {
            value: Arc::new(c.clone()),
            wake:  Wake::Key(self.key),
        }
    }
}

// --- the tests -------------------------------------------------------

#[test]
fn trim_collector_pipe_runs_to_drain() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(Trim { from: ":raw".into(), to: ":clean".into() })
            as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let stats = drive(
        &pipe, queue.clone(),
        vec![lc(":raw", "  hello  "), lc(":raw", "world\n")],
        DriveOpts::default(),
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

    drive(&pipe, queue, vec![lc(":raw", "x"), lc(":raw", "y")], DriveOpts::default());

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 6);
    let xs: Vec<_> = got.iter().filter(|c| c.get(":raw") == Some("x")).collect();
    assert_eq!(xs.len(), 3);
    let mut idxs: Vec<_> = xs.iter().map(|c| c.get(":fan_idx").unwrap()).collect();
    idxs.sort();
    assert_eq!(idxs, vec!["0", "1", "2"]);
}

#[test]
fn suspense_parks_until_key_dirty_dispatched() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let key   = bus.fresh_key();

    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey { key }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let opts = DriveOpts::default().with_bus(bus.clone());

    let stats = drive(
        &pipe, queue.clone(),
        vec![lc(":raw", "alpha")],
        opts.clone(),
    );
    assert_eq!(stats.rendered, 1);
    assert_eq!(stats.parked,   1);
    assert_eq!(sink.lock().unwrap().len(), 0);

    bus.dispatch(Event::KeyDirty(key));
    assert!(bus.is_ready(key));

    let stats = drive(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(stats.rendered, 1);
    assert_eq!(stats.parked,   0);
    assert!(!bus.is_ready(key), "driver forgot the key after pull");
    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":raw"), Some("alpha"));
}

/// The "no tokio" proof: a background OS thread dispatches the event.
/// v2's pausability has no async runtime dependency.
#[test]
fn background_thread_wake_no_async_runtime() {
    use std::time::Duration;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let key   = bus.fresh_key();

    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey { key }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "thread-fired")], opts.clone());
    assert_eq!(queue.depth(), 1);

    let bus_for_thread = bus.clone();
    let h = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        bus_for_thread.dispatch(Event::KeyDirty(key));
    });
    h.join().unwrap();

    drive(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(sink.lock().unwrap().len(), 1);
    assert_eq!(queue.depth(), 0);
}

/// End-to-end: a Component dispatches "work" to a background thread,
/// gets a key, parks. The thread does its thing, writes the response
/// to MutationStore, dispatches KeyDirty. Downstream Component reads
/// the response by key and emits a transformed cursor. No tokio.
#[test]
fn mutation_store_routes_async_response_back() {
    use std::time::Duration;

    /// Component that fires "fetch :raw and uppercase it" on a thread.
    struct AsyncUppercase {
        bus:       Arc<EventBus>,
        responses: Arc<MutationStore<LabCursor>>,
    }
    impl Component for AsyncUppercase {
        type Next = LabCursor;
        fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
            let key = self.bus.fresh_key();
            let raw = c.get(":raw").unwrap_or("").to_string();
            let parent = c.clone();

            let bus_t  = self.bus.clone();
            let resp_t = self.responses.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(5));
                let mut out = parent;
                out.set(":upper", raw.to_uppercase());
                resp_t.put(key, Arc::new(out));
                bus_t.dispatch(Event::KeyDirty(key));
            });

            // Park the input cursor. Tag the key in hex so the
            // downstream Component can fetch the response.
            let mut tagged = c.clone();
            tagged.set(":pending_key", hex32(key));
            Node::Suspense {
                value: Arc::new(tagged),
                wake:  Wake::Key(key),
            }
        }
    }

    /// Reads the response by key and emits it.
    struct ConsumeResponse { responses: Arc<MutationStore<LabCursor>> }
    impl Component for ConsumeResponse {
        type Next = LabCursor;
        fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
            let key = unhex32(c.get(":pending_key").unwrap());
            let resp = self.responses.take(key).expect("response present");
            Node::Emit(resp)
        }
    }

    fn hex32(k: NextKey) -> String {
        let mut s = String::with_capacity(64);
        for b in k.as_bytes() {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
    fn unhex32(s: &str) -> NextKey {
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i*2..i*2+2], 16).unwrap();
        }
        NextKey(out)
    }

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let resp  = Arc::new(MutationStore::<LabCursor>::new());

    let pipe = PipeInstance::new(vec![
        Arc::new(AsyncUppercase {
            bus: bus.clone(),
            responses: resp.clone(),
        }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(ConsumeResponse { responses: resp.clone() }),
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "hello")], opts.clone());
    // First drive: parked on the response.
    assert_eq!(sink.lock().unwrap().len(), 0);
    assert_eq!(queue.depth(), 1);

    // Wait for the background thread to land its response + dispatch.
    while bus.ready_count() == 0 && resp.is_empty() {
        std::thread::yield_now();
    }
    std::thread::sleep(Duration::from_millis(15));

    drive(&pipe, queue.clone(), Vec::new(), opts);
    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":upper"), Some("HELLO"));
    assert_eq!(got[0].get(":raw"),   Some("hello"));
    assert!(resp.is_empty(), "response taken from store");
    assert_eq!(queue.depth(), 0);
}

// --- alternate carrier: prove it's actually generic ------------------

/// A carrier that's not LabCursor at all — a plain integer payload —
/// exercising the same Component / Node / driver shape with N = i64.
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

    drive(
        &pipe, queue,
        vec![Arc::new(3i64), Arc::new(5i64)],
        DriveOpts::default(),
    );

    let got = sink.lock().unwrap();
    assert_eq!(*got, vec![12, 20]);
}

// --- new in Phase A: bus subscription flavors ------------------------

/// `subscribe_path(prefix, key)` + `dispatch(PathDirty(p))` wakes every
/// subscribed key whose registered path is a descendant of `p`.
#[test]
fn path_prefix_dirty_wakes_all_descendant_subscribers() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());

    // Two parker components, each parks against its own key, but
    // subscribes that key to a path. We then dispatch PathDirty at the
    // common prefix.
    let key_a = bus.fresh_key();
    let key_b = bus.fresh_key();
    let key_c = bus.fresh_key();
    bus.subscribe_path(vec![1, 2, 5], key_a);
    bus.subscribe_path(vec![1, 2, 9], key_b);
    bus.subscribe_path(vec![7, 0],    key_c); // not a descendant

    bus.dispatch(Event::PathDirty(vec![1, 2]));
    assert!( bus.is_ready(key_a));
    assert!( bus.is_ready(key_b));
    assert!(!bus.is_ready(key_c));
    assert_eq!(bus.ready_count(), 2);

    // And running the driver against rows parked on key_a / key_b drains.
    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey { key: key_a }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());
    drive(&pipe, queue.clone(), vec![lc(":raw", "xx")], opts.clone());
    drive(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(sink.lock().unwrap().len(), 1);
}

/// `subscribe_domain('fs', key)` + `dispatch(DomainDirty('fs'))` wakes
/// every key tagged with that domain. Models react-query's
/// `invalidateQueries` shape.
#[test]
fn domain_dirty_wakes_all_domain_subscribers() {
    let bus = Arc::new(EventBus::new());
    let k_fs1 = bus.fresh_key();
    let k_fs2 = bus.fresh_key();
    let k_git = bus.fresh_key();
    bus.subscribe_domain("fs",  k_fs1);
    bus.subscribe_domain("fs",  k_fs2);
    bus.subscribe_domain("git", k_git);

    bus.dispatch(Event::DomainDirty("fs"));
    assert!( bus.is_ready(k_fs1));
    assert!( bus.is_ready(k_fs2));
    assert!(!bus.is_ready(k_git));

    bus.dispatch(Event::DomainDirty("git"));
    assert!(bus.is_ready(k_git));
    assert_eq!(bus.ready_count(), 3);
}

// --- Phase F: SqliteQueue + crash-restart proof ----------------------

/// SqliteQueue is interchangeable with MemQueue at the trait boundary.
/// Same trim-collector pipe drives identically against both backends.
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

    let stats = drive(
        &pipe, queue.clone(),
        vec![lc(":raw", "  hello  "), lc(":raw", "world\n")],
        DriveOpts::default(),
    );

    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].get(":clean"), Some("hello"));
    assert_eq!(got[1].get(":clean"), Some("world"));
    assert_eq!(stats.rendered, 4);
    assert_eq!(stats.parked,   0);
    assert_eq!(queue.depth(),  0);
}

/// Sqlite many-fanout: same shape as MemQueue version.
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_queue_many_fanout_three_per_input() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());
    let sink  = Arc::new(Mutex::new(Vec::new()));

    let pipe = PipeInstance::new(vec![
        Arc::new(FanOut { n: 3 }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);

    drive(&pipe, queue, vec![lc(":raw", "x"), lc(":raw", "y")], DriveOpts::default());

    assert_eq!(sink.lock().unwrap().len(), 6);
}

/// Sqlite Suspense + KeyDirty: parker survives, drains on dispatch.
#[cfg(feature = "sqlite")]
#[test]
fn sqlite_queue_suspense_parks_until_key_dirty_dispatched() {
    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(SqliteQueue::open_in_memory());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let key   = bus.fresh_key();

    let pipe = PipeInstance::new(vec![
        Arc::new(ParkOnKey { key }) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "alpha")], opts.clone());
    assert_eq!(queue.depth(), 1);
    assert_eq!(sink.lock().unwrap().len(), 0);

    bus.dispatch(Event::KeyDirty(key));

    drive(&pipe, queue.clone(), Vec::new(), opts);
    assert_eq!(sink.lock().unwrap().len(), 1);
    assert_eq!(queue.depth(), 0);
}

/// Crash-restart proof.
///
/// 1. Open a SqliteQueue + SqliteMutationStore on a real file.
/// 2. Drive a pipe that parks on a key. Pre-populate the mutation
///    result in the persistent store under that key (simulating a
///    completed mutationFn just before "crash").
/// 3. Drop everything — Connection released, file closed.
/// 4. Reopen the same file with a brand-new SqliteQueue +
///    SqliteMutationStore + EventBus + driver. No in-memory state
///    survives.
/// 5. Dispatch KeyDirty for the same key. Drive.
/// 6. Sink output is identical to a never-crashed run: the parked row
///    resumed at depth+1 and read the result through the store.
#[cfg(feature = "sqlite")]
#[test]
fn crash_restart_resumes_parked_row_with_persisted_mutation() {
    use std::sync::Mutex as StdMutex;

    /// Component that produces a deterministic key from the cursor and
    /// parks on it. Unlike `bus.fresh_key()`, this key is reproducible
    /// across processes — a stable salt mixed with the cursor's
    /// content_hash.
    struct ParkOnDeterministicKey;
    impl Component for ParkOnDeterministicKey {
        type Next = LabCursor;
        fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
            Node::Suspense {
                value: Arc::new(c.clone()),
                wake:  Wake::Key(deterministic_key(c)),
            }
        }
    }

    /// Reads the mutation result by the deterministic key and emits it.
    struct ConsumeFromStore { store: Arc<SqliteMutationStore<LabCursor>> }
    impl Component for ConsumeFromStore {
        type Next = LabCursor;
        fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
            let k = deterministic_key(c);
            let resp = self.store.take(k).expect("persisted mutation must be present after restart");
            Node::Emit(resp)
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

    // ---- Process 1: park on key, persist mutation result, then crash.
    {
        let conn = Arc::new(StdMutex::new(rusqlite::Connection::open(&db_path).unwrap()));
        let queue: Arc<dyn QueueBackend<LabCursor>> =
            Arc::new(SqliteQueue::open(conn.clone()));
        let store = Arc::new(SqliteMutationStore::<LabCursor>::open(conn.clone()));
        let bus   = Arc::new(EventBus::new());

        let pipe = PipeInstance::new(vec![
            Arc::new(ParkOnDeterministicKey)
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(ConsumeFromStore { store: store.clone() }),
            Arc::new(Collector { sink: Arc::new(Mutex::new(Vec::new())) }),
        ]);

        let opts = DriveOpts::default().with_bus(bus.clone());

        drive(&pipe, queue.clone(), vec![input.clone()], opts);
        assert_eq!(queue.depth(), 1, "row parked in sqlite before crash");

        // Mutation completes off-thread, persists result, would dispatch
        // bus event — but the process dies before the dispatch lands.
        // We persist the result; the dispatch is *intentionally* lost.
        let mut result = (*input).clone();
        result.set(":upper", "ALPHA");
        store.put(key, Arc::new(result));
        assert_eq!(store.len(), 1);

        // Drop everything: simulate process exit.
    }

    // ---- Process 2: reopen file, fresh bus, fresh driver, redrive.
    let collected = Arc::new(Mutex::new(Vec::new()));
    {
        let conn = Arc::new(StdMutex::new(rusqlite::Connection::open(&db_path).unwrap()));
        let queue: Arc<dyn QueueBackend<LabCursor>> =
            Arc::new(SqliteQueue::open(conn.clone()));
        let store = Arc::new(SqliteMutationStore::<LabCursor>::open(conn.clone()));
        let bus   = Arc::new(EventBus::new());

        // After restart: the parked row is in the queue, the mutation
        // result is in the store. The runtime needs the bus event
        // re-dispatched to know the result is ready (events are
        // ephemeral — by design; the queue+store are the durable state,
        // the bus is the runtime's view of "what's ready right now").
        bus.dispatch(Event::KeyDirty(key));
        assert_eq!(store.len(), 1, "mutation result survived restart");
        assert_eq!(queue.depth(), 1, "parked row survived restart");

        let pipe = PipeInstance::new(vec![
            Arc::new(ParkOnDeterministicKey)
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(ConsumeFromStore { store: store.clone() }),
            Arc::new(Collector { sink: collected.clone() }),
        ]);
        let opts = DriveOpts::default().with_bus(bus.clone());

        drive(&pipe, queue.clone(), Vec::new(), opts);
        assert_eq!(queue.depth(), 0, "queue drained after restart");
        assert_eq!(store.len(),  0, "mutation result consumed");
    }

    let got = collected.lock().unwrap();
    assert_eq!(got.len(), 1, "exactly the one input emitted");
    assert_eq!(got[0].get(":raw"),   Some("alpha"));
    assert_eq!(got[0].get(":upper"), Some("ALPHA"));
}

// --- Phase B: saga-style effect dispatch (Spawner + EffectDispatch) --

/// Component shape used by both Phase B tests. Computes a stable key
/// from the cursor's content hash, dispatches a mutationFn through
/// `EffectDispatch`, parks on the same key. Downstream Component reads
/// the result and emits.
struct DispatchUppercase {
    fx: Arc<EffectDispatch<LabCursor>>,
}
impl Component for DispatchUppercase {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let key  = NextKey(c.content_hash());
        let raw  = c.get(":raw").unwrap_or("").to_string();
        let seed = c.clone();
        self.fx.dispatch(key, move || {
            // Small sleep so the parked-row state is observable before
            // the spawn finishes — otherwise the drive loop can race the
            // spawn and drain in a single call, making the test
            // non-deterministic.
            std::thread::sleep(std::time::Duration::from_millis(5));
            let mut out = seed;
            out.set(":upper", raw.to_uppercase());
            out
        });
        Node::Suspense { value: Arc::new(c.clone()), wake: Wake::Key(key) }
    }
}

struct ConsumeMut { store: Arc<MutationStore<LabCursor>> }
impl Component for ConsumeMut {
    type Next = LabCursor;
    fn render(&self, _: &RenderCtx, c: &LabCursor) -> Node<LabCursor> {
        let k = NextKey(c.content_hash());
        let r = self.store.take(k).expect("mutation result present");
        Node::Emit(r)
    }
}

/// Phase B.1: ThreadSpawner — no async runtime in the picture.
#[test]
fn effect_dispatch_with_thread_spawner_no_runtime() {
    use std::time::Duration;

    let queue: Arc<dyn QueueBackend<LabCursor>> = Arc::new(MemQueue::new());
    let sink  = Arc::new(Mutex::new(Vec::new()));
    let bus   = Arc::new(EventBus::new());
    let store = Arc::new(MutationStore::<LabCursor>::new());
    let fx    = Arc::new(EffectDispatch::new(
        bus.clone(),
        store.clone(),
        Arc::new(ThreadSpawner),
    ));

    let pipe = PipeInstance::new(vec![
        Arc::new(DispatchUppercase { fx: fx.clone() })
            as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(ConsumeMut { store: store.clone() }),
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "phase-b")], opts.clone());
    assert_eq!(queue.depth(), 1, "parked on the dispatched mutation");

    while bus.ready_count() == 0 { std::thread::yield_now(); }
    std::thread::sleep(Duration::from_millis(5));

    drive(&pipe, queue.clone(), Vec::new(), opts);
    let got = sink.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].get(":upper"), Some("PHASE-B"));
}

/// Phase B.2: TokioSpawner — same Component code, different Spawner.
/// Proves the Component is runtime-agnostic.
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
            bus.clone(),
            store.clone(),
            Arc::new(TokioSpawner),
        ));

        let pipe = PipeInstance::new(vec![
            Arc::new(DispatchUppercase { fx: fx.clone() })
                as Arc<dyn Component<Next = LabCursor>>,
            Arc::new(ConsumeMut { store: store.clone() }),
            Arc::new(Collector { sink: sink.clone() }),
        ]);
        let opts = DriveOpts::default().with_bus(bus.clone());

        drive(&pipe, queue.clone(), vec![lc(":raw", "tokio-b")], opts.clone());
        assert_eq!(queue.depth(), 1);

        while bus.ready_count() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;

        drive(&pipe, queue.clone(), Vec::new(), opts);
        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].get(":upper"), Some("TOKIO-B"));
    });
}

// --- Phase C: useMemo / Memoize -------------------------------------

/// Counts how many times its inner render fires. Wraps in Memoize and
/// observes the count to assert cache hit/miss behavior.
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

    drive(
        &pipe, queue.clone(),
        vec![
            lc(":raw", "  hi  "),
            lc(":raw", "  hi  "),  // identical content_hash
            lc(":raw", "  hi  "),  // identical content_hash
        ],
        DriveOpts::default(),
    );

    assert_eq!(sink.lock().unwrap().len(), 3, "all three rows reach the collector");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "inner render fired once; two cache hits");
    assert_eq!(cache.len(), 1, "one entry");
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

    drive(
        &pipe, queue.clone(),
        vec![lc(":raw", "  hi  "), lc(":raw", "  bye  "), lc(":raw", "  yo  ")],
        DriveOpts::default(),
    );

    assert_eq!(sink.lock().unwrap().len(), 3);
    assert_eq!(counter.load(Ordering::SeqCst), 3, "all distinct inputs miss the cache");
    assert_eq!(cache.len(), 3);
}

#[test]
fn domain_dirty_drops_tagged_memo_entries() {
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
    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "  hi  ")], opts.clone());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);

    drive(&pipe, queue.clone(), vec![lc(":raw", "  hi  ")], opts.clone());
    assert_eq!(counter.load(Ordering::SeqCst), 1, "second drive hits cache, no re-render");

    bus.dispatch(Event::DomainDirty("fs"));
    assert_eq!(cache.len(), 0, "fs-tagged entry dropped");

    drive(&pipe, queue.clone(), vec![lc(":raw", "  hi  ")], opts);
    assert_eq!(counter.load(Ordering::SeqCst), 2, "re-rendered after invalidation");
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
        // Tiny sleep to let render observe Pending state.
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
        Arc::new(ThreadSpawner),
    );

    let pipe = PipeInstance::new(vec![
        Arc::new(q) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "init")], opts.clone());
    assert_eq!(queue.depth(), 1, "parked while query pending");
    while bus.ready_count() == 0 { std::thread::yield_now(); }
    std::thread::sleep(std::time::Duration::from_millis(5));

    drive(&pipe, queue.clone(), Vec::new(), opts.clone());
    let got_a = sink.lock().unwrap().len();
    assert_eq!(got_a, 1, "first query produced one row");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "queryFn fired once");

    // Re-render with the same input — Success status, returned synchronously.
    drive(&pipe, queue.clone(), vec![lc(":raw", "init")], opts);
    assert_eq!(sink.lock().unwrap().len(), 2, "second render emits cached data");
    assert_eq!(counter.load(Ordering::SeqCst), 1, "still only one queryFn run");
}

#[test]
fn invalidate_queries_via_domain_dirty_re_runs_query_fn() {
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
        Arc::new(ThreadSpawner),
    ).with_domain("repos");

    let pipe = PipeInstance::new(vec![
        Arc::new(q) as Arc<dyn Component<Next = LabCursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());

    drive(&pipe, queue.clone(), vec![lc(":raw", "x")], opts.clone());
    while bus.ready_count() == 0 { std::thread::yield_now(); }
    std::thread::sleep(std::time::Duration::from_millis(5));
    drive(&pipe, queue.clone(), Vec::new(), opts.clone());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);

    bus.dispatch(Event::DomainDirty("repos"));
    assert_eq!(cache.len(), 0, "invalidateQueries dropped the entry");

    drive(&pipe, queue.clone(), vec![lc(":raw", "x")], opts.clone());
    // Wait until the queryFn's spawned closure has completed
    // (cache.set_success + bus.dispatch). bus.ready_count() goes back
    // to 1 after the second queryFn lands its KeyDirty.
    while bus.ready_count() == 0 { std::thread::yield_now(); }
    std::thread::sleep(std::time::Duration::from_millis(5));
    drive(&pipe, queue.clone(), Vec::new(), opts);

    assert_eq!(counter.load(Ordering::SeqCst), 2, "queryFn re-ran after invalidate");
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
