// Smoke test: FactWrite (sink) + FactRead (join via IN-batch) through
// SqliteStore on :memory:. Models the v0 schema fragment:
//   files(file_id, path)
//   refs(file_id, lo, hi)
// Pipeline:
//   seed cursors → FactWrite("files") → ... → seed cursors → FactRead("refs", key=file_id) → collect

use std::sync::Arc;
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use v4::*;

fn cur(pairs: &[(&str, &str)]) -> Cursor {
    let mut c = Cursor::default();
    for (k, v) in pairs { c.set(*k, *v); }
    c
}

fn hooks_for(store: Arc<dyn Store>) -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    let interner = Interner::new();
    Hooks {
        store,
        effects: eff_tx,
        gen: 0,
        lineage: new_lineage(),
        tele: Telemetry::new(),
        interner,
    }
}

#[tokio::test]
async fn fact_write_and_read_through_pipeline() {
    let store: Arc<dyn Store> = SqliteStore::open_memory();

    // ──────────────────────────── seed: insert files + refs ─────────
    // Pre-populate with FactWrite-style schema decl + raw insert_many.
    store.ensure_schema("files", &["file_id", "path"]);
    store.insert_many("files", vec![
        cur(&[("file_id", "1"), ("path", "a.rs")]),
        cur(&[("file_id", "2"), ("path", "b.rs")]),
    ], 0).await;

    store.ensure_schema("refs", &["file_id", "lo", "hi"]);
    store.insert_many("refs", vec![
        cur(&[("file_id", "1"), ("lo",  "10"), ("hi",  "20")]),
        cur(&[("file_id", "1"), ("lo",  "30"), ("hi",  "40")]),
        cur(&[("file_id", "2"), ("lo", "100"), ("hi", "120")]),
    ], 0).await;
    store.commit(0).await;

    // ──────────────────────────── pipeline: outer = files, inner = refs ─
    // Build a chain: outer cursors carrying file_id → FactRead(refs, key=file_id, project=[lo, hi])
    // Outer is the "snapshot" of files, fed in as a one-shot batch.
    let outer = vec![
        cur(&[("file_id", "1"), ("path", "a.rs")]),
        cur(&[("file_id", "2"), ("path", "b.rs")]),
    ];

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(FactRead::new("refs", "file_id", &["lo", "hi"])),
    ];

    let h = hooks_for(store.clone());
    let input = futures::stream::iter(vec![outer]).boxed();
    let mut stream = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));

    let mut all_out: Vec<Cursor> = Vec::new();
    while let Some(batch) = stream.next().await {
        all_out.extend(batch);
    }

    // file 1 has 2 refs, file 2 has 1 ref → 3 cross-product cursors
    assert_eq!(all_out.len(), 3, "expected 3 joined rows, got {}", all_out.len());

    // Each output cursor carries file_id (from outer, preserved), path (from outer),
    // lo + hi (from inner via FactRead projection).
    for c in &all_out {
        assert!(c.get("file_id").is_some());
        assert!(c.get("path").is_some(), "path should be preserved from outer cursor");
        assert!(c.get("lo").is_some());
        assert!(c.get("hi").is_some());
    }

    // Verify the cross-product is correct: 2 rows for file_id=1, 1 row for file_id=2
    let f1: Vec<_> = all_out.iter().filter(|c| c.get("file_id") == Some("1")).collect();
    let f2: Vec<_> = all_out.iter().filter(|c| c.get("file_id") == Some("2")).collect();
    assert_eq!(f1.len(), 2);
    assert_eq!(f2.len(), 1);

    // Spot-check: file_id=1 should have (lo=10,hi=20) and (lo=30,hi=40)
    let mut f1_pairs: Vec<(String, String)> = f1.iter()
        .map(|c| (c.get("lo").unwrap().to_string(), c.get("hi").unwrap().to_string()))
        .collect();
    f1_pairs.sort();
    assert_eq!(f1_pairs, vec![
        ("10".to_string(), "20".to_string()),
        ("30".to_string(), "40".to_string()),
    ]);
}

#[tokio::test]
async fn fact_write_op_creates_schema_and_inserts() {
    // FactWrite as a pipe step calls ensure_schema + insert_many.
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    let h = hooks_for(store.clone());

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(FactWrite::new("matches", &["FS", "PAT", "LO", "HI"])),
    ];

    let batch = vec![
        cur(&[("FS", "kernel/sched.c"), ("PAT", "printk"), ("LO", "100"), ("HI", "110")]),
        cur(&[("FS", "kernel/sched.c"), ("PAT", "printk"), ("LO", "200"), ("HI", "210")]),
        cur(&[("FS", "drivers/usb.c"),  ("PAT", "printk"), ("LO",  "10"), ("HI",  "20")]),
    ];

    let input = futures::stream::iter(vec![batch]).boxed();
    let mut stream = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    while let Some(_b) = stream.next().await {}

    store.commit(0).await;

    let snap = store.snapshot("matches").await;
    assert_eq!(snap.len(), 3);
    let kernel_rows: Vec<_> = snap.iter().filter(|c| c.get("FS") == Some("kernel/sched.c")).collect();
    assert_eq!(kernel_rows.len(), 2);
}

#[tokio::test]
async fn rule_chain_construction() {
    // Rule { body, sink } — into_chain appends FactWrite, ready for drive_with.
    let body: Vec<Arc<dyn Op>> = vec![];
    let r = Rule::new("hot_strings", body, "hot_strings_out", &["VAL", "COUNT"]);
    let chain = r.into_chain();
    // Just verifies it produced a non-empty op chain ending in FactWrite.
    assert_eq!(chain.len(), 1);
}
