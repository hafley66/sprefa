// Value-driven sources: Line + RepoFromTerm.
// Show: Sh::capture → Line → RepoFromTerm → AstNm → FactWrite, where
// the set of repos to scan is derived from data, not hardcoded.

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
    let p = std::env::temp_dir().join(format!("v4_vd_{}_{}_{}_{}", label, pid, nanos, seq));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn write_file(p: &std::path::Path, body: &str) {
    let mut f = std::fs::File::create(p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
}
async fn drain(chain: Vec<Arc<dyn Op>>, h: Hooks, seed: Vec<Vec<Cursor>>) -> Vec<Cursor> {
    let input = futures::stream::iter(seed).boxed();
    let mut s = chain.into_iter().fold(input, |acc, op| op.run(h.clone(), acc));
    let mut out = Vec::new();
    while let Some(b) = s.next().await { out.extend(b); }
    out
}

#[tokio::test]
async fn line_explodes_a_term() {
    let mut c = Cursor::default();
    c.set("OUT", "alpha\nbeta\ngamma");
    let chain: Vec<Arc<dyn Op>> = vec![Arc::new(Line::on("OUT"))];
    let store: Arc<dyn Store> = MemStore::new();
    let out = drain(chain, hooks(store), vec![vec![c]]).await;
    let vs: Vec<_> = out.iter().map(|c| c.get("OUT").unwrap().to_string()).collect();
    assert_eq!(vs, vec!["alpha", "beta", "gamma"]);
}

#[tokio::test]
async fn line_skips_empty_and_into_distinct_target() {
    let mut c = Cursor::default();
    c.set("OUT", "x\n\ny\n");
    c.set("KEEP", "context");
    let chain: Vec<Arc<dyn Op>> = vec![Arc::new(Line::on("OUT").into_term("ITEM"))];
    let store: Arc<dyn Store> = MemStore::new();
    let out = drain(chain, hooks(store), vec![vec![c]]).await;
    assert_eq!(out.len(), 2, "empty lines dropped");
    assert_eq!(out[0].get("ITEM"), Some("x"));
    assert_eq!(out[1].get("ITEM"), Some("y"));
    // Original OUT preserved on each emitted cursor (source != target).
    for c in &out {
        assert_eq!(c.get("OUT"), Some("x\n\ny\n"));
        assert_eq!(c.get("KEEP"), Some("context"));
    }
}

#[tokio::test]
async fn repo_from_term_walks_each_input_cursor_path() {
    let r1 = tempdir("alpha");
    let r2 = tempdir("beta");
    write_file(&r1.join("a.rs"), "fn a() {}");
    write_file(&r1.join("z.rs"), "fn z() {}");
    write_file(&r2.join("b.rs"), "fn b() {}");

    let mut c1 = Cursor::default(); c1.set("DIR", r1.display().to_string().as_str());
    let mut c2 = Cursor::default(); c2.set("DIR", r2.display().to_string().as_str());

    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(RepoFromTerm::new("DIR", vec!["rs".into()])),
    ];
    let store: Arc<dyn Store> = MemStore::new();
    let out = drain(chain, hooks(store), vec![vec![c1, c2]]).await;

    assert_eq!(out.len(), 3);
    let alpha = out.iter().filter(|c| c.get("REPO").map(|s| s.contains("alpha")).unwrap_or(false)).count();
    let beta  = out.iter().filter(|c| c.get("REPO").map(|s| s.contains("beta")).unwrap_or(false)).count();
    assert_eq!(alpha, 2);
    assert_eq!(beta,  1);
    // Parent's DIR term is preserved on every emitted cursor.
    for c in &out { assert!(c.get("DIR").is_some()); }

    let _ = std::fs::remove_dir_all(&r1);
    let _ = std::fs::remove_dir_all(&r2);
}

#[tokio::test]
async fn discover_repos_via_sh_then_walk_via_repo_from_term() {
    // Set up a "workspace" with two child dirs that look like repos
    // (have a .git inside) and one that doesn't.
    let workspace = tempdir("ws");
    let repo_a = workspace.join("repo_a");
    let repo_b = workspace.join("repo_b");
    let not_repo = workspace.join("not_a_repo");
    std::fs::create_dir_all(&repo_a).unwrap();
    std::fs::create_dir_all(&repo_b).unwrap();
    std::fs::create_dir_all(&not_repo).unwrap();
    std::fs::create_dir_all(repo_a.join(".git")).unwrap();
    std::fs::create_dir_all(repo_b.join(".git")).unwrap();
    write_file(&repo_a.join("lib.rs"),  "fn ra() { println!(\"a1\"); println!(\"a2\"); }");
    write_file(&repo_b.join("main.rs"), "fn rb() { println!(\"b1\"); }");
    write_file(&not_repo.join("ignored.rs"), "fn nope() { println!(\"x\"); }");

    let store: Arc<dyn Store> = SqliteStore::open_memory();
    declare_fact(&store, "println_hits", &["REPO", "FS"]);

    // ls of workspace → one cursor per child dir bound to LINE.
    // filter to dirs that contain .git → repos only.
    // RepoFromTerm walks each → emits REPO + FS per .rs file.
    // AstNm → matches → FactWrite into println_hits.
    let chain: Vec<Arc<dyn Op>> = vec![
        Arc::new(Seed::one()),
        Arc::new(Sh::emit_lines(&format!(
            "find {} -maxdepth 1 -mindepth 1 -type d", workspace.display()))),
        Arc::new(Sh::filter("[ -d ${LINE}/.git ]")),
        Arc::new(RepoFromTerm::new("LINE", vec!["rs".into()])),
        Arc::new(AstNm::new("println!($$$)", SupportLang::Rust, &[])),
        Arc::new(FactWrite::new("println_hits", &["REPO", "FS"])),
    ];
    let h = hooks(store.clone());
    drive_two_tick(chain, h, 8).await;

    let snap = store.snapshot("println_hits").await;
    // 2 from repo_a + 1 from repo_b = 3. not_a_repo dropped by filter.
    assert_eq!(snap.len(), 3, "got {:?}", snap.iter().map(|c| c.get("REPO")).collect::<Vec<_>>());
    let repo_names: std::collections::HashSet<_> = snap.iter()
        .filter_map(|c| c.get("REPO").map(|s| s.to_string())).collect();
    assert!(repo_names.contains("repo_a"));
    assert!(repo_names.contains("repo_b"));
    assert!(!repo_names.contains("not_a_repo"),
        "non-git dir must have been dropped by the [ -d .../.git ] filter");

    let _ = std::fs::remove_dir_all(&workspace);
}
