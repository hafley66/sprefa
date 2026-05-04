// Layer-3 OpCache: per-cursor cache decorator over a pure inner op.
// Same lineage-hashed input → cached output. Refuses impure inners.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::PathBuf;
use std::io::Write;
use ast_grep_language::SupportLang;
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use v4::*;

fn hooks() -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store: MemStore::new(),
        effects: eff_tx, gen: 0, lineage: new_lineage(),
        tele: Telemetry::new(), interner: Interner::new(),
    }
}

static SEQ: std::sync::atomic::AtomicU64 = AtomicU64::new(0);
fn tempdir() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("v4_opcache_{}_{}_{}", pid, nanos, seq));
    std::fs::create_dir_all(&p).unwrap();
    p
}

async fn run_chain(chain: Vec<Arc<dyn Op>>, h: Hooks, seed: Vec<Vec<Cursor>>) -> Vec<Cursor> {
    let input = futures::stream::iter(seed).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    out
}

// A pure counter op: appends an N=<calls> term per emitted cursor and
// increments a shared counter. Used to verify the inner op is bypassed
// on cache hit.
struct Counted { calls: Arc<AtomicU64> }
impl Op for Counted {
    fn ident(&self) -> [u8; 32] { ident_of(&[b"counted"]) }
    fn is_pure(&self) -> bool { true }
    fn run(self: Arc<Self>, _h: Hooks, mut input: futures::stream::BoxStream<'static, Vec<Cursor>>)
        -> futures::stream::BoxStream<'static, Vec<Cursor>>
    {
        let me = self.clone();
        Box::pin(async_stream::stream! {
            while let Some(batch) = input.next().await {
                let mut out = Vec::with_capacity(batch.len());
                for mut c in batch {
                    let n = me.calls.fetch_add(1, Ordering::Relaxed) + 1;
                    c.set("N", n.to_string());
                    out.push(c);
                }
                yield out;
            }
        })
    }
}

#[tokio::test]
async fn cache_hit_skips_inner_and_emits_nothing() {
    // OpCache is skip-on-hit, no replay. Cursors do not flow downstream
    // on warm runs — sinks have already absorbed them on cold.
    let calls = Arc::new(AtomicU64::new(0));
    let inner: Arc<dyn Op> = Arc::new(Counted { calls: calls.clone() });
    let cached = OpCache::wrap(inner);

    let chain: Vec<Arc<dyn Op>> = vec![cached.clone()];

    let mut a = Cursor::default(); a.set("K", "x");
    let mut b = Cursor::default(); b.set("K", "y");

    // Cold run: 2 misses, both cursors flow.
    let out1 = run_chain(chain.clone(), hooks(), vec![vec![a.clone(), b.clone()]]).await;
    assert_eq!(out1.len(), 2);
    assert_eq!(cached.misses(), 2);
    assert_eq!(cached.hits(),   0);
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    // Warm run: 2 hits, NOTHING emitted, inner not invoked.
    let out2 = run_chain(chain.clone(), hooks(), vec![vec![a.clone(), b.clone()]]).await;
    assert_eq!(out2.len(), 0, "warm hits emit nothing");
    assert_eq!(cached.misses(), 2);
    assert_eq!(cached.hits(),   2);
    assert_eq!(calls.load(Ordering::Relaxed), 2,
        "inner op must not run again on cache hit");
}

#[tokio::test]
async fn cache_miss_on_different_cursor_lineage() {
    let calls = Arc::new(AtomicU64::new(0));
    let inner: Arc<dyn Op> = Arc::new(Counted { calls: calls.clone() });
    let cached = OpCache::wrap(inner);
    let chain: Vec<Arc<dyn Op>> = vec![cached.clone()];

    let mut a = Cursor::default(); a.set("K", "x");
    let mut b = Cursor::default(); b.set("K", "x"); b.set("EXTRA", "yes");
    let mut c = Cursor::default(); c.set("K", "y");

    let _ = run_chain(chain.clone(), hooks(), vec![vec![a, b, c]]).await;
    assert_eq!(cached.misses(), 3, "three distinct lineages → three misses");
    assert_eq!(cached.hits(),   0);
}

#[tokio::test]
#[should_panic(expected = "is_pure=false")]
async fn cache_refuses_impure_inner() {
    let bang: Arc<dyn Op> = Arc::new(ShBang::new("true"));
    let _ = OpCache::wrap(bang);
}

#[tokio::test]
async fn cache_wraps_ast_nm_skips_re_parse_on_warm() {
    // Real-shape test: Fs → cached(AstNm). First run parses; second run
    // returns cached match cursors, no spawn_blocking, no parse.
    let dir = tempdir();
    {
        let mut f = std::fs::File::create(dir.join("a.rs")).unwrap();
        writeln!(f, "fn main() {{ println!(\"hi\"); println!(\"two\"); }}").unwrap();
    }
    let cached = OpCache::wrap(Arc::new(AstNm::new("println!($$$)", SupportLang::Rust, &[])));

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Fs::new(dir.clone(), vec!["rs".into()])),
        cached.clone(),
    ];

    let out1 = run_chain(chain.clone(), hooks(), vec![]).await;
    assert_eq!(out1.len(), 2);
    assert_eq!(cached.misses(), 1);
    assert_eq!(cached.hits(),   0);

    let out2 = run_chain(chain.clone(), hooks(), vec![]).await;
    assert_eq!(out2.len(), 0, "warm AstNm hit emits nothing; inner skipped");
    assert_eq!(cached.misses(), 1, "miss count should not grow");
    assert_eq!(cached.hits(),   1, "second run hits cache");

    let _ = std::fs::remove_dir_all(&dir);
}
