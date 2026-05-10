use effect_runtime::v2::NextKey;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const GIT_REPO_DOMAIN: &str = "git/repo";
pub const GIT_BRANCH_DOMAIN: &str = "git/branch";
pub const GIT_PR_DOMAIN: &str = "git/pr";
pub const GIT_CHECKOUT_DOMAIN: &str = "git/checkout";
pub const GIT_NOTIFICATION_DOMAIN: &str = "git/notification";
pub const GIT_REPO_EVENT_DOMAIN: &str = "git/repo-event";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhcacheChangeReq {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub event: String,
    pub repo_slug: Option<String>,
    pub payload: serde_json::Value,
    pub occurred_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotifyGhcacheChangeReq {
    pub change: GhcacheChangeReq,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyNotice {
    pub domain: String,
    pub key_hex: Option<String>,
}

pub fn ghcache_dirty_notices(change: &GhcacheChangeReq) -> Vec<(String, Option<NextKey>)> {
    let mut out = Vec::new();
    let Some(repo) = change.repo_slug.as_deref() else {
        out.push(entity_notice(&change.entity_type, change.entity_id));
        return out;
    };

    out.push((GIT_REPO_DOMAIN.to_string(), Some(git_repo_dirty_key(repo))));
    match change.entity_type.as_str() {
        "branch" => {
            if let Some(branch) =
                json_str(&change.payload, "name").or_else(|| json_str(&change.payload, "branch"))
            {
                out.push((
                    GIT_BRANCH_DOMAIN.to_string(),
                    Some(git_branch_dirty_key(repo, branch)),
                ));
            }
        }
        "pull_request" => {
            out.push((GIT_PR_DOMAIN.to_string(), None));
            if let Some(number) = json_i64(&change.payload, "number") {
                out.push((
                    GIT_PR_DOMAIN.to_string(),
                    Some(git_pr_dirty_key(repo, number)),
                ));
            }
        }
        "checkout" => {
            let branch = json_str(&change.payload, "branch").unwrap_or("");
            let path = json_str(&change.payload, "local_path").unwrap_or("");
            out.push((
                GIT_CHECKOUT_DOMAIN.to_string(),
                Some(git_checkout_dirty_key(repo, branch, path)),
            ));
        }
        "notification" => {
            out.push((
                GIT_NOTIFICATION_DOMAIN.to_string(),
                Some(git_entity_dirty_key(
                    GIT_NOTIFICATION_DOMAIN,
                    repo,
                    change.entity_id,
                )),
            ));
        }
        "repo_event" => {
            out.push((
                GIT_REPO_EVENT_DOMAIN.to_string(),
                Some(git_entity_dirty_key(
                    GIT_REPO_EVENT_DOMAIN,
                    repo,
                    change.entity_id,
                )),
            ));
        }
        _ => out.push(entity_notice(&change.entity_type, change.entity_id)),
    }
    out
}

pub fn dirty_notice(domain: impl Into<String>, key: Option<NextKey>) -> DirtyNotice {
    DirtyNotice {
        domain: domain.into(),
        key_hex: key.map(hex_key),
    }
}

pub fn poll_ghcache_changes(
    db_path: &Path,
    last_id: i64,
) -> Result<Vec<GhcacheChangeReq>, rusqlite::Error> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, entity_type, entity_id, event, repo_slug, payload_json, occurred_at
         FROM change_log
         WHERE id > ?1
         ORDER BY id",
    )?;
    let rows = stmt.query_map([last_id], |row| {
        let payload_json: Option<String> = row.get(5)?;
        Ok(GhcacheChangeReq {
            id: row.get(0)?,
            entity_type: row.get(1)?,
            entity_id: row.get(2)?,
            event: row.get(3)?,
            repo_slug: row.get(4)?,
            payload: payload_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null),
            occurred_at: row.get(6)?,
        })
    })?;

    rows.collect()
}

pub fn latest_ghcache_change_id(db_path: &Path) -> Result<i64, rusqlite::Error> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.query_row("SELECT COALESCE(MAX(id), 0) FROM change_log", [], |row| {
        row.get(0)
    })
}

pub fn git_repo_dirty_key(repo: &str) -> NextKey {
    git_dirty_key(&[repo])
}

pub fn git_branch_dirty_key(repo: &str, branch: &str) -> NextKey {
    git_dirty_key(&[repo, branch])
}

pub fn git_pr_dirty_key(repo: &str, number: i64) -> NextKey {
    git_dirty_key(&[repo, "#", &number.to_string()])
}

pub fn git_checkout_dirty_key(repo: &str, branch: &str, path: &str) -> NextKey {
    git_dirty_key(&[repo, branch, path])
}

pub fn git_entity_dirty_key(domain: &str, repo: &str, entity_id: i64) -> NextKey {
    git_dirty_key(&[domain, repo, &entity_id.to_string()])
}

fn git_dirty_key(parts: &[&str]) -> NextKey {
    let mut h = blake3::Hasher::new();
    for part in parts {
        h.update(part.as_bytes());
        h.update(b"\0");
    }
    NextKey(*h.finalize().as_bytes())
}

fn entity_notice(entity_type: &str, entity_id: i64) -> (String, Option<NextKey>) {
    let domain = format!("ghcache/{entity_type}");
    let key = git_dirty_key(&[&domain, &entity_id.to_string()]);
    (domain, Some(key))
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.as_str())
}

fn json_i64(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64())
}

fn hex_key(key: NextKey) -> String {
    key.0.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn branch_change_maps_to_repo_and_branch_dirty() {
        let change = GhcacheChangeReq {
            id: 1,
            entity_type: "branch".into(),
            entity_id: 2,
            event: "updated".into(),
            repo_slug: Some("acme/api".into()),
            payload: json!({"name": "main"}),
            occurred_at: "2026-05-09T00:00:00Z".into(),
        };

        assert_eq!(
            ghcache_dirty_notices(&change),
            vec![
                (
                    GIT_REPO_DOMAIN.to_string(),
                    Some(git_repo_dirty_key("acme/api"))
                ),
                (
                    GIT_BRANCH_DOMAIN.to_string(),
                    Some(git_branch_dirty_key("acme/api", "main"))
                ),
            ],
        );
    }

    #[test]
    fn poll_reads_change_log_rows_after_last_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("gh.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE change_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id INTEGER NOT NULL,
                event TEXT NOT NULL,
                repo_slug TEXT,
                payload_json TEXT,
                occurred_at TEXT NOT NULL
            );
            INSERT INTO change_log
                (entity_type, entity_id, event, repo_slug, payload_json, occurred_at)
            VALUES
                ('branch', 1, 'inserted', 'acme/api', '{"name":"main"}', '2026-05-09T00:00:00Z'),
                ('pull_request', 2, 'updated', 'acme/api', '{"number":17}', '2026-05-09T00:01:00Z');
            "#,
        )
        .unwrap();
        drop(conn);

        let rows = poll_ghcache_changes(&db, 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].entity_type, "pull_request");
        assert_eq!(rows[0].payload["number"], 17);
    }
}
