use anyhow::{bail, Result};
use sqlx::SqlitePool;
use sprefa_rules::LinkRule;

/// Execute all link rules to create edges in match_links.
///
/// Each [`LinkRule`] supplies a raw SQL WHERE fragment that is injected into
/// a fixed query skeleton. See the doc comment on [`sprefa_rules::RuleSet::link_rules`]
/// for the full skeleton, available column aliases, and examples.
///
/// ## Skeleton (reproduced here for grep-ability)
///
/// ```sql
/// INSERT OR IGNORE INTO match_links (source_match_id, target_match_id, link_kind)
/// SELECT src_m.id, tgt_m.id, '<link_rule.kind>'
/// FROM matches src_m
/// JOIN refs    src_r  ON src_m.ref_id     = src_r.id
/// JOIN strings src_s  ON src_r.string_id  = src_s.id
/// JOIN files   src_f  ON src_r.file_id    = src_f.id
/// JOIN repos   src_rp ON src_f.repo_id    = src_rp.id
///
/// JOIN matches tgt_m  ON tgt_m.id != src_m.id
/// JOIN refs    tgt_r  ON tgt_m.ref_id     = tgt_r.id
/// JOIN strings tgt_s  ON tgt_r.string_id  = tgt_s.id
/// JOIN files   tgt_f  ON tgt_r.file_id    = tgt_f.id
///
/// WHERE src_rp.name = :repo_name
///   AND NOT EXISTS (
///       SELECT 1 FROM match_links ml
///       WHERE ml.source_match_id = src_m.id AND ml.link_kind = '<link_rule.kind>'
///   )
///   AND (<link_rule.sql>)
/// ```
///
/// ## WARNING: raw SQL injection
///
/// `link_rule.sql` is interpolated directly. This is a local developer
/// toolchain -- the tradeoff is full SQL expressiveness for prototyping.
/// Never expose this to untrusted input.
///
/// Idempotent: the NOT EXISTS guard + INSERT OR IGNORE means re-running
/// is cheap and only processes unlinked matches.
///
/// Returns the total number of new links created across all rules.
#[tracing::instrument(skip(db, link_rules), fields(repo = %repo_name))]
pub async fn resolve_match_links(
    db: &SqlitePool,
    repo_name: &str,
    link_rules: &[LinkRule],
) -> Result<usize> {
    let mut total = 0;

    for rule in link_rules {
        let label = &rule.kind;

        // Resolve the WHERE fragment from either predicate DSL or raw sql.
        let user_sql = match (&rule.predicate, &rule.sql) {
            (Some(pred), None) => sprefa_rules::link_compile::compile(pred),
            (None, Some(sql)) => sql.clone(),
            (Some(_), Some(_)) => bail!("link rule '{}': both sql and predicate set", label),
            (None, None) => bail!("link rule '{}': neither sql nor predicate set", label),
        };

        let query = format!(
            "INSERT OR IGNORE INTO match_links (source_match_id, target_match_id, link_kind)
             SELECT src_m.id, tgt_m.id, '{kind}'
             FROM matches src_m
             JOIN refs    src_r  ON src_m.ref_id     = src_r.id
             JOIN strings src_s  ON src_r.string_id  = src_s.id
             JOIN files   src_f  ON src_r.file_id    = src_f.id
             JOIN repos   src_rp ON src_f.repo_id    = src_rp.id

             JOIN matches tgt_m  ON tgt_m.id != src_m.id
             JOIN refs    tgt_r  ON tgt_m.ref_id     = tgt_r.id
             JOIN strings tgt_s  ON tgt_r.string_id  = tgt_s.id
             JOIN files   tgt_f  ON tgt_r.file_id    = tgt_f.id

             WHERE src_rp.name = ?
               AND NOT EXISTS (
                   SELECT 1 FROM match_links ml
                   WHERE ml.source_match_id = src_m.id AND ml.link_kind = '{kind}'
               )
               AND ({user_sql})",
            kind = rule.kind,
        );

        let result = sqlx::query(&query)
            .bind(repo_name)
            .execute(db)
            .await;

        match result {
            Ok(r) => {
                let count = r.rows_affected() as usize;
                if count > 0 {
                    tracing::debug!("{}: link rule '{}' created {} links", repo_name, label, count);
                }
                total += count;
            }
            Err(e) => {
                tracing::error!(
                    "{}: link rule '{}' failed: {}. SQL fragment was: {}",
                    repo_name, label, e, user_sql
                );
                return Err(e.into());
            }
        }
    }

    if total > 0 {
        tracing::debug!("{}: {} total match links created", repo_name, total);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_schema::init_db;

    async fn make_db() -> SqlitePool {
        init_db(":memory:").await.unwrap()
    }

    async fn seed_repo(db: &SqlitePool, name: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO repos (name, root_path) VALUES (?, '/tmp') RETURNING id")
            .bind(name)
            .fetch_one(db)
            .await
            .unwrap()
    }

    async fn seed_file(db: &SqlitePool, repo_id: i64, path: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO files (repo_id, path, content_hash) VALUES (?, ?, 'h') RETURNING id",
        )
        .bind(repo_id)
        .bind(path)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn seed_string(db: &SqlitePool, value: &str) -> i64 {
        sqlx::query("INSERT OR IGNORE INTO strings (value, norm) VALUES (?, ?)")
            .bind(value)
            .bind(value.to_lowercase())
            .execute(db)
            .await
            .unwrap();
        sqlx::query_scalar("SELECT id FROM strings WHERE value = ?")
            .bind(value)
            .fetch_one(db)
            .await
            .unwrap()
    }

    /// Seed a ref + match, optionally with target_file_id. Returns (ref_id, match_id).
    async fn seed_ref_match(
        db: &SqlitePool,
        file_id: i64,
        value: &str,
        kind: &str,
        target_file_id: Option<i64>,
    ) -> (i64, i64) {
        let string_id = seed_string(db, value).await;
        let ref_id: i64 = sqlx::query_scalar(
            "INSERT INTO refs (string_id, file_id, span_start, span_end, is_path, target_file_id)
             VALUES (?, ?, 0, 0, 0, ?) RETURNING id",
        )
        .bind(string_id)
        .bind(file_id)
        .bind(target_file_id)
        .fetch_one(db)
        .await
        .unwrap();
        let match_id: i64 = sqlx::query_scalar(
            "INSERT INTO matches (ref_id, rule_name, kind) VALUES (?, 'test', ?) RETURNING id",
        )
        .bind(ref_id)
        .bind(kind)
        .fetch_one(db)
        .await
        .unwrap();
        (ref_id, match_id)
    }

    fn import_binding_rule() -> LinkRule {
        LinkRule {
            kind: "import_binding".into(),
            sql: Some("src_m.kind = 'import_name' AND tgt_m.kind = 'export_name' AND src_r.target_file_id = tgt_r.file_id AND tgt_r.string_id = src_r.string_id".into()),
            predicate: None,
        }
    }

    #[tokio::test]
    async fn links_import_to_export() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app").await;
        let file_a = seed_file(&db, repo_id, "src/app.ts").await;
        let file_b = seed_file(&db, repo_id, "src/utils.ts").await;

        let (_export_ref, export_match) =
            seed_ref_match(&db, file_b, "foo", "export_name", None).await;
        let (_import_ref, import_match) =
            seed_ref_match(&db, file_a, "foo", "import_name", Some(file_b)).await;

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()]).await.unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> = sqlx::query_as(
            "SELECT source_match_id, target_match_id, link_kind FROM match_links",
        )
        .fetch_optional(&db)
        .await
        .unwrap();
        assert_eq!(row, Some((import_match, export_match, "import_binding".to_string())));
    }

    #[tokio::test]
    async fn idempotent() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app").await;
        let file_a = seed_file(&db, repo_id, "src/app.ts").await;
        let file_b = seed_file(&db, repo_id, "src/utils.ts").await;

        seed_ref_match(&db, file_b, "foo", "export_name", None).await;
        seed_ref_match(&db, file_a, "foo", "import_name", Some(file_b)).await;

        let rules = [import_binding_rule()];
        let first = resolve_match_links(&db, "app", &rules).await.unwrap();
        let second = resolve_match_links(&db, "app", &rules).await.unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 0);
    }

    #[tokio::test]
    async fn no_link_without_resolved_target() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app").await;
        let file_a = seed_file(&db, repo_id, "src/app.ts").await;
        let file_b = seed_file(&db, repo_id, "src/utils.ts").await;

        seed_ref_match(&db, file_b, "foo", "export_name", None).await;
        seed_ref_match(&db, file_a, "foo", "import_name", None).await;

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()]).await.unwrap();
        assert_eq!(linked, 0);
    }

    #[tokio::test]
    async fn no_link_when_name_differs() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app").await;
        let file_a = seed_file(&db, repo_id, "src/app.ts").await;
        let file_b = seed_file(&db, repo_id, "src/utils.ts").await;

        seed_ref_match(&db, file_b, "bar", "export_name", None).await;
        seed_ref_match(&db, file_a, "foo", "import_name", Some(file_b)).await;

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()]).await.unwrap();
        assert_eq!(linked, 0);
    }

    /// Test a custom link rule: cross-repo norm-based linking.
    #[tokio::test]
    async fn custom_norm_link_rule() {
        let db = make_db().await;
        let repo_a = seed_repo(&db, "consumer").await;
        let repo_b = seed_repo(&db, "provider").await;
        let file_a = seed_file(&db, repo_a, "values.yaml").await;
        let file_b = seed_file(&db, repo_b, "package.json").await;

        // "MyOrg/Api" in values.yaml, "myorg/api" in package.json -- same norm
        let (_, src_match) = seed_ref_match(&db, file_a, "MyOrg/Api", "image_repo", None).await;
        let (_, tgt_match) = seed_ref_match(&db, file_b, "myorg/api", "package_name", None).await;

        let rule = LinkRule {
            kind: "image_source".into(),
            sql: Some("src_m.kind = 'image_repo' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm".into()),
            predicate: None,
        };

        let linked = resolve_match_links(&db, "consumer", &[rule]).await.unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> = sqlx::query_as(
            "SELECT source_match_id, target_match_id, link_kind FROM match_links",
        )
        .fetch_optional(&db)
        .await
        .unwrap();
        assert_eq!(row, Some((src_match, tgt_match, "image_source".to_string())));
    }

    /// Multiple link rules execute in sequence.
    #[tokio::test]
    async fn multiple_rules() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app").await;
        let file_a = seed_file(&db, repo_id, "src/app.ts").await;
        let file_b = seed_file(&db, repo_id, "src/utils.ts").await;

        seed_ref_match(&db, file_b, "foo", "export_name", None).await;
        seed_ref_match(&db, file_a, "foo", "import_name", Some(file_b)).await;
        seed_ref_match(&db, file_b, "utils-lib", "package_name", None).await;
        seed_ref_match(&db, file_a, "utils-lib", "dep_name", None).await;

        let rules = [
            import_binding_rule(),
            LinkRule {
                kind: "dependency".into(),
                sql: Some("src_m.kind = 'dep_name' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm".into()),
                predicate: None,
            },
        ];

        let linked = resolve_match_links(&db, "app", &rules).await.unwrap();
        assert_eq!(linked, 2);

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM match_links")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }
}
