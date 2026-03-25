use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use memmap2::Mmap;
use rayon::prelude::*;
use sqlx::SqlitePool;
use xxhash_rust::xxh3::xxh3_128;

use sprefa_config::{CompiledFilter, NormalizeConfig, RepoConfig};
use sprefa_extract::{Extractor, RawRef};

use crate::{
    files::list_files,
    normalize::{normalize, normalize2},
};

pub struct Scanner {
    pub extractors: Vec<Box<dyn Extractor>>,
    pub db: SqlitePool,
    pub normalize_config: Option<NormalizeConfig>,
    pub global_filter: Option<sprefa_config::FilterConfig>,
}

pub struct ScanResult {
    pub repo: String,
    pub branch: String,
    pub files_scanned: usize,
    pub refs_inserted: usize,
}

pub(crate) struct ExtractedFile {
    pub rel_path: String,
    pub content_hash: String,
    pub stem: Option<String>,
    pub ext: Option<String>,
    pub refs: Vec<RawRef>,
}

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

// SQLite's bundled libsqlite3 in sqlx supports up to 32766 bound params.
// Keep chunks well under that ceiling.
const STR_CHUNK: usize = 2000;  // 3 params each  -> 6000 per stmt
const FILE_CHUNK: usize = 2000; // 5 params each  -> 10000 per stmt
const REF_CHUNK: usize = 1000;  // 8 params each  -> 8000 per stmt

impl Scanner {
    fn extractor_for(&self, ext: &str) -> Option<&dyn Extractor> {
        self.extractors
            .iter()
            .find(|e| e.extensions().contains(&ext))
            .map(|e| e.as_ref())
    }

    pub async fn scan_repo(&self, config: &RepoConfig, branch: &str) -> Result<ScanResult> {
        let repo_path = Path::new(&config.path);

        let filter_config = sprefa_config::resolve_filter(self.global_filter.as_ref(), config, branch);
        let compiled_filter = filter_config
            .as_ref()
            .map(CompiledFilter::compile)
            .transpose()?;

        let files = list_files(repo_path, compiled_filter.as_ref())?;
        tracing::info!("{}/{}: {} files", config.name, branch, files.len());

        // Parallel extract: pure Rust, zero DB contact.
        let extracted: Vec<ExtractedFile> = files
            .par_iter()
            .filter_map(|abs_path| {
                let rel = abs_path.strip_prefix(repo_path).ok()?.to_str()?;
                let ext = abs_path.extension().and_then(|e| e.to_str());
                let extractor = ext.and_then(|e| self.extractor_for(e))?;
                let file = std::fs::File::open(abs_path).ok()?;
                let mmap = unsafe { Mmap::map(&file).ok()? };
                let hash = format!("{:x}", xxh3_128(&mmap));
                let refs = extractor.extract(&mmap, rel);
                if refs.is_empty() {
                    return None;
                }
                Some(ExtractedFile {
                    rel_path: rel.to_string(),
                    content_hash: hash,
                    stem: abs_path.file_stem().and_then(|s| s.to_str()).map(String::from),
                    ext: ext.map(String::from),
                    refs,
                })
            })
            .collect();

        let refs_inserted = self.flush(config, branch, extracted).await?;

        Ok(ScanResult {
            repo: config.name.clone(),
            branch: branch.to_string(),
            files_scanned: files.len(),
            refs_inserted,
        })
    }

    pub(crate) async fn flush(
        &self,
        config: &RepoConfig,
        branch: &str,
        files: Vec<ExtractedFile>,
    ) -> Result<usize> {
        // Two metadata upserts outside the main transaction (idempotent, tiny).
        let repo_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO repos (name, root_path) VALUES (?, ?)
             ON CONFLICT(name) DO UPDATE SET root_path = excluded.root_path
             RETURNING id",
        )
        .bind(&config.name)
        .bind(&config.path)
        .fetch_one(&self.db)
        .await?;

        sqlx::query(
            "INSERT INTO repo_branches (repo_id, branch) VALUES (?, ?)
             ON CONFLICT(repo_id, branch) DO NOTHING",
        )
        .bind(repo_id)
        .bind(branch)
        .execute(&self.db)
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
        let mut tx = self.db.begin().await?;

        // Bulk insert unique strings.
        let string_data: Vec<(String, String, Option<String>)> = unique_strings.iter()
            .map(|v| {
                let norm = normalize(v);
                let norm2 = self.normalize_config.as_ref().and_then(|c| normalize2(v, c));
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
            let ph = chunk.iter().map(|_| "(?,?,?,?,?)").collect::<Vec<_>>().join(",");
            let sql = format!(
                "INSERT INTO files (repo_id, path, content_hash, stem, ext) VALUES {ph}
                 ON CONFLICT(repo_id, path, content_hash) DO UPDATE SET scanned_at = NULL"
            );
            let mut q = sqlx::query(&sql);
            for f in chunk {
                q = q.bind(repo_id).bind(&f.rel_path).bind(&f.content_hash)
                     .bind(f.stem.as_deref()).bind(f.ext.as_deref());
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_extract::RawRef;
    use sprefa_schema::{init_db, RefKind};

    async fn make_db() -> SqlitePool {
        init_db(":memory:").await.unwrap()
    }

    fn make_scanner(db: SqlitePool) -> Scanner {
        Scanner {
            extractors: vec![],
            db,
            normalize_config: None,
            global_filter: None,
        }
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

    #[tokio::test]
    async fn flush_inserts_strings_and_refs() {
        let db = make_db().await;
        let scanner = make_scanner(db.clone());

        let files = vec![
            ExtractedFile {
                rel_path: "package.json".to_string(),
                content_hash: "abc123".to_string(),
                stem: Some("package".to_string()),
                ext: Some("json".to_string()),
                refs: vec![
                    raw_ref("express", RefKind::DepName),
                    raw_ref("lodash", RefKind::DepName),
                ],
            },
        ];

        let inserted = scanner.flush(&repo_config("myrepo"), "main", files).await.unwrap();
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
        let scanner = make_scanner(db.clone());

        // "express" appears in two files -- should produce one strings row, two refs rows
        let files = vec![
            ExtractedFile {
                rel_path: "a/package.json".to_string(),
                content_hash: "hash1".to_string(),
                stem: Some("package".to_string()),
                ext: Some("json".to_string()),
                refs: vec![raw_ref("express", RefKind::DepName)],
            },
            ExtractedFile {
                rel_path: "b/package.json".to_string(),
                content_hash: "hash2".to_string(),
                stem: Some("package".to_string()),
                ext: Some("json".to_string()),
                refs: vec![raw_ref("express", RefKind::DepName)],
            },
        ];

        let inserted = scanner.flush(&repo_config("myrepo"), "main", files).await.unwrap();
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
        let scanner = make_scanner(db.clone());

        let files = vec![
            ExtractedFile {
                rel_path: "package.json".to_string(),
                content_hash: "abc".to_string(),
                stem: Some("package".to_string()),
                ext: Some("json".to_string()),
                refs: vec![
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
                ],
            },
        ];

        scanner.flush(&repo_config("myrepo"), "main", files).await.unwrap();

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
        let scanner = make_scanner(db.clone());

        let make_files = || vec![
            ExtractedFile {
                rel_path: "package.json".to_string(),
                content_hash: "abc".to_string(),
                stem: Some("package".to_string()),
                ext: Some("json".to_string()),
                refs: vec![raw_ref("express", RefKind::DepName)],
            },
        ];

        scanner.flush(&repo_config("myrepo"), "main", make_files()).await.unwrap();
        let second = scanner.flush(&repo_config("myrepo"), "main", make_files()).await.unwrap();

        // Second flush inserts 0 new refs (all duplicates)
        assert_eq!(second, 0);

        let ref_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
            .fetch_one(&db).await.unwrap();
        assert_eq!(ref_count, 1);
    }

    #[tokio::test]
    async fn flush_handles_many_files() {
        let db = make_db().await;
        let scanner = make_scanner(db.clone());

        // Generate enough files to cross chunk boundaries
        let files: Vec<ExtractedFile> = (0..3000).map(|i| ExtractedFile {
            rel_path: format!("src/file_{i}.ts"),
            content_hash: format!("hash_{i}"),
            stem: Some(format!("file_{i}")),
            ext: Some("ts".to_string()),
            refs: vec![
                raw_ref(&format!("dep_{i}"), RefKind::DepName),
                raw_ref("shared-dep", RefKind::DepName),
            ],
        }).collect();

        let inserted = scanner.flush(&repo_config("bigmono"), "main", files).await.unwrap();

        // 3000 unique dep_N + 1 shared = 3001 strings, 6000 refs
        assert_eq!(inserted, 6000);

        let string_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strings")
            .fetch_one(&db).await.unwrap();
        assert_eq!(string_count, 3001);
    }
}
