// Smoke test: drive_two_tick = render (drive_with) + commit (store flush).
// Also exercises CONTENT_HASH stamping by AstNm and the is_pure marker.

use std::sync::Arc;
use std::path::PathBuf;
use std::io::Write;
use ast_grep_language::SupportLang;
use tokio::sync::mpsc;
use v4::*;

fn hooks_for(store: Arc<dyn Store>) -> Hooks {
    let (eff_tx, _eff_rx) = mpsc::unbounded_channel();
    Hooks {
        store,
        effects: eff_tx,
        gen: 0,
        lineage: new_lineage(),
        tele: Telemetry::new(),
        interner: Interner::new(),
    }
}

#[tokio::test]
async fn two_tick_runs_render_then_commit() {
    // Tick 1 buffers FactWrite into pending; tick 2 flushes via commit.
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    let h = hooks_for(store.clone());

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(FactWrite::new("kv", &["k", "v"])),
    ];

    let batch = vec![
        { let mut c = Cursor::default(); c.set("k", "alpha"); c.set("v", "1"); c },
        { let mut c = Cursor::default(); c.set("k", "beta");  c.set("v", "2"); c },
    ];
    let input_stream = futures::stream::iter(vec![batch]).boxed();
    // Run render alone so we can confirm "no commit yet"
    let mut rendered = chain.clone().into_iter().fold(input_stream, |acc, op| op.run(h.clone(), acc));
    while let Some(_b) = rendered.next().await {}
    // Pre-commit snapshot should be empty (writes are pending).
    let pre = store.snapshot("kv").await;
    assert_eq!(pre.len(), 0, "pre-commit snapshot should be empty (writes still buffered)");
    // Commit: tick 2.
    store.commit(0).await;
    let post = store.snapshot("kv").await;
    assert_eq!(post.len(), 2, "post-commit snapshot should reflect flushed writes");
}

#[tokio::test]
async fn two_tick_helper_runs_both_phases() {
    // drive_two_tick = render + commit in one call.
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    let h = hooks_for(store.clone());

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(FactWrite::new("kv", &["k"])),
    ];

    // Inject one batch via a synthetic source op.
    pub struct SeedBatch { pub rows: Vec<Cursor> }
    impl Op for SeedBatch {
        fn ident(&self) -> [u8; 32] { ident_of(&[b"seed"]) }
        fn run(self: Arc<Self>, _h: Hooks, _in: futures::stream::BoxStream<'static, Vec<Cursor>>)
            -> futures::stream::BoxStream<'static, Vec<Cursor>>
        {
            let rows = self.rows.clone();
            Box::pin(async_stream::stream! { yield rows; })
        }
    }

    let seed: Arc<dyn Op> = Arc::new(SeedBatch {
        rows: vec![
            { let mut c = Cursor::default(); c.set("k", "x"); c },
            { let mut c = Cursor::default(); c.set("k", "y"); c },
            { let mut c = Cursor::default(); c.set("k", "z"); c },
        ],
    });
    let chain: Vec<Arc<dyn Op>> = vec![seed, chain.into_iter().next().unwrap()];

    drive_two_tick(chain, h, 8).await;

    let snap = store.snapshot("kv").await;
    assert_eq!(snap.len(), 3, "drive_two_tick should have flushed 3 rows post-commit");
}

#[tokio::test]
async fn ast_nm_stamps_content_hash() {
    // Tiny synthetic file → Fs → AstNm → check CONTENT_HASH on output cursors.
    let tmpdir = tempdir_in_target();
    let file_path = tmpdir.join("a.rs");
    {
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "fn main() {{ println!(\"hi\"); }}").unwrap();
    }

    let store: Arc<dyn Store> = MemStore::new();
    let h = hooks_for(store);

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Fs::new(tmpdir.clone(), vec!["rs".into()])),
        Arc::new(AstNm::new("println!($$$)", SupportLang::Rust, &[])),
    ];

    let mut stream = chain.into_iter().fold(
        futures::stream::iter(Vec::<Vec<Cursor>>::new()).boxed(),
        |acc, op| op.run(h.clone(), acc),
    );
    let mut all_out: Vec<Cursor> = Vec::new();
    while let Some(batch) = stream.next().await {
        all_out.extend(batch);
    }

    assert!(!all_out.is_empty(), "expected at least one match for println!");
    for c in &all_out {
        let hash = c.get("CONTENT_HASH");
        assert!(hash.is_some(), "every emitted cursor should carry CONTENT_HASH");
        // blake3 hex is 64 chars
        assert_eq!(hash.unwrap().len(), 64);
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmpdir);
}

#[tokio::test]
async fn is_pure_marker_classification() {
    // AstNm pure; Fs / FactWrite / FactRead / Fact / Count not pure.
    let astnm: Arc<dyn Op> = Arc::new(AstNm::new("$X", SupportLang::Rust, &[]));
    assert!(astnm.is_pure(), "AstNm should be pure");

    let fs: Arc<dyn Op> = Arc::new(Fs::new(PathBuf::from("/tmp"), vec!["rs".into()]));
    assert!(!fs.is_pure(), "Fs is a source op, not pure");

    let fw: Arc<dyn Op> = Arc::new(FactWrite::new("t", &["c"]));
    assert!(!fw.is_pure(), "FactWrite is a sink, not pure");

    let fr: Arc<dyn Op> = Arc::new(FactRead::new("t", "c", &["d"]));
    assert!(!fr.is_pure(), "FactRead reads external state, not pure");
}

fn tempdir_in_target() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("v4_two_tick_{}_{}", pid, nanos));
    std::fs::create_dir_all(&p).unwrap();
    p
}

use futures::stream::StreamExt as _;
