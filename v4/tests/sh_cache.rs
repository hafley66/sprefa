// Shell caching: Sh is_pure=true ⇒ wraps under OpCache. The skip-on-hit
// contract for AstNm carries over: cold pass executes the shell command;
// warm pass with the same cursor lineage drops the input batch and emits
// nothing downstream. Position contract still holds — OpCache must sit
// upstream of a sink (or another cache); in-pipeline aggregators see
// nothing on warm runs by design.
//
// What we measure here:
//  1. wrap(Sh) does not panic (is_pure stays true through the cache).
//  2. cold pass runs the cmd N times (counted via touchfile).
//  3. warm pass runs the cmd 0 times for the same input set.
//  4. distinct cursor lineage on a follow-up batch ⇒ all misses (cold).

use std::sync::Arc;
use futures::stream::StreamExt;
use tempfile::TempDir;
use tokio::sync::mpsc;
use v4::*;

fn hooks() -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store: MemStore::new(), effects: eff_tx, gen: 0, lineage: new_lineage(),
        tele: Telemetry::new(), interner: Interner::new(),
    }
}

async fn run(op: Arc<dyn Op>, h: Hooks, batch: Vec<Cursor>) -> Vec<Cursor> {
    let input = futures::stream::iter(vec![batch]).boxed();
    let mut s = op.run(h, input);
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    out
}

#[tokio::test]
async fn sh_wraps_under_op_cache() {
    // Sanity: OpCache::wrap requires is_pure. Sh is pure by author
    // declaration, so this must not panic.
    let sh = Sh::emit_lines("echo hi");
    let _wrapped = OpCache::wrap(Arc::new(sh));
}

#[tokio::test]
async fn sh_warm_skip_drops_invocations() {
    // cmd appends a marker file per invocation. Cold = 3 invocations
    // (one per cursor). Warm = same 3 cursors, 0 new invocations.
    let dir = TempDir::new().unwrap();
    let touch = dir.path().join("hits");
    let cmd = format!("echo x >> {}", touch.display());
    let cache = OpCache::wrap(Arc::new(Sh::filter(&cmd)));

    let mut a = Cursor::default(); a.set("V", "alpha");
    let mut b = Cursor::default(); b.set("V", "beta");
    let mut c = Cursor::default(); c.set("V", "gamma");

    // Cold pass: 3 misses, 3 invocations.
    let cold_out = run(cache.clone() as Arc<dyn Op>, hooks(),
                       vec![a.clone(), b.clone(), c.clone()]).await;
    assert_eq!(cold_out.len(), 3);
    assert_eq!(cache.misses(), 3);
    assert_eq!(cache.hits(), 0);
    let cold_invocations = std::fs::read_to_string(&touch).map(|s| s.lines().count()).unwrap_or(0);
    assert_eq!(cold_invocations, 3);

    // Warm pass: same cursors, 3 hits, 0 new invocations, 0 emits.
    let warm_out = run(cache.clone() as Arc<dyn Op>, hooks(),
                       vec![a, b, c]).await;
    assert_eq!(warm_out.len(), 0, "warm pass emits nothing (skip-on-hit)");
    assert_eq!(cache.hits(), 3);
    assert_eq!(cache.misses(), 3);
    let warm_invocations = std::fs::read_to_string(&touch).map(|s| s.lines().count()).unwrap_or(0);
    assert_eq!(warm_invocations, 3, "no new shell invocations on warm");
}

#[tokio::test]
async fn sh_distinct_lineage_misses_again() {
    // Different cursor terms ⇒ different lineage hash ⇒ miss + invoke.
    let dir = TempDir::new().unwrap();
    let touch = dir.path().join("hits");
    let cmd = format!("echo x >> {}", touch.display());
    let cache = OpCache::wrap(Arc::new(Sh::filter(&cmd)));

    let mut a = Cursor::default(); a.set("V", "alpha");
    let _ = run(cache.clone() as Arc<dyn Op>, hooks(), vec![a]).await;
    assert_eq!(cache.misses(), 1);

    let mut b = Cursor::default(); b.set("V", "beta");
    let _ = run(cache.clone() as Arc<dyn Op>, hooks(), vec![b]).await;
    assert_eq!(cache.misses(), 2, "new lineage = miss");
    assert_eq!(cache.hits(), 0);

    let invocations = std::fs::read_to_string(&touch).unwrap().lines().count();
    assert_eq!(invocations, 2);
}

#[tokio::test]
async fn sh_emit_lines_warm_skip() {
    // EmitLines mode: cold emits N children per parent; warm skips
    // entirely. The skip-on-hit contract drops the parent input, which
    // means no children either. Verifies EmitLines is no exception.
    let cache = OpCache::wrap(Arc::new(Sh::emit_lines("printf 'a\\nb\\nc\\n'")));

    let mut p = Cursor::default(); p.set("PARENT", "p1");
    let cold = run(cache.clone() as Arc<dyn Op>, hooks(), vec![p.clone()]).await;
    assert_eq!(cold.len(), 3, "3 stdout lines emit 3 cursors");
    assert!(cold.iter().all(|c| c.get("PARENT") == Some("p1")));

    let warm = run(cache.clone() as Arc<dyn Op>, hooks(), vec![p]).await;
    assert_eq!(warm.len(), 0);
    assert_eq!(cache.hits(), 1);
}

#[tokio::test]
#[should_panic(expected = "OpCache::wrap requires a pure inner op")]
async fn sh_bang_refused_by_op_cache() {
    // ShBang is_pure=false ⇒ wrap panics. Same refusal contract as for
    // any other impure op.
    let _ = OpCache::wrap(Arc::new(ShBang::new("echo side")));
}
