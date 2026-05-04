// declare_fact + NULL-on-missing + RuleQuery (rule? read).

use std::sync::Arc;
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

#[tokio::test]
async fn declare_fact_then_partial_inserts_become_null() {
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    declare_fact(&store, "users", &["id", "name", "email"]);

    // Insert one full row, one with email missing, one with only id.
    let mk = |kv: &[(&str, &str)]| {
        let mut c = Cursor::default();
        for (k, v) in kv { c.set(*k, *v); }
        c
    };
    store.insert_many("users", vec![
        mk(&[("id", "1"), ("name", "alpha"), ("email", "a@x")]),
        mk(&[("id", "2"), ("name", "beta")]),
        mk(&[("id", "3")]),
    ], 0).await;
    store.commit(0).await;

    let snap = store.snapshot("users").await;
    assert_eq!(snap.len(), 3);
    let by_id: std::collections::HashMap<_, _> = snap.iter()
        .map(|c| (c.get("id").unwrap().to_string(), c.clone())).collect();

    // Full row: every term set.
    assert_eq!(by_id["1"].get("name"),  Some("alpha"));
    assert_eq!(by_id["1"].get("email"), Some("a@x"));
    // Missing email → not set on cursor (NULL round-trips as missing).
    assert_eq!(by_id["2"].get("name"),  Some("beta"));
    assert!(by_id["2"].get("email").is_none(),
        "missing-on-insert must round-trip as missing-on-read, not as empty string");
    // Only id set.
    assert!(by_id["3"].get("name").is_none());
    assert!(by_id["3"].get("email").is_none());
}

#[tokio::test]
async fn null_distinguishes_from_empty_string() {
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    declare_fact(&store, "kv", &["k", "v"]);

    let mut a = Cursor::default(); a.set("k", "missing");
    let mut b = Cursor::default(); b.set("k", "empty"); b.set("v", "");
    store.insert_many("kv", vec![a, b], 0).await;
    store.commit(0).await;

    let rows = store.read_in("kv", "k", vec!["missing".into(), "empty".into()], vec!["v".into()]).await;
    let by_k: std::collections::HashMap<_, _> = rows.iter()
        .map(|c| (c.get("k").unwrap().to_string(), c.clone())).collect();
    assert!(by_k["missing"].get("v").is_none(),
        "missing must read back as None, not Some(\"\")");
    assert_eq!(by_k["empty"].get("v"),   Some(""),
        "explicit empty string must read back as Some(\"\")");
}

#[tokio::test]
async fn rule_write_then_rule_query_round_trip() {
    let store: Arc<dyn Store> = SqliteStore::open_memory();

    // Rule: from a synthetic seed batch, write into "tags" table.
    pub struct SeedBatch { pub rows: Vec<Cursor> }
    impl Op for SeedBatch {
        fn ident(&self) -> [u8; 32] { ident_of(&[b"seed"]) }
        fn run(self: Arc<Self>, _h: Hooks, _i: futures::stream::BoxStream<'static, Vec<Cursor>>)
            -> futures::stream::BoxStream<'static, Vec<Cursor>>
        {
            let rows = self.rows.clone();
            Box::pin(async_stream::stream! { yield rows; })
        }
    }
    let mk = |kv: &[(&str, &str)]| {
        let mut c = Cursor::default();
        for (k, v) in kv { c.set(*k, *v); }
        c
    };
    let seed = Arc::new(SeedBatch {
        rows: vec![
            mk(&[("FILE", "a.rs"), ("TAG", "todo")]),
            mk(&[("FILE", "a.rs"), ("TAG", "fixme")]),
            mk(&[("FILE", "b.rs"), ("TAG", "todo")]),
        ],
    });
    let rule = Rule::new(
        "tags_rule",
        vec![seed as Arc<dyn Op>],
        "tags",
        &["FILE", "TAG"],
    );

    let h = hooks(store.clone());
    drive_two_tick(rule.clone().into_chain(), h, 8).await;

    // rule? on the sink: read tags for FILE=a.rs, project TAG.
    let q = RuleQuery::from_rule(&rule, "FILE", &["TAG"]);
    let mut input_c = Cursor::default(); input_c.set("FILE", "a.rs");
    let chain: Vec<Arc<dyn Op>> = vec![Arc::new(q)];
    let h = hooks(store.clone());
    let input = futures::stream::iter(vec![vec![input_c]]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }

    // Expect 2 rows (todo + fixme on a.rs); b.rs filtered out by key.
    assert_eq!(out.len(), 2);
    let mut tags: Vec<_> = out.iter().map(|c| c.get("TAG").unwrap().to_string()).collect();
    tags.sort();
    assert_eq!(tags, vec!["fixme", "todo"]);
}

#[tokio::test]
async fn rule_query_by_sink_works_without_rule_handle() {
    // RuleQuery::by_sink for cases where caller only knows the sink fact name.
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    declare_fact(&store, "things", &["id", "kind"]);
    let mut c1 = Cursor::default(); c1.set("id", "1"); c1.set("kind", "alpha");
    let mut c2 = Cursor::default(); c2.set("id", "2"); c2.set("kind", "beta");
    store.insert_many("things", vec![c1, c2], 0).await;
    store.commit(0).await;

    let q = RuleQuery::by_sink("things", "id", &["kind"]);
    let mut input_c = Cursor::default(); input_c.set("id", "2");
    let chain: Vec<Arc<dyn Op>> = vec![Arc::new(q)];
    let h = hooks(store.clone());
    let input = futures::stream::iter(vec![vec![input_c]]).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get("kind"), Some("beta"));
}
