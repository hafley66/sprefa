use sqlx::SqlitePool;

/// A ref affected by a file move or declaration rename.
/// Contains everything the planner needs to compute an edit.
#[derive(Debug, Clone)]
pub struct AffectedRef {
    pub ref_id: i64,
    pub span_start: u32,
    pub span_end: u32,
    pub value: String,
    pub source_file_rel: String,
    pub source_repo_root: String,
}

impl AffectedRef {
    pub fn source_abs_path(&self) -> String {
        format!("{}/{}", self.source_repo_root, self.source_file_rel)
    }
}

fn to_affected(rows: Vec<(i64, i64, i64, String, String, String)>) -> Vec<AffectedRef> {
    rows.into_iter()
        .map(|(id, ss, se, val, fp, rr)| AffectedRef {
            ref_id: id,
            span_start: ss as u32,
            span_end: se as u32,
            value: val,
            source_file_rel: fp,
            source_repo_root: rr,
        })
        .collect()
}

/// All ImportPath refs whose target_file_id points at the given file.
/// These are the import strings that need rewriting when the target moves.
pub async fn import_paths_targeting(
    pool: &SqlitePool,
    target_file_id: i64,
) -> anyhow::Result<Vec<AffectedRef>> {
    let rows: Vec<(i64, i64, i64, String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.span_start, r.span_end, s.value, f.path, repos.root_path
        FROM refs r
        JOIN strings s ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        WHERE r.target_file_id = ?
          AND r.ref_kind = 10
        "#,
    )
    .bind(target_file_id)
    .fetch_all(pool)
    .await?;
    Ok(to_affected(rows))
}

/// All ImportName refs with a specific name, in files that also have an
/// ImportPath targeting the given file.
///
/// This is the set of `import { Name }` refs that need rewriting when
/// `Name` is renamed in the target file.
///
/// Limitation: if a source file imports the same name from two different
/// modules and only one renames it, both refs get returned. The false
/// positive rate is low in practice (same name from two sources is rare).
pub async fn import_names_from_file(
    pool: &SqlitePool,
    target_file_id: i64,
    name: &str,
) -> anyhow::Result<Vec<AffectedRef>> {
    let rows: Vec<(i64, i64, i64, String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.span_start, r.span_end, s.value, f.path, repos.root_path
        FROM refs r
        JOIN strings s ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        WHERE r.ref_kind = 11
          AND s.value = ?
          AND r.file_id IN (
              SELECT r2.file_id FROM refs r2
              WHERE r2.target_file_id = ?
                AND r2.ref_kind = 10
          )
        "#,
    )
    .bind(name)
    .bind(target_file_id)
    .fetch_all(pool)
    .await?;
    Ok(to_affected(rows))
}

/// All RsUse refs in the same repo as the given file.
///
/// The caller resolves super::/self:: to absolute form in Rust and filters
/// by module path. This avoids the SQL prefix-matching gap where relative
/// paths (super::, self::) can't be matched by string comparison alone.
///
/// Typical repo has hundreds to low thousands of RsUse refs, so fetching
/// all and filtering in Rust is cheaper than complex SQL with per-file
/// module path resolution.
pub async fn all_rs_uses_in_repo(
    pool: &SqlitePool,
    file_id: i64,
) -> anyhow::Result<Vec<AffectedRef>> {
    let rows: Vec<(i64, i64, i64, String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.span_start, r.span_end, s.value, f.path, repos.root_path
        FROM refs r
        JOIN strings s ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        WHERE r.ref_kind = 30
          AND f.repo_id = (SELECT repo_id FROM files WHERE id = ?)
        "#,
    )
    .bind(file_id)
    .fetch_all(pool)
    .await?;
    Ok(to_affected(rows))
}

/// Files that re-export a name from a target file without aliasing.
///
/// Finds files where:
/// - An ImportPath ref targets `source_file_id`
/// - An ExportName ref matches `name`
/// - An ImportName ref also matches `name` (confirming non-aliased re-export)
///
/// Returns the file_ids of the relay files (barrels). Used for transitive
/// rename propagation through re-export chains.
pub async fn reexport_relay_file_ids(
    pool: &SqlitePool,
    source_file_id: i64,
    name: &str,
) -> anyhow::Result<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT f.id
        FROM refs r_path
        JOIN files f ON r_path.file_id = f.id
        WHERE r_path.target_file_id = ?
          AND r_path.ref_kind = 10
          AND EXISTS (
              SELECT 1 FROM refs r_exp
              JOIN strings s_exp ON r_exp.string_id = s_exp.id
              WHERE r_exp.file_id = f.id
                AND r_exp.ref_kind = 12
                AND s_exp.value = ?
          )
          AND EXISTS (
              SELECT 1 FROM refs r_imp
              JOIN strings s_imp ON r_imp.string_id = s_imp.id
              WHERE r_imp.file_id = f.id
                AND r_imp.ref_kind = 11
                AND s_imp.value = ?
          )
        "#,
    )
    .bind(source_file_id)
    .bind(name)
    .bind(name)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Find the file that exports `name` and is imported by `importer_file_id`.
///
/// Looks for: ImportPath in importer_file with a target_file_id where
/// that target file has ExportName matching `name`.
///
/// Returns the target file_id, or None if no match.
pub async fn upstream_export_file(
    pool: &SqlitePool,
    importer_file_id: i64,
    name: &str,
) -> anyhow::Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT r_path.target_file_id
        FROM refs r_path
        WHERE r_path.file_id = ?
          AND r_path.ref_kind = 10
          AND r_path.target_file_id IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM refs r_exp
              JOIN strings s ON r_exp.string_id = s.id
              WHERE r_exp.file_id = r_path.target_file_id
                AND r_exp.ref_kind = 12
                AND s.value = ?
          )
        LIMIT 1
        "#,
    )
    .bind(importer_file_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Find ExportName ref(s) for `name` in a specific file.
/// Returns AffectedRef with span info for editing.
pub async fn export_ref_in_file(
    pool: &SqlitePool,
    file_id: i64,
    name: &str,
) -> anyhow::Result<Vec<AffectedRef>> {
    let rows: Vec<(i64, i64, i64, String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.span_start, r.span_end, s.value, f.path, repos.root_path
        FROM refs r
        JOIN strings s ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        WHERE r.file_id = ?
          AND r.ref_kind = 12
          AND s.value = ?
        "#,
    )
    .bind(file_id)
    .bind(name)
    .fetch_all(pool)
    .await?;
    Ok(to_affected(rows))
}

/// Absolute path for a file_id (repo root + relative path).
pub async fn file_abs_path(
    pool: &SqlitePool,
    file_id: i64,
) -> anyhow::Result<Option<String>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT f.path, repos.root_path FROM files f JOIN repos ON f.repo_id = repos.id WHERE f.id = ?",
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(fp, rr)| format!("{}/{}", rr, fp)))
}
