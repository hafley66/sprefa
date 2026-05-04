// Layer-2: per-rule input set hash, skip-if-unchanged.
// Probe = body[0].probe() walks fs + blake3-hashes each file. RuleStateCache
// remembers last-run hash per rule_name. drive_rule_cached skips drain on hit.

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

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn tempdir() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("v4_layer2_{}_{}_{}", pid, nanos, seq));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn fs_probe_returns_sorted_path_hash_pairs() {
    let dir = tempdir();
    {
        let mut f = std::fs::File::create(dir.join("z.rs")).unwrap();
        writeln!(f, "fn z() {{}}").unwrap();
        let mut f = std::fs::File::create(dir.join("a.rs")).unwrap();
        writeln!(f, "fn a() {{}}").unwrap();
        // non-rs filtered out
        let mut f = std::fs::File::create(dir.join("b.txt")).unwrap();
        writeln!(f, "skip").unwrap();
    }
    let fs: Arc<dyn Op> = Arc::new(Fs::new(dir.clone(), vec!["rs".into()]));
    let probed = fs.probe().expect("Fs implements probe");
    assert_eq!(probed.len(), 2);
    assert!(probed[0].0.ends_with("a.rs"));
    assert!(probed[1].0.ends_with("z.rs"));
    // Each entry has 32-byte hash.
    for (_, h) in &probed { assert_eq!(h.len(), 32); }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn input_set_hash_stable_across_calls() {
    let dir = tempdir();
    {
        let mut f = std::fs::File::create(dir.join("a.rs")).unwrap();
        writeln!(f, "fn main() {{}}").unwrap();
    }
    let body: Vec<Arc<dyn Op>> = vec![
        Arc::new(Fs::new(dir.clone(), vec!["rs".into()])),
        Arc::new(AstNm::new("$X", SupportLang::Rust, &[])),
    ];
    let r = Rule::new("r", body, "out", &["FS"]);
    let h1 = r.input_set_hash().unwrap();
    let body2: Vec<Arc<dyn Op>> = vec![
        Arc::new(Fs::new(dir.clone(), vec!["rs".into()])),
        Arc::new(AstNm::new("$X", SupportLang::Rust, &[])),
    ];
    let r2 = Rule::new("r", body2, "out", &["FS"]);
    let h2 = r2.input_set_hash().unwrap();
    assert_eq!(h1, h2, "hash is deterministic on identical fs state");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn input_set_hash_changes_on_file_edit() {
    let dir = tempdir();
    let p = dir.join("a.rs");
    {
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "fn main() {{}}").unwrap();
    }
    let mk = || -> Rule {
        let body: Vec<Arc<dyn Op>> = vec![
            Arc::new(Fs::new(dir.clone(), vec!["rs".into()])),
        ];
        Rule::new("r", body, "out", &["FS"])
    };
    let h1 = mk().input_set_hash().unwrap();
    {
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "fn main() {{ println!(\"hi\"); }}").unwrap();
    }
    let h2 = mk().input_set_hash().unwrap();
    assert_ne!(h1, h2, "edit changes content_hash → set hash differs");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn drive_rule_cached_skips_on_unchanged_input() {
    let dir = tempdir();
    {
        let mut f = std::fs::File::create(dir.join("a.rs")).unwrap();
        writeln!(f, "fn main() {{ println!(\"hi\"); }}").unwrap();
    }
    let store: Arc<dyn Store> = SqliteStore::open_memory();
    let cache = RuleStateCache::new();

    let mk_rule = || -> Rule {
        let body: Vec<Arc<dyn Op>> = vec![
            Arc::new(Fs::new(dir.clone(), vec!["rs".into()])),
            Arc::new(AstNm::new("println!($$$)", SupportLang::Rust, &[])),
        ];
        Rule::new("println_rule", body, "println_hits", &["FS", "CONTENT_HASH"])
    };

    // First run: no entry in cache → Ran. Sink populated.
    let h = hooks_for(store.clone());
    let out1 = drive_rule_cached(mk_rule(), h, 8, &cache).await;
    assert!(matches!(out1, RuleRunOutcome::Ran));
    let after_first = store.snapshot("println_hits").await;
    assert!(!after_first.is_empty(), "first run should populate sink");
    let n_first = after_first.len();

    // Second run, no edits: cache hit → Skipped. Sink unchanged.
    let h = hooks_for(store.clone());
    let out2 = drive_rule_cached(mk_rule(), h, 8, &cache).await;
    assert!(matches!(out2, RuleRunOutcome::Skipped));
    let after_second = store.snapshot("println_hits").await;
    assert_eq!(after_second.len(), n_first,
        "skipped run must not append more rows");

    // Edit the file: cache miss → Ran. Sink grows (re-emits matches).
    {
        let mut f = std::fs::File::create(dir.join("a.rs")).unwrap();
        writeln!(f, "fn main() {{ println!(\"hi\"); println!(\"two\"); }}").unwrap();
    }
    let h = hooks_for(store.clone());
    let out3 = drive_rule_cached(mk_rule(), h, 8, &cache).await;
    assert!(matches!(out3, RuleRunOutcome::Ran),
        "edit must invalidate cache and force a re-run");
    let after_third = store.snapshot("println_hits").await;
    assert!(after_third.len() > n_first,
        "post-edit run should add new println_hits rows ({} → {})",
        n_first, after_third.len());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn sqlite_open_path_round_trip() {
    let dir = tempdir();
    let db_path = dir.join("v4_test.db");

    let store: Arc<dyn Store> = SqliteStore::open_path(&db_path);
    store.ensure_schema("kv", &["k", "v"]);
    let mut c = Cursor::default();
    c.set("k", "hello"); c.set("v", "world");
    store.insert_many("kv", vec![c], 0).await;
    store.commit(0).await;

    // Reopen the same file in a new connection — confirms on-disk persistence.
    let store2: Arc<dyn Store> = SqliteStore::open_path(&db_path);
    store2.ensure_schema("kv", &["k", "v"]);
    let snap = store2.snapshot("kv").await;
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].get("k"), Some("hello"));

    let _ = std::fs::remove_dir_all(&dir);
}
