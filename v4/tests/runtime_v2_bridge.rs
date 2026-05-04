//! Phase G demonstration: v4's `Cursor` consumed by
//! `effect_runtime::v2` directly. Proves the bridge is wired and the
//! substrate is callable from v4 without rebuilding any of it.
//!
//! Builds a 3-stage pipe (Trim → Memoize<Trim> → Collector) over
//! `MemQueue<Cursor>` and exercises:
//!   - cursor flowing through the substrate
//!   - useMemo cache (Phase C) keyed on cursor content_hash
//!   - DomainDirty drops the memo entry, next render re-fires the
//!     wrapped Component (Phase C invalidation).
//!
//! Sqlite is off here (default-features = false on the v4 dep) so the
//! test does not pull rusqlite into v4's dep graph.

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use effect_runtime::v2::{
    attach_cache_to_bus, drive, Component, DriveOpts, Event, EventBus,
    MemoCache, MemQueue, Memoize, Node, PipeInstance, QueueBackend, RenderCtx,
};

use v4::Cursor;

fn term(n: &str, v: &str) -> (Arc<str>, Arc<str>) { (n.into(), v.into()) }

fn cursor_with(pairs: &[(&str, &str)]) -> Cursor {
    let mut terms: Vec<_> = pairs.iter().map(|(n, v)| term(n, v)).collect();
    terms.sort_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()));
    Cursor { terms }
}

struct CountingTrim {
    counter: Arc<AtomicU64>,
    from:    String,
    to:      String,
}

impl Component for CountingTrim {
    type Next = Cursor;

    fn render(&self, _: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        let raw = c.terms.iter()
            .find(|(n, _)| n.as_ref() == self.from)
            .map(|(_, v)| v.as_ref().to_string())
            .unwrap_or_default();
        let mut next_terms = c.terms.clone();
        let trimmed: Arc<str> = raw.trim().into();
        let to: Arc<str> = self.to.as_str().into();
        match next_terms.binary_search_by(|(n, _)| n.as_ref().cmp(self.to.as_str())) {
            Ok(i)  => next_terms[i].1 = trimmed,
            Err(i) => next_terms.insert(i, (to, trimmed)),
        }
        Node::Emit(Arc::new(Cursor { terms: next_terms }))
    }
}

struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }

impl Component for Collector {
    type Next = Cursor;
    fn render(&self, _: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.sink.lock().unwrap().push(c.clone());
        Node::Done
    }
}

#[test]
fn v4_cursor_flows_through_effect_runtime_v2_with_memoize() {
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let sink    = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(AtomicU64::new(0));
    let bus     = Arc::new(EventBus::new());
    let cache   = Arc::new(MemoCache::<Cursor>::new());
    attach_cache_to_bus(cache.clone(), &bus);

    let memoized = Memoize::new(
        CountingTrim {
            counter: counter.clone(),
            from: ":raw".into(),
            to:   ":clean".into(),
        },
        "v4_trim_clean",
        cache.clone(),
    ).with_domain("fs");

    let pipe = PipeInstance::new(vec![
        Arc::new(memoized) as Arc<dyn Component<Next = Cursor>>,
        Arc::new(Collector { sink: sink.clone() }),
    ]);
    let opts = DriveOpts::default().with_bus(bus.clone());

    let same = cursor_with(&[(":raw", "  hi  ")]);
    let other = cursor_with(&[(":raw", "  bye  ")]);

    drive(
        &pipe, queue.clone(),
        vec![Arc::new(same.clone()), Arc::new(same.clone()), Arc::new(other.clone())],
        opts.clone(),
    );

    assert_eq!(sink.lock().unwrap().len(), 3);
    assert_eq!(counter.load(Ordering::SeqCst), 2,
        "two distinct content hashes ⇒ inner ran twice; one cache hit");
    assert_eq!(cache.len(), 2);

    bus.dispatch(Event::DomainDirty("fs"));
    assert_eq!(cache.len(), 0, "fs-tagged cache entries dropped");

    drive(&pipe, queue.clone(), vec![Arc::new(same.clone())], opts);
    assert_eq!(counter.load(Ordering::SeqCst), 3, "re-rendered after invalidation");
    assert_eq!(sink.lock().unwrap().len(), 4);
}
