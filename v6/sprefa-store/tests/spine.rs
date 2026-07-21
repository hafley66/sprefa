//! ONE test. The golden path of acceptance, at scale. No pile of trivial green
//! checks — each narrow piece is pushed to scale ONCE, then the whole pipeline
//! runs together and the acceptance properties are asserted on the result.
//!
//! Pieces proven at scale (Phase 1): resident interner, content dedup, unified
//! node insert, unified edge insert. Whole-system acceptance (Phase 2): a repo
//! with a worktree and a background checkout, each carrying a committed HEAD rev
//! and an uncommitted WORK rev, a file byte-identical across revs, cross-family
//! reachability, and full FK integrity.

use std::time::Instant;

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sprefa_store::transforms::content_hash;
use sprefa_store::{stmt_counter, EdgeRow, NodeRow, SpanRow, Store};

const N_STRINGS: usize = 100_000;
const N_FILES: usize = 5_000;
const N_NODES: usize = 50_000;
const N_EDGES: usize = N_NODES - 1;

async fn scalar_i64(store: &Store, sql: &str) -> i64 {
    let row = store
        .db()
        .query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, sql.to_owned()))
        .await
        .unwrap()
        .unwrap();
    row.try_get_by_index::<i64>(0).unwrap()
}

#[tokio::test]
async fn golden_path_at_scale() {
    let store = Store::open("sqlite::memory:").await.unwrap();

    // ---- Phase 1a: resident interner at scale --------------------------------
    let t = Instant::now();
    let mut ids = Vec::with_capacity(N_STRINGS);
    for i in 0..N_STRINGS {
        ids.push(store.intern(&format!("ident_{i}")));
    }
    // dense + contiguous from 0
    assert_eq!(*ids.first().unwrap(), 0);
    assert_eq!(*ids.last().unwrap(), (N_STRINGS - 1) as i64);
    // resolve roundtrip on a sample
    assert_eq!(store.resolve(0).as_deref(), Some("ident_0"));
    assert_eq!(
        store.resolve((N_STRINGS - 1) as i64).as_deref(),
        Some(&format!("ident_{}", N_STRINGS - 1)[..])
    );
    // re-intern is a no-op id-wise
    assert_eq!(store.intern("ident_42"), 42);
    let flushed = store.flush_strings().await.unwrap();
    assert_eq!(flushed, N_STRINGS, "every distinct string persisted once");
    // a second flush writes nothing (nothing new interned)
    assert_eq!(store.intern("ident_42"), 42);
    assert_eq!(store.flush_strings().await.unwrap(), 0);
    eprintln!("[1a] interned {N_STRINGS} strings in {:?}", t.elapsed());

    // ---- Phase 1b: content dedup at scale ------------------------------------
    let t = Instant::now();
    let files: Vec<([u8; 16], i64, i64)> = (0..N_FILES)
        .map(|i| (content_hash(format!("file body {i}").as_bytes()), i as i64, 10))
        .collect();
    store.files_insert_batch(&files).await.unwrap();
    assert_eq!(scalar_i64(&store, "SELECT count(*) FROM files").await, N_FILES as i64);
    // re-inserting identical content adds nothing
    store.files_insert_batch(&files).await.unwrap();
    assert_eq!(
        scalar_i64(&store, "SELECT count(*) FROM files").await,
        N_FILES as i64,
        "byte-identical content dedups to one row"
    );
    let some_file = store.file_id_of(files[0].0).await.unwrap().unwrap();
    eprintln!("[1b] inserted {N_FILES} deduped files in {:?}", t.elapsed());

    // ---- Phase 1c: unified node insert at scale (4 families in one table) -----
    let t = Instant::now();
    let name_id = ids[7];
    let nodes: Vec<NodeRow> = (0..N_NODES)
        .map(|i| NodeRow {
            node_id: i as i64,
            family: (i % 4) as i32,
            file_id: some_file,
            byte_start: (i * 13) as i64,
            byte_len: 5,
            kind: (i % 9) as i32,
            name_id: Some(name_id),
        })
        .collect();
    stmt_counter::reset();
    store.nodes_insert_batch(&nodes).await.unwrap();
    let node_stmts = stmt_counter::get();
    assert_eq!(scalar_i64(&store, "SELECT count(*) FROM node").await, N_NODES as i64);
    // all four families landed in the ONE table
    assert_eq!(scalar_i64(&store, "SELECT count(DISTINCT family) FROM node").await, 4);
    // NOT N+1: 50,000 rows cost ceil(N/100) statements, not one per row.
    let expected_stmts = (N_NODES as u64).div_ceil(100);
    assert_eq!(
        node_stmts, expected_stmts,
        "batched insert issued {node_stmts} statements for {N_NODES} rows (expected {expected_stmts}); an N+1 would be {N_NODES}"
    );
    eprintln!("[1c] inserted {N_NODES} nodes in {node_stmts} statements (not {N_NODES}) in {:?}", t.elapsed());

    // ---- Phase 1d: unified edge insert at scale (cross-family chain) ----------
    let t = Instant::now();
    let edges: Vec<EdgeRow> = (0..N_EDGES)
        .map(|i| EdgeRow {
            family: (i % 4) as i32, // edge family varies; endpoints span families
            src_id: i as i64,
            dst_id: (i + 1) as i64,
            kind: 0,
        })
        .collect();
    store.edges_insert_batch(&edges).await.unwrap();
    assert_eq!(scalar_i64(&store, "SELECT count(*) FROM edge").await, N_EDGES as i64);
    eprintln!("[1d] inserted {N_EDGES} cross-family edges in {:?}", t.elapsed());

    // ---- Phase 1e: cross-family reachability, ONE recursive CTE, at scale -----
    let t = Instant::now();
    let reached = scalar_i64(
        &store,
        "WITH RECURSIVE reach(n) AS (\
           SELECT 0 \
           UNION \
           SELECT e.dst_id FROM edge e JOIN reach ON e.src_id = reach.n\
         ) SELECT count(*) FROM reach",
    )
    .await;
    assert_eq!(
        reached, N_NODES as i64,
        "one CTE over the unified edge table reaches every node across all families"
    );
    eprintln!("[1e] reached {reached} nodes via one cross-family CTE in {:?}", t.elapsed());

    // ---- Phase 2: whole-system acceptance ------------------------------------
    // A repo with a worktree and a background checkout; each has a committed HEAD
    // and an uncommitted WORK; one file is byte-identical across both revs.
    let repo = store.repo_upsert("acme/widgets", "/repo", "https://x/y").await.unwrap();

    let wt_path = store.intern("/checkouts/worktree-a");
    let bg_path = store.intern("/checkouts/background-b");
    let shared_path = store.intern("src/shared.rs"); // ONE path, referenced by two revs
    store.flush_strings().await.unwrap();

    let worktree = store.root_insert(repo, wt_path).await.unwrap();
    let background = store.root_insert(repo, bg_path).await.unwrap();

    let head = store
        .rev_committed(repo, sprefa_store::transforms::git_sha_bytes(&"ab".repeat(20)).unwrap())
        .await
        .unwrap();
    let work_wt = store.rev_work(repo, worktree, head).await.unwrap();
    let work_bg = store.rev_work(repo, background, head).await.unwrap();
    // one WORK rev per root: asking again returns the same id, not a new row
    assert_eq!(store.rev_work(repo, worktree, head).await.unwrap(), work_wt);
    assert_ne!(work_wt, work_bg);
    assert_ne!(head, work_wt);

    // The same file content lives at HEAD and at the worktree's WORK, same path.
    let shared_hash = content_hash(b"pub fn shared() {}");
    store.files_insert_batch(&[(shared_hash, 18, 1)]).await.unwrap();
    let shared_file = store.file_id_of(shared_hash).await.unwrap().unwrap();
    store
        .place_files_batch(&[
            (head, shared_path, shared_file),
            (work_wt, shared_path, shared_file),
        ])
        .await
        .unwrap();
    // a span into that content, naming an interned identifier. Intern + flush
    // BEFORE the insert so the string_id FK into `strings` resolves.
    let span_name = store.intern("shared");
    store.flush_strings().await.unwrap();
    store
        .spans_insert_batch(&[SpanRow {
            file_id: shared_file,
            start: 7,
            end: 13,
            string_id: Some(span_name),
        }])
        .await
        .unwrap();

    // ---- Acceptance assertions ----------------------------------------------
    // A. content dedup: identical bytes at two revs = ONE files row for it
    assert_eq!(
        scalar_i64(
            &store,
            &format!("SELECT count(*) FROM files WHERE file_id = {shared_file}")
        )
        .await,
        1
    );
    // ...placed at exactly two (rev, path) locations
    assert_eq!(
        scalar_i64(
            &store,
            &format!("SELECT count(*) FROM revs_files WHERE file_id = {shared_file}")
        )
        .await,
        2
    );
    // B. the path is stored ONCE, referenced by both placements
    assert_eq!(
        scalar_i64(
            &store,
            &format!("SELECT count(*) FROM strings WHERE string_id = {shared_path}")
        )
        .await,
        1
    );
    assert_eq!(
        scalar_i64(
            &store,
            &format!("SELECT count(*) FROM revs_files WHERE path_string_id = {shared_path}")
        )
        .await,
        2
    );
    // C. WORK vs HEAD shape: HEAD has a sha and no root; WORK has a root and no sha
    assert_eq!(
        scalar_i64(
            &store,
            &format!(
                "SELECT count(*) FROM repo_revs WHERE rev_id = {head} \
                 AND git_sha IS NOT NULL AND root_id IS NULL AND kind = 0"
            )
        )
        .await,
        1
    );
    assert_eq!(
        scalar_i64(
            &store,
            &format!(
                "SELECT count(*) FROM repo_revs WHERE rev_id = {work_wt} \
                 AND git_sha IS NULL AND root_id = {worktree} AND kind = 1"
            )
        )
        .await,
        1
    );
    // D. FK integrity: enforcement is ON and nothing dangles
    let fk_violations = store
        .db()
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "PRAGMA foreign_key_check".to_owned(),
        ))
        .await
        .unwrap();
    assert!(fk_violations.is_empty(), "no dangling foreign keys");
    // E. dense string ids: max == count - 1, no gaps, no hash-valued ids
    let max_sid = scalar_i64(&store, "SELECT max(string_id) FROM strings").await;
    let cnt_sid = scalar_i64(&store, "SELECT count(*) FROM strings").await;
    assert_eq!(max_sid, cnt_sid - 1, "string ids are dense (Codd + varint happy)");

    eprintln!("[2] whole-system acceptance passed");

    // ---- Phase 3: WORK unstaged-change detection at scale --------------------
    // A repo whose HEAD has N_WORK files; a WORK rev diverges DIRTY of them. The
    // "which files have unstaged changes" answer must be ONE join returning
    // exactly the DIRTY paths — not a per-file compare.
    const N_WORK: usize = 20_000;
    const DIRTY: usize = 137;
    let repo2 = store.repo_upsert("acme/scale", "/s", "https://s").await.unwrap();
    let head2 = store
        .rev_committed(repo2, sprefa_store::transforms::git_sha_bytes(&"cd".repeat(20)).unwrap())
        .await
        .unwrap();
    let root2 = {
        let p = store.intern("/checkouts/scale");
        store.flush_strings().await.unwrap();
        store.root_insert(repo2, p).await.unwrap()
    };
    let work2 = store.rev_work(repo2, root2, head2).await.unwrap();

    // intern N_WORK paths + 2*N_WORK content bodies (HEAD body + DIRTY body)
    let paths: Vec<i64> = (0..N_WORK).map(|i| store.intern(&format!("s/f{i}.rs"))).collect();
    store.flush_strings().await.unwrap();
    let head_hashes: Vec<[u8; 16]> = (0..N_WORK).map(|i| content_hash(format!("head body {i}").as_bytes())).collect();
    // the first DIRTY files get a different body at WORK; the rest are identical
    let work_hashes: Vec<[u8; 16]> = (0..N_WORK)
        .map(|i| {
            if i < DIRTY {
                content_hash(format!("WORK edit {i}").as_bytes())
            } else {
                head_hashes[i]
            }
        })
        .collect();

    let mut all_content: Vec<([u8; 16], i64, i64)> = head_hashes.iter().map(|h| (*h, 1, 1)).collect();
    all_content.extend(work_hashes.iter().filter(|h| !head_hashes.contains(h)).map(|h| (*h, 1, 1)));
    store.files_insert_batch(&all_content).await.unwrap();

    // resolve every hash in batch (NOT per file) and place HEAD + WORK
    let mut want = head_hashes.clone();
    want.extend(work_hashes.iter().copied());
    let idmap = store.file_ids_by_hashes(&want).await.unwrap();
    let head_place: Vec<(i64, i64, i64)> =
        (0..N_WORK).map(|i| (head2, paths[i], idmap[&head_hashes[i]])).collect();
    let work_place: Vec<(i64, i64, i64)> =
        (0..N_WORK).map(|i| (work2, paths[i], idmap[&work_hashes[i]])).collect();
    store.place_files_batch(&head_place).await.unwrap();
    store.place_files_batch(&work_place).await.unwrap();

    // THE query: unstaged changes, one join, counted for N+1
    let t = Instant::now();
    stmt_counter::reset();
    let dirty = store.unstaged_path_ids(work2).await.unwrap();
    let dirty_stmts = stmt_counter::get();
    assert_eq!(dirty.len(), DIRTY, "exactly the diverged files report unstaged changes");
    assert_eq!(dirty_stmts, 1, "unstaged detection is ONE join, not one query per file");
    // it is exactly the first DIRTY paths
    let mut got = dirty.clone();
    got.sort();
    let mut expect: Vec<i64> = paths[..DIRTY].to_vec();
    expect.sort();
    assert_eq!(got, expect, "the reported paths are exactly the edited ones");
    eprintln!(
        "[3] {DIRTY} unstaged of {N_WORK} files found in {dirty_stmts} query, {:?}",
        t.elapsed()
    );
}
