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
