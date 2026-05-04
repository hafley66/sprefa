// Next / NextQ / Close — channel pub/sub between concurrent rules.
// Channel hub is process-global; each test uses unique chan names.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use v4::*;

fn hooks(store: Arc<dyn Store>) -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store, effects: eff_tx, gen: 0, lineage: new_lineage(),
        tele: Telemetry::new(), interner: Interner::new(),
    }
}
static CHAN_SEQ: AtomicU64 = AtomicU64::new(0);
fn chan(prefix: &str) -> String {
    let n = CHAN_SEQ.fetch_add(1, Ordering::SeqCst);
    format!("{}_{}_{}", prefix, std::process::id(), n)
}

#[tokio::test]
async fn next_then_next_q_round_trip() {
    let ch = chan("rt");
    let mut a = Cursor::default(); a.set("V", "alpha");
    let mut b = Cursor::default(); b.set("V", "beta");
    let mut c = Cursor::default(); c.set("V", "gamma");

    // Subscribe BEFORE producer runs so broadcast doesn't drop events.
    let store: Arc<dyn Store> = MemStore::new();
    let consumer = tokio::spawn({
        let ch = ch.clone();
        let h = hooks(store.clone());
        async move {
            let chain: Vec<Arc<dyn Op>> = vec![Arc::new(NextQ::new(ch).take(3))];
            let input = futures::stream::iter(Vec::<Vec<Cursor>>::new()).boxed();
            let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
            let mut out = Vec::new();
            while let Some(b) = s.next().await { out.extend(b); }
            out
        }
    });

    // Yield to let the consumer subscribe before we send.
    tokio::task::yield_now().await;

    let chain: Vec<Arc<dyn Op>> = vec![Arc::new(Next::new(ch.clone()))];
    let h = hooks(store.clone());
    let input = futures::stream::iter(vec![vec![a, b, c]]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    while let Some(_) = s.next().await {}

    let received = consumer.await.unwrap();
    assert_eq!(received.len(), 3);
    let vs: Vec<_> = received.iter().map(|c| c.get("V").unwrap().to_string()).collect();
    assert_eq!(vs, vec!["alpha", "beta", "gamma"]);
}

#[tokio::test]
async fn close_op_terminates_unbounded_next_q() {
    let ch = chan("close");
    let store: Arc<dyn Store> = MemStore::new();

    let consumer = tokio::spawn({
        let ch = ch.clone();
        let h = hooks(store.clone());
        async move {
            // No `.take(...)`. Will run until channel closed.
            let chain: Vec<Arc<dyn Op>> = vec![Arc::new(NextQ::new(ch))];
            let input = futures::stream::iter(Vec::<Vec<Cursor>>::new()).boxed();
            let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
            let mut out = Vec::new();
            while let Some(b) = s.next().await { out.extend(b); }
            out
        }
    });
    tokio::task::yield_now().await;

    let mut a = Cursor::default(); a.set("V", "a");
    let mut b = Cursor::default(); b.set("V", "b");
    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Next::new(ch.clone())),
        Arc::new(Close::new(ch.clone())),
    ];
    let h = hooks(store.clone());
    let input = futures::stream::iter(vec![vec![a, b]]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    while let Some(_) = s.next().await {}

    // Close ran after the Next batch flowed → consumer recv returns
    // Closed and the source-shaped op terminates.
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(2), consumer
    ).await.expect("consumer must terminate after Close").unwrap();
    assert_eq!(received.len(), 2);
}

#[tokio::test]
async fn next_q_take_caps_emission_and_returns() {
    let ch = chan("take");
    let store: Arc<dyn Store> = MemStore::new();

    let consumer = tokio::spawn({
        let ch = ch.clone();
        let h = hooks(store.clone());
        async move {
            // Cap at 2 even though we'll send 5.
            let chain: Vec<Arc<dyn Op>> = vec![Arc::new(NextQ::new(ch).take(2))];
            let input = futures::stream::iter(Vec::<Vec<Cursor>>::new()).boxed();
            let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
            let mut out = Vec::new();
            while let Some(b) = s.next().await { out.extend(b); }
            out
        }
    });
    tokio::task::yield_now().await;

    let cs: Vec<Cursor> = (0..5).map(|i| {
        let mut c = Cursor::default(); c.set("V", &i.to_string()[..]); c
    }).collect();
    let chain: Vec<Arc<dyn Op>> = vec![Arc::new(Next::new(ch.clone()))];
    let h = hooks(store.clone());
    let input = futures::stream::iter(vec![cs]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    while let Some(_) = s.next().await {}

    let received = tokio::time::timeout(
        std::time::Duration::from_secs(2), consumer
    ).await.expect("take cap must terminate").unwrap();
    assert_eq!(received.len(), 2);
}

#[tokio::test]
async fn rule_with_empty_body_declares_schema_via_rule_declare() {
    // The "fact = rule with empty body" unification: declare the sink
    // schema through Rule::declare without driving anything.
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    let r = Rule::new("things", vec![], "things", &["id", "name"]);
    r.declare(&store);

    // Schema exists; can write rows that omit `name` (NULL).
    let mut c = Cursor::default(); c.set("id", "1");
    store.insert_many("things", vec![c], 0).await;
    store.commit(0).await;

    let snap = store.snapshot("things").await;
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].get("id"), Some("1"));
    assert!(snap[0].get("name").is_none());
}
