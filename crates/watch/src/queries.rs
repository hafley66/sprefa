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

/// All RsUse refs whose string value starts with the given module path prefix.
///
/// Used when a Rust file moves: all `use crate::old_mod::...` refs need rewriting.
/// The prefix should be a module path like `crate::utils` -- this matches both
/// `crate::utils` exactly and `crate::utils::Foo`, `crate::utils::*`, etc.
pub async fn rs_uses_with_prefix(
    pool: &SqlitePool,
    prefix: &str,
) -> anyhow::Result<Vec<AffectedRef>> {
    let exact = prefix.to_string();
    let starts = format!("{}::", prefix);
    let rows: Vec<(i64, i64, i64, String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.span_start, r.span_end, s.value, f.path, repos.root_path
        FROM refs r
        JOIN strings s ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        WHERE r.ref_kind = 30
          AND (s.value = ? OR s.value LIKE ? || '%')
        "#,
    )
    .bind(&exact)
    .bind(&starts)
    .fetch_all(pool)
    .await?;
    Ok(to_affected(rows))
}

/// All RsUse refs whose last path segment matches the given name,
/// in files that are within the given repo.
///
/// Used for Rust declaration renames: when `Foo` is renamed to `Bar` in
/// `crate::utils`, find all `use crate::utils::Foo` refs.
pub async fn rs_uses_ending_with(
    pool: &SqlitePool,
    module_path: &str,
    name: &str,
) -> anyhow::Result<Vec<AffectedRef>> {
    let target_value = format!("{}::{}", module_path, name);
    let rows: Vec<(i64, i64, i64, String, String, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.span_start, r.span_end, s.value, f.path, repos.root_path
        FROM refs r
        JOIN strings s ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        WHERE r.ref_kind = 30
          AND s.value = ?
        "#,
    )
    .bind(&target_value)
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
