use std::collections::{HashMap, HashSet};

use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_config::{NormalizeConfig, RepoConfig};
use sprefa_index::{normalize, normalize2, ExtractedFile};

// SQLite's bundled libsqlite3 in sqlx supports up to 32766 bound params.
// Keep chunks well under that ceiling.
const STR_CHUNK: usize = 2000;  // 3 params each  -> 6000 per stmt
const FILE_CHUNK: usize = 2000; // 5 params each  -> 10000 per stmt
const REF_CHUNK: usize = 1000;  // 8 params each  -> 8000 per stmt

// All foreign keys resolved to real DB ids -- ready for bulk insert.
struct ResolvedRef {
    string_id: i64,
    file_id: i64,
    span_start: i64,
    span_end: i64,
    is_path: bool,
    ref_kind: i64,
    parent_key_string_id: Option<i64>,
    node_path: Option<String>,
}

#[tracing::instrument(skip(db, config, files, normalize_config), fields(repo = %config.name, branch = %branch, file_count = files.len()))]
pub async fn flush(
    db: &SqlitePool,
    config: &RepoConfig,
    branch: &str,
    files: Vec<ExtractedFile>,
    normalize_config: Option<&NormalizeConfig>,
    scanner_hash: &str,
) -> Result<usize> {
    // Two metadata upserts outside the main transaction (idempotent, tiny).
    let repo_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO repos (name, root_path) VALUES (?, ?)
         ON CONFLICT(name) DO UPDATE SET root_path = excluded.root_path
         RETURNING id",
    )
    .bind(&config.name)
    .bind(&config.path)
    .fetch_one(db)
    .await?;

    let is_wt = super::is_wt_branch(branch);
    sqlx::query(
        "INSERT INTO repo_branches (repo_id, branch, is_working_tree) VALUES (?, ?, ?)
         ON CONFLICT(repo_id, branch) DO UPDATE SET is_working_tree = excluded.is_working_tree",
    )
    .bind(repo_id)
    .bind(branch)
    .bind(is_wt)
    .execute(db)
    .await?;

    if files.is_empty() {
        return Ok(0);
    }

    // -- Phase 1: deduplicate strings entirely in Rust --
    let mut string_seen: HashSet<String> = HashSet::new();
    let mut unique_strings: Vec<String> = Vec::new();
    for file in &files {
        for r in &file.refs {
            if string_seen.insert(r.value.clone()) {
                unique_strings.push(r.value.clone());
            }
            if let Some(pk) = &r.parent_key {
                if string_seen.insert(pk.clone()) {
                    unique_strings.push(pk.clone());
                }
            }
        }
    }

    // -- Phase 2: one transaction, all DB writes --
    let mut tx = db.begin().await?;

    // Bulk insert unique strings.
    let string_data: Vec<(String, String, Option<String>)> = unique_strings.iter()
        .map(|v| {
            let norm = normalize(v);
            let norm2 = normalize_config.and_then(|c| normalize2(v, c));
            (v.clone(), norm, norm2)
        })
        .collect();

    for chunk in string_data.chunks(STR_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!("INSERT OR IGNORE INTO strings (value, norm, norm2) VALUES {ph}");
        let mut q = sqlx::query(&sql);
        for (v, n, n2) in chunk {
            q = q.bind(v.as_str()).bind(n.as_str()).bind(n2.as_deref());
        }
        q.execute(&mut *tx).await?;
    }

    // Read back string id -> value in one pass (chunked IN).
    let mut string_id_map: HashMap<String, i64> = HashMap::with_capacity(unique_strings.len());
    for chunk in unique_strings.chunks(STR_CHUNK) {
        let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, value FROM strings WHERE value IN ({ph})");
        let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
        for v in chunk { q = q.bind(v.as_str()); }
        for (id, value) in q.fetch_all(&mut *tx).await? {
            string_id_map.insert(value, id);
        }
    }

    // Bulk insert files.
    for chunk in files.chunks(FILE_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT INTO files (repo_id, path, content_hash, stem, ext, scanner_hash) VALUES {ph}
             ON CONFLICT(repo_id, path, content_hash) DO UPDATE SET scanner_hash = excluded.scanner_hash"
        );
        let mut q = sqlx::query(&sql);
        for f in chunk {
            q = q.bind(repo_id).bind(&f.rel_path).bind(&f.content_hash)
                 .bind(f.stem.as_deref()).bind(f.ext.as_deref())
                 .bind(scanner_hash);
        }
        q.execute(&mut *tx).await?;
    }

    // Read back all file ids for this repo in one query.
    let file_id_map: HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT path, id FROM files WHERE repo_id = ?"
    )
    .bind(repo_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    // Bulk insert branch_files.
    // Working-tree branches do full-replace: the scanner knows the complete
    // on-disk file set, so stale entries must be removed. Committed branches
    // stay additive (files accumulate across incremental fetches).
    if is_wt {
        sqlx::query("DELETE FROM branch_files WHERE repo_id = ? AND branch = ?")
            .bind(repo_id)
            .bind(branch)
            .execute(&mut *tx)
            .await?;
    }

    let branch_file_ids: Vec<i64> = files.iter()
        .filter_map(|f| file_id_map.get(&f.rel_path).copied())
        .collect();

    for chunk in branch_file_ids.chunks(FILE_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT OR IGNORE INTO branch_files (repo_id, branch, file_id) VALUES {ph}"
        );
        let mut q = sqlx::query(&sql);
        for file_id in chunk {
            q = q.bind(repo_id).bind(branch).bind(file_id);
        }
        q.execute(&mut *tx).await?;
    }

    // Resolve all ref foreign keys in Rust.
    let mut resolved_refs: Vec<ResolvedRef> = Vec::new();
    for file in &files {
        let file_id = match file_id_map.get(&file.rel_path) {
            Some(&id) => id,
            None => continue,
        };
        for r in &file.refs {
            let string_id = match string_id_map.get(&r.value) {
                Some(&id) => id,
                None => continue,
            };
            resolved_refs.push(ResolvedRef {
                string_id,
                file_id,
                span_start: r.span_start as i64,
                span_end: r.span_end as i64,
                is_path: r.is_path,
                ref_kind: r.kind.as_u8() as i64,
                parent_key_string_id: r.parent_key.as_ref()
                    .and_then(|pk| string_id_map.get(pk).copied()),
                node_path: r.node_path.clone(),
            });
        }
    }

    // Bulk insert refs, count inserted via changes().
    let mut refs_inserted = 0usize;
    for chunk in resolved_refs.chunks(REF_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?,?,?,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT OR IGNORE INTO refs
             (string_id, file_id, span_start, span_end, is_path, ref_kind,
              parent_key_string_id, node_path)
             VALUES {ph}"
        );
        let mut q = sqlx::query(&sql);
        for r in chunk {
            q = q.bind(r.string_id).bind(r.file_id)
                 .bind(r.span_start).bind(r.span_end)
                 .bind(r.is_path).bind(r.ref_kind)
                 .bind(r.parent_key_string_id).bind(r.node_path.as_deref());
        }
        q.execute(&mut *tx).await?;
        let changes: i64 = sqlx::query_scalar("SELECT changes()")
            .fetch_one(&mut *tx).await?;
        refs_inserted += changes as usize;
    }

    tx.commit().await?;
    Ok(refs_inserted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_extract::RawRef;
    use sprefa_index::ExtractedFile;
    use sprefa_schema::{init_db, RefKind};

    async fn make_db() -> SqlitePool {
        init_db(":memory:").await.unwrap()
    }

    fn repo_config(name: &str) -> RepoConfig {
        RepoConfig {
            name: name.to_string(),
            path: "/tmp/test".to_string(),
            branches: None,
            filter: None,
            branch_overrides: None,
        }
    }

    fn raw_ref(value: &str, kind: RefKind) -> RawRef {
        RawRef {
            value: value.to_string(),
            span_start: 0,
            span_end: 0,
            kind,
            is_path: false,
            parent_key: None,
            node_path: None,
        }
    }

    fn extracted(rel_path: &str, content_hash: &str, ext: &str, refs: Vec<RawRef>) -> ExtractedFile {
        let stem = rel_path.split('/').last()
            .and_then(|n| n.split('.').next())
            .map(String::from);
        ExtractedFile {
            rel_path: rel_path.to_string(),
            content_hash: content_hash.to_string(),
            stem,
            ext: Some(ext.to_string()),
            refs,
            was_skipped: false,
        }
    }

    fn skipped(rel_path: &str, content_hash: &str, ext: &str) -> ExtractedFile {
        let stem = rel_path.split('/').last()
            .and_then(|n| n.split('.').next())
            .map(String::from);
        ExtractedFile {
            rel_path: rel_path.to_string(),
            content_hash: content_hash.to_string(),
            stem,
            ext: Some(ext.to_string()),
            refs: vec![],
            was_skipped: true,
        }
    }

    #[tokio::test]
    async fn flush_inserts_strings_and_refs() {
        let db = make_db().await;
        let files = vec![
            extracted("package.json", "abc123", "json", vec![
                raw_ref("express", RefKind::DepName),
                raw_ref("lodash", RefKind::DepName),
            ]),
        ];

        let inserted = flush(&db, &repo_config("myrepo"), "main", files, None, "v1").await.unwrap();
        assert_eq!(inserted, 2);

        let string_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strings")
            .fetch_one(&db).await.unwrap();
        assert_eq!(string_count, 2);

        let ref_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
            .fetch_one(&db).await.unwrap();
        assert_eq!(ref_count, 2);
    }

    #[tokio::test]
    async fn flush_deduplicates_strings_across_files() {
        let db = make_db().await;

        // "express" appears in two files -- should produce one strings row, two refs rows
        let files = vec![
            extracted("a/package.json", "hash1", "json", vec![raw_ref("express", RefKind::DepName)]),
            extracted("b/package.json", "hash2", "json", vec![raw_ref("express", RefKind::DepName)]),
        ];

        let inserted = flush(&db, &repo_config("myrepo"), "main", files, None, "v1").await.unwrap();
        assert_eq!(inserted, 2);

        let string_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strings")
            .fetch_one(&db).await.unwrap();
        assert_eq!(string_count, 1);

        let ref_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
            .fetch_one(&db).await.unwrap();
        assert_eq!(ref_count, 2);
    }

    #[tokio::test]
    async fn flush_links_parent_key() {
        let db = make_db().await;
        let files = vec![
            extracted("package.json", "abc", "json", vec![
                RawRef {
                    value: "4.18.2".to_string(),
                    span_start: 0,
                    span_end: 0,
                    kind: RefKind::DepVersion,
                    is_path: false,
                    parent_key: Some("express".to_string()),
                    node_path: Some("dependencies/express/version".to_string()),
                },
                raw_ref("express", RefKind::DepName),
            ]),
        ];

        flush(&db, &repo_config("myrepo"), "main", files, None, "v1").await.unwrap();

        // Both "express" and "4.18.2" should be in strings
        let string_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strings")
            .fetch_one(&db).await.unwrap();
        assert_eq!(string_count, 2);

        // The version ref should have parent_key_string_id pointing at "express"
        let parent_linked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM refs WHERE parent_key_string_id IS NOT NULL"
        )
        .fetch_one(&db).await.unwrap();
        assert_eq!(parent_linked, 1);

        let node_path: Option<String> = sqlx::query_scalar(
            "SELECT node_path FROM refs WHERE node_path IS NOT NULL"
        )
        .fetch_optional(&db).await.unwrap().flatten();
        assert_eq!(node_path.as_deref(), Some("dependencies/express/version"));
    }

    #[tokio::test]
    async fn flush_is_idempotent() {
        let db = make_db().await;

        let make_files = || vec![
            extracted("package.json", "abc", "json", vec![raw_ref("express", RefKind::DepName)]),
        ];

        flush(&db, &repo_config("myrepo"), "main", make_files(), None, "v1").await.unwrap();
        let second = flush(&db, &repo_config("myrepo"), "main", make_files(), None, "v1").await.unwrap();

        assert_eq!(second, 0);

        let ref_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
            .fetch_one(&db).await.unwrap();
        assert_eq!(ref_count, 1);
    }

    #[tokio::test]
    async fn flush_handles_many_files() {
        let db = make_db().await;

        let files: Vec<ExtractedFile> = (0..3000).map(|i| extracted(
            &format!("src/file_{i}.ts"),
            &format!("hash_{i}"),
            "ts",
            vec![
                raw_ref(&format!("dep_{i}"), RefKind::DepName),
                raw_ref("shared-dep", RefKind::DepName),
            ],
        )).collect();

        let inserted = flush(&db, &repo_config("bigmono"), "main", files, None, "v1").await.unwrap();

        // 3000 unique dep_N + 1 shared = 3001 strings, 6000 refs
        assert_eq!(inserted, 6000);

        let string_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strings")
            .fetch_one(&db).await.unwrap();
        assert_eq!(string_count, 3001);
    }

    #[tokio::test]
    async fn flush_stores_scanner_hash_on_files() {
        let db = make_db().await;
        let files = vec![
            extracted("src/a.ts", "hash1", "ts", vec![raw_ref("lodash", RefKind::DepName)]),
        ];

        flush(&db, &repo_config("myrepo"), "main", files, None, "binary-v1").await.unwrap();

        let scanner_hash: Option<String> =
            sqlx::query_scalar("SELECT scanner_hash FROM files WHERE path = 'src/a.ts'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(scanner_hash.as_deref(), Some("binary-v1"));
    }

    #[tokio::test]
    async fn flush_skipped_files_get_branch_files_but_no_new_refs() {
        let db = make_db().await;

        // First scan: insert a file with refs.
        let initial = vec![
            extracted("src/a.ts", "hash1", "ts", vec![raw_ref("lodash", RefKind::DepName)]),
        ];
        flush(&db, &repo_config("myrepo"), "main", initial, None, "binary-v1").await.unwrap();

        let refs_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
            .fetch_one(&db).await.unwrap();

        // Second scan: same file, marked as skipped (same binary hash).
        let second = vec![skipped("src/a.ts", "hash1", "ts")];
        let inserted = flush(&db, &repo_config("myrepo"), "feature", second, None, "binary-v1").await.unwrap();

        // No new refs inserted.
        assert_eq!(inserted, 0);
        let refs_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
            .fetch_one(&db).await.unwrap();
        assert_eq!(refs_after, refs_before);

        // Branch_files entry created for the new branch.
        let branch_file_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM branch_files WHERE branch = 'feature'"
        )
        .fetch_one(&db).await.unwrap();
        assert_eq!(branch_file_count, 1);
    }

    #[tokio::test]
    async fn flush_wt_sets_is_working_tree() {
        let db = make_db().await;

        let files_a = vec![extracted("src/a.ts", "h1", "ts", vec![raw_ref("x", RefKind::DepName)])];
        let files_b = vec![extracted("src/a.ts", "h1", "ts", vec![raw_ref("x", RefKind::DepName)])];

        flush(&db, &repo_config("myrepo"), "main", files_a, None, "v1").await.unwrap();
        flush(&db, &repo_config("myrepo"), "main+wt", files_b, None, "v1").await.unwrap();

        let committed: (i64,) = sqlx::query_as(
            "SELECT is_working_tree FROM repo_branches WHERE branch = 'main'"
        ).fetch_one(&db).await.unwrap();
        assert_eq!(committed.0, 0);

        let wt: (i64,) = sqlx::query_as(
            "SELECT is_working_tree FROM repo_branches WHERE branch = 'main+wt'"
        ).fetch_one(&db).await.unwrap();
        assert_eq!(wt.0, 1);
    }

    #[tokio::test]
    async fn flush_wt_replaces_branch_files() {
        let db = make_db().await;

        // First wt flush: a.ts + b.ts
        let files1 = vec![
            extracted("src/a.ts", "h1", "ts", vec![raw_ref("x", RefKind::DepName)]),
            extracted("src/b.ts", "h2", "ts", vec![raw_ref("y", RefKind::DepName)]),
        ];
        flush(&db, &repo_config("myrepo"), "main+wt", files1, None, "v1").await.unwrap();

        let count1: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM branch_files WHERE branch = 'main+wt'"
        ).fetch_one(&db).await.unwrap();
        assert_eq!(count1, 2);

        // Second wt flush: b.ts + c.ts (a.ts removed from disk)
        let files2 = vec![
            extracted("src/b.ts", "h2", "ts", vec![raw_ref("y", RefKind::DepName)]),
            extracted("src/c.ts", "h3", "ts", vec![raw_ref("z", RefKind::DepName)]),
        ];
        flush(&db, &repo_config("myrepo"), "main+wt", files2, None, "v1").await.unwrap();

        let count2: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM branch_files WHERE branch = 'main+wt'"
        ).fetch_one(&db).await.unwrap();
        assert_eq!(count2, 2);

        // a.ts should be gone, c.ts should be present
        let paths: Vec<String> = sqlx::query_scalar(
            "SELECT f.path FROM branch_files bf JOIN files f ON bf.file_id = f.id WHERE bf.branch = 'main+wt' ORDER BY f.path"
        ).fetch_all(&db).await.unwrap();
        assert_eq!(paths, vec!["src/b.ts", "src/c.ts"]);
    }

    #[tokio::test]
    async fn flush_committed_is_additive() {
        let db = make_db().await;

        let files1 = vec![
            extracted("src/a.ts", "h1", "ts", vec![raw_ref("x", RefKind::DepName)]),
        ];
        flush(&db, &repo_config("myrepo"), "main", files1, None, "v1").await.unwrap();

        let files2 = vec![
            extracted("src/b.ts", "h2", "ts", vec![raw_ref("y", RefKind::DepName)]),
        ];
        flush(&db, &repo_config("myrepo"), "main", files2, None, "v1").await.unwrap();

        // Both should survive -- committed flush is additive
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM branch_files WHERE branch = 'main'"
        ).fetch_one(&db).await.unwrap();
        assert_eq!(count, 2);

        let paths: Vec<String> = sqlx::query_scalar(
            "SELECT f.path FROM branch_files bf JOIN files f ON bf.file_id = f.id WHERE bf.branch = 'main' ORDER BY f.path"
        ).fetch_all(&db).await.unwrap();
        assert_eq!(paths, vec!["src/a.ts", "src/b.ts"]);
    }

    // -- scope filtering tests --

    use sprefa_schema::BranchScope;

    /// Populate committed (main) with file_a containing "alpha", and wt (main+wt) with
    /// file_b containing "beta". Both share "shared".
    async fn seed_scoped_db() -> SqlitePool {
        let db = make_db().await;

        let committed_files = vec![
            extracted("src/a.ts", "ha", "ts", vec![
                raw_ref("alpha", RefKind::ImportName),
                raw_ref("shared", RefKind::ImportName),
            ]),
        ];
        flush(&db, &repo_config("myrepo"), "main", committed_files, None, "v1").await.unwrap();

        let wt_files = vec![
            extracted("src/b.ts", "hb", "ts", vec![
                raw_ref("beta", RefKind::ImportName),
                raw_ref("shared", RefKind::ImportName),
            ]),
        ];
        flush(&db, &repo_config("myrepo"), "main+wt", wt_files, None, "v1").await.unwrap();

        db
    }

    #[tokio::test]
    async fn query_committed_excludes_wt_refs() {
        let db = seed_scoped_db().await;

        // "shared" exists in both branches, but committed scope should only return a.ts
        let hits = sprefa_schema::search_refs(&db, "shared", Some(BranchScope::Committed)).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].refs.len(), 1);
        assert_eq!(hits[0].refs[0].file_path, "src/a.ts");

        // "beta" only exists in wt, committed should find nothing
        let hits = sprefa_schema::search_refs(&db, "beta", Some(BranchScope::Committed)).await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn query_local_returns_only_wt_refs() {
        let db = seed_scoped_db().await;

        // "shared" in local scope should only return b.ts
        let hits = sprefa_schema::search_refs(&db, "shared", Some(BranchScope::Local)).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].refs.len(), 1);
        assert_eq!(hits[0].refs[0].file_path, "src/b.ts");

        // "alpha" only exists in committed, local should find nothing
        let hits = sprefa_schema::search_refs(&db, "alpha", Some(BranchScope::Local)).await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn query_default_is_committed() {
        let db = seed_scoped_db().await;

        // None defaults to Committed -- only committed branch refs
        let hits = sprefa_schema::search_refs(&db, "shared", None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].refs.len(), 1);
        assert_eq!(hits[0].refs[0].file_path, "src/a.ts");

        // "beta" only in wt, default (committed) should not find it
        let hits = sprefa_schema::search_refs(&db, "beta", None).await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn query_all_returns_both_tiers() {
        let db = seed_scoped_db().await;

        // Explicit All returns refs from both branches
        let hits = sprefa_schema::search_refs(&db, "shared", Some(BranchScope::All)).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].refs.len(), 2);

        let alpha = sprefa_schema::search_refs(&db, "alpha", Some(BranchScope::All)).await.unwrap();
        assert_eq!(alpha.len(), 1);
        let beta = sprefa_schema::search_refs(&db, "beta", Some(BranchScope::All)).await.unwrap();
        assert_eq!(beta.len(), 1);
    }
}
