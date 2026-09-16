#![cfg(test)]

use crate::db::Db;

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

pub(super) fn initialize(db: &Db) -> Result<(), StageError> {
    db.execute_batch_on(
        "_stage_schema",
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

pub(super) fn derive_stage_ready(db: &Db, generation: i64) -> Result<StageReady, StageError> {
    let outside_key = db.query_opt(
        "_next",
        "SELECT n.scope, n.name
         FROM _next AS n
         LEFT JOIN _changed_key AS k ON k.scope = n.scope AND k.name = n.name
         WHERE k.scope IS NULL LIMIT 1",
        &[],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    if let Some((scope, name)) = outside_key {
        return Err(StageError::RowOutsideChangedKey { scope, name });
    }
    let mut digest = blake3::Hasher::new();
    digest.update(b"SPRF-STAGE\0\0\x01");
    digest.update(&generation.to_be_bytes());

    let mut key_count = 0usize;
    db.for_each_row(
        "_changed_key",
        "SELECT scope, name FROM _changed_key ORDER BY scope, name",
        &[],
        |row| {
            digest.update(&[1]);
            hash_str(&mut digest, &row.get::<_, String>(0)?)?;
            hash_str(&mut digest, &row.get::<_, String>(1)?)?;
            key_count += 1;
            Ok(())
        },
    )?;

    let mut row_count = 0usize;
    db.for_each_row(
        "_next",
        "SELECT scope, name, value FROM _next ORDER BY scope, name, value",
        &[],
        |row| {
            digest.update(&[2]);
            hash_str(&mut digest, &row.get::<_, String>(0)?)?;
            hash_str(&mut digest, &row.get::<_, String>(1)?)?;
            digest.update(&row.get::<_, i64>(2)?.to_be_bytes());
            row_count += 1;
            Ok(())
        },
    )?;
    Ok(StageReady {
        generation,
        key_count,
        row_count,
        digest: *digest.finalize().as_bytes(),
    })
}

fn hash_str(digest: &mut blake3::Hasher, value: &str) -> anyhow::Result<()> {
    let len = u32::try_from(value.len())
        .map_err(|_| anyhow::anyhow!("staged identity string exceeds u32 length"))?;
    digest.update(&len.to_be_bytes());
    digest.update(value.as_bytes());
    Ok(())
}

pub(super) fn verify_stage_ready(db: &Db, expected: &StageReady) -> Result<(), StageError> {
    let stored = db
        .query_opt(
            "_stage_ready",
            "SELECT generation, key_count, row_count, digest FROM _stage_ready WHERE singleton = 1",
            &[],
            |row| {
                Ok(StageReady {
                    generation: row.get(0)?,
                    key_count: row.get(1)?,
                    row_count: row.get(2)?,
                    digest: blob32(row.get(3)?)?,
                })
            },
        )?
        .ok_or(StageError::StageNotReady)?;
    let actual = derive_stage_ready(db, expected.generation)?;
    if stored != *expected || actual != *expected {
        return Err(StageError::StageSealMismatch);
    }
    Ok(())
}

pub(super) fn blob32(bytes: Vec<u8>) -> anyhow::Result<[u8; 32]> {
    let len = bytes.len();
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("digest blob must be exactly 32 bytes, got {len}"))
}

/// Set-based delete: every `_minus` row (already scoped to this generation's
/// changed keys, computed earlier in the same consume transaction) leaves
/// `rel_fixture` in one statement — never a per-row delete loop.
pub(super) fn apply_minus(db: &Db) -> Result<usize, StageError> {
    Ok(db.exec_on(
        "rel_fixture",
        "DELETE FROM rel_fixture WHERE (scope, name, value) IN (SELECT scope, name, value FROM _minus)",
    )?)
}

/// Set-based insert, the `apply_minus` counterpart: every `_plus` row lands in
/// one statement.
pub(super) fn apply_plus(db: &Db) -> Result<usize, StageError> {
    Ok(db.exec_on(
        "rel_fixture",
        "INSERT INTO rel_fixture(scope, name, value) SELECT scope, name, value FROM _plus",
    )?)
}

pub(super) fn read_fixture_rows(db: &Db) -> Result<Vec<FixtureRow>, StageError> {
    Ok(db.query_rows(
        "rel_fixture",
        "SELECT scope, name, value FROM rel_fixture ORDER BY scope, name, value",
        &[],
        |row| {
            Ok(FixtureRow {
                scope: row.get(0)?,
                name: row.get(1)?,
                value: row.get(2)?,
            })
        },
    )?)
}

pub(super) fn assert_diff_plans(db: &Db) -> Result<(), StageError> {
    assert_plan(
        db,
        "plus",
        PLUS_SQL,
        &["SEARCH N USING PRIMARY KEY", "SEARCH P USING PRIMARY KEY"],
    )?;
    assert_plan(
        db,
        "minus",
        MINUS_SQL,
        &["SEARCH P USING PRIMARY KEY", "SEARCH N USING PRIMARY KEY"],
    )
}

fn assert_plan(
    db: &Db,
    label: &'static str,
    sql: &str,
    required: &[&str],
) -> Result<(), StageError> {
    let explain = format!("EXPLAIN QUERY PLAN {sql}");
    let details: Vec<String> =
        db.query_rows(
            "_explain",
            &explain,
            &[],
            |row| Ok(row.get::<_, String>(3)?),
        )?;
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
