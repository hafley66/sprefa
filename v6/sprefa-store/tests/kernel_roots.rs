//! Real-repo acceptance: ingest `mm/` from THREE sibling Linux-kernel checkouts
//! (main / worktree / background) at one pinned rev, with a WORK edit in one.
//! ONE overloaded test that shows the model works on real data and answers the
//! open question: do we need the main/worktree/background RootKind enum?
//!
//! Skips (does not fail) when the kernel fixture is absent — set up with
//! `tests/fixtures/setup-kernel-roots.sh` (needs a Linux clone; LINUX_REPO=...).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sprefa_store::transforms::{content_hash, git_sha_bytes, normalize};
use sprefa_store::{stmt_counter, Store};

const PINNED: &str = "27fa82620cbaa89a7fc11ac3057701d598813e87";

async fn scalar(store: &Store, sql: &str) -> i64 {
    store
        .db()
        .query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, sql.to_owned()))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap()
}

/// Top-level `mm/*.c` and `mm/*.h` under a checkout root, sorted, as
/// (relpath like "mm/util.c", bytes).
fn ingest_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mm = root.join("mm");
    let mut out = Vec::new();
    let mut names: Vec<PathBuf> = fs::read_dir(&mm)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|s| s.to_str()), Some("c") | Some("h")))
        .collect();
    names.sort();
    for p in names {
        let bytes = fs::read(&p).unwrap();
        out.push((format!("mm/{}", p.file_name().unwrap().to_str().unwrap()), bytes));
    }
    out
}

/// Place every file of a checkout at `rev`: intern paths, flush, dedup content,
/// junction rows. The SAME code path for all three roots — nothing branches on
/// which kind of checkout it is. That uniformity is itself the enum answer.
///
/// No N+1: paths intern in memory, content inserts in ONE batch, and file ids
/// resolve in ONE batched `IN` query (`file_ids_by_hashes`), never one SELECT
/// per file.
async fn place_checkout(store: &Store, rev: i64, files: &[(String, Vec<u8>)]) {
    let mut path_ids = Vec::with_capacity(files.len());
    let mut hashes = Vec::with_capacity(files.len());
    let mut content = Vec::with_capacity(files.len());
    for (path, bytes) in files {
        path_ids.push(store.intern(path));
        let h = content_hash(bytes);
        hashes.push(h);
        content.push((h, bytes.len() as i64, 1i64));
    }
    store.flush_strings().await.unwrap();
    store.files_insert_batch(&content).await.unwrap();
    let idmap = store.file_ids_by_hashes(&hashes).await.unwrap();
    let rows: Vec<(i64, i64, i64)> = files
        .iter()
        .enumerate()
        .map(|(i, _)| (rev, path_ids[i], idmap[&hashes[i]]))
        .collect();
    store.place_files_batch(&rows).await.unwrap();
}

#[tokio::test]
async fn kernel_three_roots_work_vs_head() {
    // Build (or refresh) the fixture; skip cleanly if there is no Linux clone.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("bash")
        .arg(here.join("tests/fixtures/setup-kernel-roots.sh"))
        .output()
        .expect("run setup script");
    if out.status.code() == Some(3) {
        eprintln!("SKIP: {}", String::from_utf8_lossy(&out.stderr).trim());
        return;
    }
    assert!(out.status.success(), "fixture setup failed: {}", String::from_utf8_lossy(&out.stderr));
    let roots_dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());

    let store = Store::open("sqlite::memory:").await.unwrap();
    let repo = store.repo_upsert("torvalds/linux", "/ext/linux", "https://x/linux").await.unwrap();
    let head = store.rev_committed(repo, git_sha_bytes(PINNED).unwrap()).await.unwrap();

    // ---- register the three sibling roots (same repo, distinct paths) --------
    // No RootKind: a root is just (repo, path). All three register identically.
    let mut root_ids = Vec::new();
    for name in ["main", "worktree", "background"] {
        let abs = roots_dir.join(name);
        let path_id = store.intern(abs.to_str().unwrap());
        store.flush_strings().await.unwrap();
        root_ids.push(store.root_insert(repo, path_id).await.unwrap());
    }

    // ---- ingest HEAD content from the two CLEAN roots ------------------------
    // main and worktree are the SAME commit with clean trees, so their files are
    // byte-identical. Ingesting both must collapse: one committed rev, one row
    // per (rev, path). (background's tree is dirty — it is the WORK state below,
    // which is exactly the real distinction between the roots.)
    let main_files = ingest_files(&roots_dir.join("main"));
    let distinct_paths = main_files.len() as i64;
    let distinct_content = main_files.iter().map(|(_, b)| content_hash(b)).collect::<BTreeSet<_>>().len() as i64;

    for name in ["main", "worktree"] {
        let files = ingest_files(&roots_dir.join(name));
        place_checkout(&store, head, &files).await;
    }

    let head_files = scalar(&store, "SELECT count(*) FROM files").await;
    let head_junctions = scalar(&store, &format!("SELECT count(*) FROM revs_files WHERE rev_id = {head}")).await;
    assert_eq!(head_files, distinct_content, "identical content across the two clean roots dedups to distinct-content rows");
    assert_eq!(head_junctions, distinct_paths, "two same-rev roots collapse to ONE (rev,path) row per file");
    eprintln!(
        "[HEAD] 2 clean roots x {distinct_paths} files -> {head_files} content rows, {head_junctions} junction rows (roots collapsed)"
    );

    // ---- WORK: the background root's dirty tree (util.c diverged) -------------
    let bg_root = root_ids[2];
    let work = store.rev_work(repo, bg_root, head).await.unwrap();
    let files_before = scalar(&store, "SELECT count(*) FROM files").await;
    let bg_files = ingest_files(&roots_dir.join("background"));
    place_checkout(&store, work, &bg_files).await;
    let files_after = scalar(&store, "SELECT count(*) FROM files").await;

    // exactly ONE new content row (the edited util.c); every other WORK file is
    // byte-identical to HEAD and reuses its file_id.
    assert_eq!(files_after - files_before, 1, "only the edited file is new content at WORK");
    let work_junctions = scalar(&store, &format!("SELECT count(*) FROM revs_files WHERE rev_id = {work}")).await;
    assert_eq!(work_junctions, distinct_paths, "WORK places every file, dirty or not");

    // util.c: WORK file_id differs from HEAD; an unedited file shares its file_id.
    let util_path = store.intern("mm/util.c");
    let mmap_path = store.intern("mm/mmap.c");
    store.flush_strings().await.unwrap();
    let util_head = scalar(&store, &format!("SELECT file_id FROM revs_files WHERE rev_id={head} AND path_string_id={util_path}")).await;
    let util_work = scalar(&store, &format!("SELECT file_id FROM revs_files WHERE rev_id={work} AND path_string_id={util_path}")).await;
    let mmap_head = scalar(&store, &format!("SELECT file_id FROM revs_files WHERE rev_id={head} AND path_string_id={mmap_path}")).await;
    let mmap_work = scalar(&store, &format!("SELECT file_id FROM revs_files WHERE rev_id={work} AND path_string_id={mmap_path}")).await;
    assert_ne!(util_head, util_work, "the edited file diverges: WORK != HEAD");
    assert_eq!(mmap_head, mmap_work, "an unedited file shares one content row across HEAD and WORK");
    eprintln!("[WORK] util.c HEAD file_id={util_head} != WORK file_id={util_work}; mmap.c shared file_id={mmap_head}");

    // the path "mm/util.c" is stored ONCE though referenced by HEAD and WORK
    assert_eq!(scalar(&store, &format!("SELECT count(*) FROM strings WHERE string_id={util_path}")).await, 1);
    assert_eq!(scalar(&store, &format!("SELECT count(*) FROM revs_files WHERE path_string_id={util_path}")).await, 2);

    // the WORK concept doing its job: which files have unstaged changes? ONE join
    // (v1's `<sha>+` done relationally via base_rev_id), and it is exactly util.c.
    stmt_counter::reset();
    let unstaged = store.unstaged_path_ids(work).await.unwrap();
    assert_eq!(stmt_counter::get(), 1, "unstaged detection is one join, not per-file");
    assert_eq!(unstaged, vec![util_path], "exactly mm/util.c has unstaged changes");
    eprintln!("[WORK] unstaged files detected in 1 query: {} path(s)", unstaged.len());

    // ---- normalization on REAL kernel identifiers ----------------------------
    let util_bytes = fs::read(roots_dir.join("main/mm/util.c")).unwrap();
    let idents = real_identifiers(&util_bytes);
    assert!(idents.len() > 200, "found real identifiers to normalize: {}", idents.len());
    // idempotent + case/underscore-collapsing on EVERY real identifier
    for id in &idents {
        let n = normalize(id);
        assert_eq!(normalize(&n), n, "normalize idempotent for {id:?}");
        assert_eq!(normalize(&id.to_uppercase()), n, "case-insensitive for {id:?}");
        assert_eq!(normalize(&id.replace('_', "")), n, "punct-insensitive for {id:?}");
    }
    // a concrete real example
    let sample: Vec<_> = idents.iter().filter(|s| s.contains('_')).take(3).collect();
    for s in &sample {
        eprintln!("[norm] {s:?} -> {:?}", normalize(s));
    }
    // two real spelling-variants collapse to one bucket
    assert_eq!(normalize("kvmalloc_node"), normalize("KVMallocNode"));

    // ---- FK integrity + dense ids -------------------------------------------
    let fk = store
        .db()
        .query_all_raw(Statement::from_string(DatabaseBackend::Sqlite, "PRAGMA foreign_key_check".to_owned()))
        .await
        .unwrap();
    assert!(fk.is_empty(), "no dangling foreign keys over real data");
    let max_sid = scalar(&store, "SELECT max(string_id) FROM strings").await;
    let cnt_sid = scalar(&store, "SELECT count(*) FROM strings").await;
    assert_eq!(max_sid, cnt_sid - 1, "dense string ids over real paths + identifiers");

    // ---- the enum verdict, now enacted ---------------------------------------
    // RootKind is GONE. Every root registered and ingested through the identical
    // path; the only thing that distinguished a checkout was WORK, anchored by
    // root identity (root_id) + base_rev_id, never a kind. This test is the
    // standing evidence: a root is (repo, path), and dirtiness is a join.
    eprintln!("[enum] no RootKind exists; roots collapsed on shared rev; WORK carried the only real distinction.");
}

/// Extract C identifier tokens (`[A-Za-z_][A-Za-z0-9_]*`) from real source.
fn real_identifiers(bytes: &[u8]) -> BTreeSet<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = BTreeSet::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            cur.push(ch);
        } else {
            if cur.len() >= 3 && !cur.chars().next().unwrap().is_ascii_digit() {
                out.insert(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    out
}
