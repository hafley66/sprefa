// Empty-body rule = passthrough. Caller supplies seed rows; rule
// echoes them straight into its sink fact via FactWrite. The unification
// proves out: declare-only is `Rule::new(...).declare(store)`, and a
// driven empty-body rule with input is "input → sink".

use std::sync::Arc;
use tokio::sync::mpsc;
use v4::*;

fn hooks(store: Arc<dyn Store>) -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store, effects: eff_tx, gen: 0, lineage: new_lineage(),
        tele: Telemetry::new(), interner: Interner::new(),
    }
}

#[tokio::test]
async fn empty_body_rule_echoes_seed_into_sink() {
    let store: Arc<dyn Store> = MemStore::new();
    let h = hooks(store.clone());

    // fact println_hits(REPO, FS) shape.
    let rule = Rule::new("println_hits", vec![], "println_hits", &["REPO", "FS"]);
    assert!(rule.is_passthrough());

    let mut a = Cursor::default(); a.set("REPO", "alpha"); a.set("FS", "src/main.rs");
    let mut b = Cursor::default(); b.set("REPO", "alpha"); b.set("FS", "src/lib.rs");
    let mut c = Cursor::default(); c.set("REPO", "beta");  c.set("FS", "x.rs");

    drive_rule_with_input(rule, vec![a, b, c], h, 8).await;
    store.commit(0).await;

    let rows = store.snapshot("println_hits").await;
    assert_eq!(rows.len(), 3, "all three seed cursors should pass through");
    let mut got: Vec<(String, String)> = rows.iter()
        .map(|c| (c.get("REPO").unwrap_or("").to_string(),
                  c.get("FS").unwrap_or("").to_string()))
        .collect();
    got.sort();
    assert_eq!(got, vec![
        ("alpha".into(), "src/lib.rs".into()),
        ("alpha".into(), "src/main.rs".into()),
        ("beta".into(),  "x.rs".into()),
    ]);
}

#[tokio::test]
async fn empty_body_rule_via_drive_rule_cached_is_skipped() {
    // No caller, no source: declaration-only path. Returns Skipped.
    // Schema is still ensured.
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    let h = hooks(store.clone());
    let cache = RuleStateCache::new();
    let rule = Rule::new("empty_decl", vec![], "empty_decl", &["A", "B"]);

    let outcome = drive_rule_cached(rule, h, 8, &cache).await;
    assert!(matches!(outcome, RuleRunOutcome::Skipped));

    // Schema present, table empty.
    let rows = store.snapshot("empty_decl").await;
    assert_eq!(rows.len(), 0);
}

#[tokio::test]
async fn non_empty_rule_with_seed_runs_body_then_writes() {
    // Rule with a body op (Filter-like via NextQ won't work without a chan;
    // use a simple identity-shaped body). Here we just push a Count op as
    // body to prove the chain composes: Seed > Count > FactWrite.
    use std::sync::atomic::AtomicU64;
    let store: Arc<dyn Store> = MemStore::new();
    let h = hooks(store.clone());

    let matches    = Arc::new(AtomicU64::new(0));
    let bytes_seen = Arc::new(AtomicU64::new(0));
    let body: Vec<Arc<dyn Op>> = vec![Arc::new(Count {
        matches: matches.clone(), bytes_seen: bytes_seen.clone(),
    })];

    let rule = Rule::new("counted", body, "counted", &["X"]);
    let mut c1 = Cursor::default(); c1.set("X", "1");
    let mut c2 = Cursor::default(); c2.set("X", "2");

    drive_rule_with_input(rule, vec![c1, c2], h, 8).await;
    store.commit(0).await;

    let rows = store.snapshot("counted").await;
    assert_eq!(rows.len(), 2);
    assert_eq!(matches.load(std::sync::atomic::Ordering::Relaxed), 2);
}
