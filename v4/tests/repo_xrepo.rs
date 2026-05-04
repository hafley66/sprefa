// Repo source op: multi-root walk emitting cursors with REPO + FS.
// Composes with AstNm/Re downstream; supports Layer-2 probe across all
// roots; round-trips through SqliteStore partitioned by REPO.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::PathBuf;
use std::io::Write;
use ast_grep_language::SupportLang;
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

static SEQ: AtomicU64 = AtomicU64::new(0);
fn tempdir(label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("v4_repo_{}_{}_{}_{}", label, pid, nanos, seq));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write(p: &std::path::Path, body: &str) {
    let mut f = std::fs::File::create(p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
}

async fn drain(chain: Vec<Arc<dyn Op>>, h: Hooks) -> Vec<Cursor> {
    let input = futures::stream::iter(Vec::<Vec<Cursor>>::new()).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    out
}

#[tokio::test]
async fn repo_emits_cursor_per_file_with_repo_and_fs() {
    let r1 = tempdir("alpha");
    let r2 = tempdir("beta");
    write(&r1.join("a.rs"), "fn a() {}");
    write(&r1.join("z.rs"), "fn z() {}");
    write(&r2.join("b.rs"), "fn b() {}");
    write(&r2.join("c.txt"), "skip me");

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Repo::new(
            vec![("alpha".into(), r1.clone()), ("beta".into(), r2.clone())],
            vec!["rs".into()],
        )),
    ];
    let store: Arc<dyn Store> = MemStore::new();
    let out = drain(chain, hooks(store)).await;

    assert_eq!(out.len(), 3, "2 from alpha, 1 from beta, .txt filtered");
    let by_repo: std::collections::HashMap<&str, usize> = out.iter()
        .fold(std::collections::HashMap::new(), |mut m, c| {
            *m.entry(c.get("REPO").unwrap_or("?")).or_insert(0) += 1; m
        });
    assert_eq!(by_repo.get("alpha"), Some(&2));
    assert_eq!(by_repo.get("beta"),  Some(&1));
    for c in &out {
        assert!(c.get("FS").is_some(), "every cursor carries FS");
    }

    let _ = std::fs::remove_dir_all(&r1);
    let _ = std::fs::remove_dir_all(&r2);
}

#[tokio::test]
async fn repo_composes_with_ast_nm_attributes_matches_to_repo() {
    // Same pattern across two repos with different content. Each match
    // cursor inherits REPO from its source.
    let r1 = tempdir("svc1");
    let r2 = tempdir("svc2");
    write(&r1.join("a.rs"), "fn main() { println!(\"r1-1\"); println!(\"r1-2\"); }");
    write(&r2.join("a.rs"), "fn main() { println!(\"only-r2\"); }");

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Repo::new(
            vec![("svc1".into(), r1.clone()), ("svc2".into(), r2.clone())],
            vec!["rs".into()],
        )),
        Arc::new(AstNm::new("println!($$$)", SupportLang::Rust, &[])),
    ];
    let store: Arc<dyn Store> = MemStore::new();
    let out = drain(chain, hooks(store)).await;

    let r1_hits = out.iter().filter(|c| c.get("REPO") == Some("svc1")).count();
    let r2_hits = out.iter().filter(|c| c.get("REPO") == Some("svc2")).count();
    assert_eq!(r1_hits, 2);
    assert_eq!(r2_hits, 1);
    for c in &out {
        assert!(c.get("CONTENT_HASH").is_some(),
            "AstNm stamps CONTENT_HASH on every match cursor");
    }

    let _ = std::fs::remove_dir_all(&r1);
    let _ = std::fs::remove_dir_all(&r2);
}

#[tokio::test]
async fn repo_probe_changes_only_for_edited_repo() {
    let r1 = tempdir("probe1");
    let r2 = tempdir("probe2");
    write(&r1.join("a.rs"), "fn main() {}");
    write(&r2.join("b.rs"), "fn main() {}");

    let mk_repo = || -> Arc<dyn Op> {
        Arc::new(Repo::new(
            vec![("probe1".into(), r1.clone()), ("probe2".into(), r2.clone())],
            vec!["rs".into()],
        ))
    };
    let entries1 = mk_repo().probe().unwrap();
    assert_eq!(entries1.len(), 2);
    // Slug-prefix in keys keeps cross-repo paths distinct.
    assert!(entries1[0].0.starts_with("probe1::") || entries1[0].0.starts_with("probe2::"));

    // Edit r1's file only. Per-repo input set hash changes; r2 entries
    // remain bit-identical.
    write(&r1.join("a.rs"), "fn main() { let _ = 1; }");
    let entries2 = mk_repo().probe().unwrap();
    assert_eq!(entries2.len(), 2);
    let h_r1_before = entries1.iter().find(|(p, _)| p.starts_with("probe1::")).unwrap().1;
    let h_r1_after  = entries2.iter().find(|(p, _)| p.starts_with("probe1::")).unwrap().1;
    let h_r2_before = entries1.iter().find(|(p, _)| p.starts_with("probe2::")).unwrap().1;
    let h_r2_after  = entries2.iter().find(|(p, _)| p.starts_with("probe2::")).unwrap().1;
    assert_ne!(h_r1_before, h_r1_after, "edited repo's hash must change");
    assert_eq!(h_r2_before, h_r2_after, "untouched repo's hash must not change");

    let _ = std::fs::remove_dir_all(&r1);
    let _ = std::fs::remove_dir_all(&r2);
}

#[tokio::test]
async fn repo_to_factwrite_partitions_results_by_repo_in_sqlite() {
    // Cross-repo structured query: write all matches into one fact table
    // partitioned by REPO. Then query by REPO via FactRead.
    let r1 = tempdir("backend");
    let r2 = tempdir("frontend");
    write(&r1.join("svc.rs"), "fn handler() { println!(\"a\"); println!(\"b\"); }");
    write(&r2.join("ui.rs"),  "fn render() { println!(\"x\"); }");

    let store: Arc<dyn Store> = SqliteStore::open_memory();
    declare_fact(&store, "println_hits", &["REPO", "FS", "LO", "HI"]);

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Repo::new(
            vec![("backend".into(),  r1.clone()),
                 ("frontend".into(), r2.clone())],
            vec!["rs".into()],
        )),
        Arc::new(AstNm::new("println!($$$)", SupportLang::Rust, &[])),
        Arc::new(FactWrite::new("println_hits", &["REPO", "FS", "LO", "HI"])),
    ];
    let h = hooks(store.clone());
    drive_two_tick(chain, h, 8).await;

    // FactRead by REPO=backend should return 2 rows; frontend, 1.
    let backend_rows = store.read_in("println_hits", "REPO",
        vec!["backend".into()], vec!["FS".into(), "LO".into()]).await;
    let frontend_rows = store.read_in("println_hits", "REPO",
        vec!["frontend".into()], vec!["FS".into(), "LO".into()]).await;
    assert_eq!(backend_rows.len(), 2);
    assert_eq!(frontend_rows.len(), 1);

    let _ = std::fs::remove_dir_all(&r1);
    let _ = std::fs::remove_dir_all(&r2);
}
