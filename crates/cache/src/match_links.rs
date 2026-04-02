use anyhow::{bail, Result};
use sprefa_rules::LinkRule;
use sqlx::SqlitePool;

#[cfg(test)]
use sprefa_rules::{LinkPredicate, Side};

/// Execute all link rules to create edges in match_links.
///
/// Each [`LinkRule`] supplies a raw SQL WHERE fragment that is injected into
/// a fixed query skeleton. See the doc comment on [`sprefa_rules::DerivedRules::link_rules`]
/// for the full skeleton, available column aliases, and examples.
///
/// ## Skeleton (reproduced here for grep-ability)
///
/// Source side: always file-backed (INNER JOINs).
/// Target side: LEFT JOINs to support both file-backed and repo_ref-backed matches.
/// The CHECK on matches guarantees exactly one of (ref_id, repo_ref_id) is non-null.
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
/// JOIN matches       tgt_m  ON tgt_m.id != src_m.id
/// LEFT JOIN refs      tgt_r  ON tgt_m.ref_id      = tgt_r.id
/// LEFT JOIN repo_refs tgt_rr ON tgt_m.repo_ref_id  = tgt_rr.id
/// JOIN strings       tgt_s  ON COALESCE(tgt_r.string_id, tgt_rr.string_id) = tgt_s.id
/// LEFT JOIN files     tgt_f  ON tgt_r.file_id      = tgt_f.id
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

        // Optional target repo scoping: JOIN repos on the target side and
        // add an IN clause to restrict which repos can be link targets.
        let (tgt_repo_join, tgt_repo_where, tgt_repo_binds) = if let Some(repos) =
            &rule.target_repos
        {
            if repos.is_empty() {
                (String::new(), String::new(), vec![])
            } else {
                let placeholders: Vec<&str> = repos.iter().map(|_| "?").collect();
                (
                        "\n             JOIN repos tgt_rp ON COALESCE(tgt_f.repo_id, tgt_rr.repo_id) = tgt_rp.id".into(),
                        format!("\n               AND tgt_rp.name IN ({})", placeholders.join(", ")),
                        repos.clone(),
                    )
            }
        } else {
            (String::new(), String::new(), vec![])
        };

        let query = format!(
            "INSERT OR IGNORE INTO match_links (source_match_id, target_match_id, link_kind)
            SELECT src_m.id, tgt_m.id, '{kind}'
            FROM matches src_m
            JOIN refs    src_r  ON src_m.ref_id     = src_r.id
            JOIN strings src_s  ON src_r.string_id  = src_s.id
            JOIN files   src_f  ON src_r.file_id    = src_f.id
            JOIN repos   src_rp ON src_f.repo_id    = src_rp.id

            JOIN matches        tgt_m  ON tgt_m.id != src_m.id
            LEFT JOIN refs      tgt_r  ON tgt_m.ref_id      = tgt_r.id
            LEFT JOIN repo_refs tgt_rr ON tgt_m.repo_ref_id  = tgt_rr.id
            JOIN strings        tgt_s  ON COALESCE(tgt_r.string_id, tgt_rr.string_id) = tgt_s.id
            LEFT JOIN files     tgt_f  ON tgt_r.file_id      = tgt_f.id{tgt_repo_join}

            WHERE src_rp.name = ?
              AND NOT EXISTS (
                  SELECT 1 FROM match_links ml
                  WHERE ml.source_match_id = src_m.id AND ml.link_kind = '{kind}'
              ){tgt_repo_where}
              AND ({user_sql})",
            kind = rule.kind,
        );

        let mut q = sqlx::query(&query).bind(repo_name);
        for repo in &tgt_repo_binds {
            q = q.bind(repo);
        }
        let result = q.execute(db).await;

        match result {
            Ok(r) => {
                let count = r.rows_affected() as usize;
                if count > 0 {
                    tracing::debug!(
                        "{}: link rule '{}' created {} links",
                        repo_name,
                        label,
                        count
                    );
                }
                total += count;
            }
            Err(e) => {
                tracing::error!(
                    "{}: link rule '{}' failed: {}. SQL fragment was: {}",
                    repo_name,
                    label,
                    e,
                    user_sql
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
            "INSERT INTO matches (ref_id, repo_ref_id, rule_name, kind) VALUES (?, NULL, 'test', ?) RETURNING id",
        )
        .bind(ref_id)
        .bind(kind)
        .fetch_one(db)
        .await
        .unwrap();
        (ref_id, match_id)
    }

    /// Seed a repo_ref + match (repo-anchored, no file). Returns match_id.
    async fn seed_repo_ref_match(db: &SqlitePool, repo_id: i64, value: &str, kind: &str) -> i64 {
        let string_id = seed_string(db, value).await;
        let repo_ref_id: i64 = sqlx::query_scalar(
            "INSERT OR IGNORE INTO repo_refs (string_id, repo_id, kind) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(string_id)
        .bind(repo_id)
        .bind(kind)
        .fetch_one(db)
        .await
        .unwrap();
        let match_id: i64 = sqlx::query_scalar(
            "INSERT INTO matches (ref_id, repo_ref_id, rule_name, kind) VALUES (NULL, ?, '__meta__', ?) RETURNING id",
        )
        .bind(repo_ref_id)
        .bind(kind)
        .fetch_one(db)
        .await
        .unwrap();
        match_id
    }

    fn import_binding_rule() -> LinkRule {
        LinkRule {
            kind: "import_binding".into(),
            sql: Some("src_m.kind = 'import_name' AND tgt_m.kind = 'export_name' AND src_r.target_file_id = tgt_r.file_id AND tgt_r.string_id = src_r.string_id".into()),
            predicate: None,
            target_repos: None,
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

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()])
            .await
            .unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT source_match_id, target_match_id, link_kind FROM match_links")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert_eq!(
            row,
            Some((import_match, export_match, "import_binding".to_string()))
        );
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

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()])
            .await
            .unwrap();
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

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()])
            .await
            .unwrap();
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
            target_repos: None,
        };

        let linked = resolve_match_links(&db, "consumer", &[rule]).await.unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT source_match_id, target_match_id, link_kind FROM match_links")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert_eq!(
            row,
            Some((src_match, tgt_match, "image_source".to_string()))
        );
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
                target_repos: None,
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

    /// target_repos restricts which repos can be link targets.
    #[tokio::test]
    async fn target_repos_scopes_linking() {
        let db = make_db().await;
        let repo_src = seed_repo(&db, "consumer").await;
        let repo_a = seed_repo(&db, "provider-a").await;
        let repo_b = seed_repo(&db, "provider-b").await;
        let file_src = seed_file(&db, repo_src, "values.yaml").await;
        let file_a = seed_file(&db, repo_a, "package.json").await;
        let file_b = seed_file(&db, repo_b, "package.json").await;

        // Source repo has an image_repo match.
        seed_ref_match(&db, file_src, "myapp", "image_repo", None).await;
        // Both target repos have a matching package_name.
        seed_ref_match(&db, file_a, "myapp", "package_name", None).await;
        seed_ref_match(&db, file_b, "myapp", "package_name", None).await;

        // Scope to only provider-a.
        let rule = LinkRule {
            kind: "image_source".into(),
            sql: Some("src_m.kind = 'image_repo' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm".into()),
            predicate: None,
            target_repos: Some(vec!["provider-a".into()]),
        };

        let linked = resolve_match_links(&db, "consumer", &[rule]).await.unwrap();
        assert_eq!(linked, 1);

        // Verify only provider-a got linked, not provider-b.
        let tgt_file_id: i64 = sqlx::query_scalar(
            "SELECT tgt_r.file_id FROM match_links ml
             JOIN matches tgt_m ON ml.target_match_id = tgt_m.id
             JOIN refs tgt_r ON tgt_m.ref_id = tgt_r.id",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(tgt_file_id, file_a);
    }

    /// Predicate DSL form: dep_name -> package_name via norm_eq.
    /// Exercises compile() round-trip rather than raw sql.
    #[tokio::test]
    async fn dep_to_package_via_predicate() {
        let db = make_db().await;
        let repo_a = seed_repo(&db, "consumer").await;
        let repo_b = seed_repo(&db, "provider").await;
        let file_a = seed_file(&db, repo_a, "package.json").await;
        let file_b = seed_file(&db, repo_b, "package.json").await;

        // Case differs: dep uses "React", package declares "react" -- norm_eq should match.
        let (_, src_match) = seed_ref_match(&db, file_a, "React", "dep_name", None).await;
        let (_, tgt_match) = seed_ref_match(&db, file_b, "react", "package_name", None).await;

        let rule = LinkRule {
            kind: "dep_to_package".into(),
            sql: None,
            predicate: Some(LinkPredicate::And {
                all: vec![
                    LinkPredicate::KindEq {
                        side: Side::Src,
                        value: "dep_name".into(),
                    },
                    LinkPredicate::KindEq {
                        side: Side::Tgt,
                        value: "package_name".into(),
                    },
                    LinkPredicate::NormEq,
                ],
            }),
            target_repos: None,
        };

        let linked = resolve_match_links(&db, "consumer", &[rule]).await.unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT source_match_id, target_match_id, link_kind FROM match_links")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert_eq!(
            row,
            Some((src_match, tgt_match, "dep_to_package".to_string()))
        );
    }

    /// Predicate DSL form: env_var_ref -> env_var_name via norm_eq.
    #[tokio::test]
    async fn env_var_binding_via_predicate() {
        let db = make_db().await;
        let repo_src = seed_repo(&db, "app").await;
        let repo_infra = seed_repo(&db, "infra").await;
        let file_src = seed_file(&db, repo_src, "src/config.ts").await;
        let file_infra = seed_file(&db, repo_infra, "k8s/configmap.yaml").await;

        let (_, src_match) =
            seed_ref_match(&db, file_src, "DATABASE_URL", "env_var_ref", None).await;
        let (_, tgt_match) =
            seed_ref_match(&db, file_infra, "DATABASE_URL", "env_var_name", None).await;

        let rule = LinkRule {
            kind: "env_var_binding".into(),
            sql: None,
            predicate: Some(LinkPredicate::And {
                all: vec![
                    LinkPredicate::KindEq {
                        side: Side::Src,
                        value: "env_var_ref".into(),
                    },
                    LinkPredicate::KindEq {
                        side: Side::Tgt,
                        value: "env_var_name".into(),
                    },
                    LinkPredicate::NormEq,
                ],
            }),
            target_repos: None,
        };

        let linked = resolve_match_links(&db, "app", &[rule]).await.unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT source_match_id, target_match_id, link_kind FROM match_links")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert_eq!(
            row,
            Some((src_match, tgt_match, "env_var_binding".to_string()))
        );
    }

    /// target_repos: None links across all repos (default behavior).
    #[tokio::test]
    async fn no_target_repos_links_all() {
        let db = make_db().await;
        let repo_src = seed_repo(&db, "consumer").await;
        let repo_a = seed_repo(&db, "provider-a").await;
        let repo_b = seed_repo(&db, "provider-b").await;
        let file_src = seed_file(&db, repo_src, "values.yaml").await;
        let file_a = seed_file(&db, repo_a, "package.json").await;
        let file_b = seed_file(&db, repo_b, "package.json").await;

        seed_ref_match(&db, file_src, "myapp", "image_repo", None).await;
        seed_ref_match(&db, file_a, "myapp", "package_name", None).await;
        seed_ref_match(&db, file_b, "myapp", "package_name", None).await;

        let rule = LinkRule {
            kind: "image_source".into(),
            sql: Some("src_m.kind = 'image_repo' AND tgt_m.kind = 'package_name' AND src_s.norm = tgt_s.norm".into()),
            predicate: None,
            target_repos: None,
        };

        let linked = resolve_match_links(&db, "consumer", &[rule]).await.unwrap();
        assert_eq!(linked, 2);
    }

    /// Link a file-backed match to a repo_ref-backed match (image_tag -> git_tag).
    #[tokio::test]
    async fn links_to_repo_ref_target() {
        let db = make_db().await;
        let repo_src = seed_repo(&db, "infra").await;
        let repo_tgt = seed_repo(&db, "api").await;
        let file_src = seed_file(&db, repo_src, "deploy/values.yaml").await;

        // Source: deploy image tag in infra repo (file-backed)
        let (_, src_match) = seed_ref_match(&db, file_src, "v1.2.3", "deploy_image_tag", None).await;
        // Target: git tag in api repo (repo_ref-backed)
        let tgt_match = seed_repo_ref_match(&db, repo_tgt, "v1.2.3", "git_tag").await;

        let rule = LinkRule {
            kind: "image_tag_to_git_tag".into(),
            sql: None,
            predicate: Some(LinkPredicate::And {
                all: vec![
                    LinkPredicate::KindEq {
                        side: Side::Src,
                        value: "deploy_image_tag".into(),
                    },
                    LinkPredicate::KindEq {
                        side: Side::Tgt,
                        value: "git_tag".into(),
                    },
                    LinkPredicate::NormEq,
                ],
            }),
            target_repos: None,
        };

        let linked = resolve_match_links(&db, "infra", &[rule]).await.unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT source_match_id, target_match_id, link_kind FROM match_links")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert_eq!(
            row,
            Some((src_match, tgt_match, "image_tag_to_git_tag".to_string()))
        );
    }

    /// Link a file-backed match to a repo_ref-backed match with target_repos scoping.
    #[tokio::test]
    async fn repo_ref_target_with_target_repos_scope() {
        let db = make_db().await;
        let repo_src = seed_repo(&db, "infra").await;
        let repo_a = seed_repo(&db, "api").await;
        let repo_b = seed_repo(&db, "web").await;
        let file_src = seed_file(&db, repo_src, "deploy/values.yaml").await;

        seed_ref_match(&db, file_src, "v2.0.0", "deploy_image_tag", None).await;
        // Both repos have the same git tag
        seed_repo_ref_match(&db, repo_a, "v2.0.0", "git_tag").await;
        seed_repo_ref_match(&db, repo_b, "v2.0.0", "git_tag").await;

        // Scope to only api repo
        let rule = LinkRule {
            kind: "image_tag_to_git_tag".into(),
            sql: None,
            predicate: Some(LinkPredicate::And {
                all: vec![
                    LinkPredicate::KindEq {
                        side: Side::Src,
                        value: "deploy_image_tag".into(),
                    },
                    LinkPredicate::KindEq {
                        side: Side::Tgt,
                        value: "git_tag".into(),
                    },
                    LinkPredicate::NormEq,
                ],
            }),
            target_repos: Some(vec!["api".into()]),
        };

        let linked = resolve_match_links(&db, "infra", &[rule]).await.unwrap();
        assert_eq!(linked, 1);
    }

    /// Existing file-to-file links still work with the new LEFT JOIN skeleton.
    #[tokio::test]
    async fn file_to_file_links_unchanged() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app").await;
        let file_a = seed_file(&db, repo_id, "src/app.ts").await;
        let file_b = seed_file(&db, repo_id, "src/utils.ts").await;

        let (_, export_match) = seed_ref_match(&db, file_b, "foo", "export_name", None).await;
        let (_, import_match) =
            seed_ref_match(&db, file_a, "foo", "import_name", Some(file_b)).await;

        // Also seed a repo_ref to ensure it doesn't interfere
        seed_repo_ref_match(&db, repo_id, "app", "repo_name").await;

        let linked = resolve_match_links(&db, "app", &[import_binding_rule()])
            .await
            .unwrap();
        assert_eq!(linked, 1);

        let row: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT source_match_id, target_match_id, link_kind FROM match_links")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert_eq!(
            row,
            Some((import_match, export_match, "import_binding".to_string()))
        );
    }
}
