#![cfg(test)]

use rusqlite::{params, Connection, OptionalExtension};

use super::{FixtureRow, StageError, StageReady};

pub(super) const PLUS_SQL: &str = "
    INSERT OR IGNORE INTO _plus(scope, name, value)
    SELECT n.scope, n.name, n.value
    FROM _changed_key AS k
    CROSS JOIN _next AS n
    LEFT JOIN rel_fixture AS p
      ON p.scope = n.scope AND p.name = n.name AND p.value = n.value
    WHERE n.scope = k.scope AND n.name = k.name
      AND p.scope IS NULL";

pub(super) const MINUS_SQL: &str = "
    INSERT OR IGNORE INTO _minus(scope, name, value)
    SELECT p.scope, p.name, p.value
    FROM _changed_key AS k
    CROSS JOIN rel_fixture AS p
    LEFT JOIN _next AS n
      ON n.scope = p.scope AND n.name = p.name AND n.value = p.value
    WHERE p.scope = k.scope AND p.name = k.name
      AND n.scope IS NULL";

pub(super) fn initialize(conn: &Connection) -> Result<(), StageError> {
    conn.execute_batch(
        "PRAGMA temp_store=FILE;
        PRAGMA automatic_index=OFF;
        CREATE TABLE IF NOT EXISTS rel_fixture(
            scope TEXT NOT NULL,
            name TEXT NOT NULL,
            value INTEGER NOT NULL,
            PRIMARY KEY(scope, name, value)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS _delta_manifest(
            generation INTEGER PRIMARY KEY,
            key_count INTEGER NOT NULL,
            row_count INTEGER NOT NULL,
            plus_count INTEGER NOT NULL,
            minus_count INTEGER NOT NULL,
            digest BLOB NOT NULL CHECK(length(digest) = 32),
            exact INTEGER NOT NULL CHECK(exact = 1)
        );
        CREATE TABLE IF NOT EXISTS _delta_watermark(
            singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
            generation INTEGER NOT NULL,
            digest BLOB NOT NULL CHECK(length(digest) = 32)
        );
        CREATE TEMP TABLE _next(
            scope TEXT NOT NULL,
            name TEXT NOT NULL,
            value INTEGER NOT NULL,
            PRIMARY KEY(scope, name, value)
        ) WITHOUT ROWID;
        CREATE TEMP TABLE _changed_key(
            scope TEXT NOT NULL,
            name TEXT NOT NULL,
            PRIMARY KEY(scope, name)
        ) WITHOUT ROWID;
        CREATE TEMP TABLE _plus(
            scope TEXT NOT NULL,
            name TEXT NOT NULL,
            value INTEGER NOT NULL,
            PRIMARY KEY(scope, name, value)
        ) WITHOUT ROWID;
        CREATE TEMP TABLE _minus(
            scope TEXT NOT NULL,
            name TEXT NOT NULL,
            value INTEGER NOT NULL,
            PRIMARY KEY(scope, name, value)
        ) WITHOUT ROWID;
        CREATE TEMP TABLE _stage_ready(
            singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
            generation INTEGER NOT NULL,
            key_count INTEGER NOT NULL,
            row_count INTEGER NOT NULL,
            digest BLOB NOT NULL CHECK(length(digest) = 32)
        );",
    )?;
    Ok(())
}

pub(super) fn derive_stage_ready(
    conn: &Connection,
    generation: i64,
) -> Result<StageReady, StageError> {
    let outside_key = conn
        .query_row(
            "SELECT n.scope, n.name
         FROM _next AS n
         LEFT JOIN _changed_key AS k ON k.scope = n.scope AND k.name = n.name
         WHERE k.scope IS NULL LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((scope, name)) = outside_key {
        return Err(StageError::RowOutsideChangedKey { scope, name });
    }
    let mut digest = blake3::Hasher::new();
    digest.update(b"SPRF-STAGE\0\0\x01");
    digest.update(&generation.to_be_bytes());

    let mut key_count = 0usize;
    {
        let mut stmt = conn.prepare("SELECT scope, name FROM _changed_key ORDER BY scope, name")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            digest.update(&[1]);
            hash_str(&mut digest, &row.get::<_, String>(0)?)?;
            hash_str(&mut digest, &row.get::<_, String>(1)?)?;
            key_count += 1;
        }
    }
    let mut row_count = 0usize;
    {
        let mut stmt =
            conn.prepare("SELECT scope, name, value FROM _next ORDER BY scope, name, value")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            digest.update(&[2]);
            hash_str(&mut digest, &row.get::<_, String>(0)?)?;
            hash_str(&mut digest, &row.get::<_, String>(1)?)?;
            digest.update(&row.get::<_, i64>(2)?.to_be_bytes());
            row_count += 1;
        }
    }
    Ok(StageReady {
        generation,
        key_count,
        row_count,
        digest: *digest.finalize().as_bytes(),
    })
}

fn hash_str(digest: &mut blake3::Hasher, value: &str) -> Result<(), StageError> {
    let len = u32::try_from(value.len()).map_err(|_| StageError::IdentityTooLarge)?;
    digest.update(&len.to_be_bytes());
    digest.update(value.as_bytes());
    Ok(())
}

pub(super) fn verify_stage_ready(
    conn: &Connection,
    expected: &StageReady,
) -> Result<(), StageError> {
    let stored = conn
        .query_row(
            "SELECT generation, key_count, row_count, digest FROM _stage_ready WHERE singleton = 1",
            [],
            |row| {
                Ok(StageReady {
                    generation: row.get(0)?,
                    key_count: row.get(1)?,
                    row_count: row.get(2)?,
                    digest: blob32(row.get(3)?)?,
                })
            },
        )
        .optional()?
        .ok_or(StageError::StageNotReady)?;
    let actual = derive_stage_ready(conn, expected.generation)?;
    if stored != *expected || actual != *expected {
        return Err(StageError::StageSealMismatch);
    }
    Ok(())
}

pub(super) fn blob32(bytes: Vec<u8>) -> rusqlite::Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(32, 32))
}

pub(super) fn apply_minus(conn: &Connection) -> Result<usize, StageError> {
    let mut read = conn.prepare("SELECT scope, name, value FROM _minus")?;
    let mut rows = read.query([])?;
    let mut delete = conn
        .prepare_cached("DELETE FROM rel_fixture WHERE scope = ?1 AND name = ?2 AND value = ?3")?;
    let mut affected = 0;
    while let Some(row) = rows.next()? {
        affected += delete.execute(params![
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?
        ])?;
    }
    Ok(affected)
}

pub(super) fn apply_plus(conn: &Connection) -> Result<usize, StageError> {
    let mut read = conn.prepare("SELECT scope, name, value FROM _plus")?;
    let mut rows = read.query([])?;
    let mut insert =
        conn.prepare_cached("INSERT INTO rel_fixture(scope, name, value) VALUES (?1, ?2, ?3)")?;
    let mut affected = 0;
    while let Some(row) = rows.next()? {
        affected += insert.execute(params![
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?
        ])?;
    }
    Ok(affected)
}

pub(super) fn read_fixture_rows(conn: &Connection) -> Result<Vec<FixtureRow>, StageError> {
    let mut stmt =
        conn.prepare("SELECT scope, name, value FROM rel_fixture ORDER BY scope, name, value")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FixtureRow {
                scope: row.get(0)?,
                name: row.get(1)?,
                value: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(super) fn assert_diff_plans(conn: &Connection) -> Result<(), StageError> {
    assert_plan(
        conn,
        "plus",
        PLUS_SQL,
        &["SEARCH N USING PRIMARY KEY", "SEARCH P USING PRIMARY KEY"],
    )?;
    assert_plan(
        conn,
        "minus",
        MINUS_SQL,
        &["SEARCH P USING PRIMARY KEY", "SEARCH N USING PRIMARY KEY"],
    )
}

fn assert_plan(
    conn: &Connection,
    label: &'static str,
    sql: &str,
    required: &[&str],
) -> Result<(), StageError> {
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain)?;
    let details = stmt
        .query_map([], |row| row.get::<_, String>(3))?
        .collect::<Result<Vec<_>, _>>()?;
    let upper: Vec<String> = details
        .iter()
        .map(|line| line.to_ascii_uppercase())
        .collect();
    for forbidden in [
        "SCAN P",
        "SCAN REL_FIXTURE",
        "SUBQUERY",
        "EXCEPT",
        "TEMP B-TREE",
    ] {
        if upper.iter().any(|line| line.contains(forbidden)) {
            return Err(StageError::Plan {
                label,
                detail: format!("forbidden `{forbidden}` in {details:?}"),
            });
        }
    }
    for needle in required {
        if !upper.iter().any(|line| line.contains(needle)) {
            return Err(StageError::Plan {
                label,
                detail: format!("required `{needle}` absent from {details:?}"),
            });
        }
    }
    let driver = upper.iter().position(|line| line.contains("SCAN K"));
    let first_probe = upper.iter().position(|line| line.contains("SEARCH "));
    if driver.is_none() || first_probe.is_none() || driver >= first_probe {
        return Err(StageError::Plan {
            label,
            detail: format!("changed-key driver ordering absent from {details:?}"),
        });
    }
    Ok(())
}
