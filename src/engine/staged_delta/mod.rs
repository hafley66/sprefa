//! Test-only SQLite harness for a sealed, bounded staged-set diff.
//!
//! Staging is autocommit work and cannot mutate the persistent fixture. Only a
//! complete `StageReady` seal may enter the short consume transaction, where
//! exact public plus/minus sets, application, manifest, and watermark commit
//! atomically.

#![cfg(test)]

use std::fmt;
use std::path::Path;

use crate::ast::Value;
use crate::db::Db;

mod sql;
#[cfg(test)]
mod tests;

use sql::{
    apply_minus, apply_plus, assert_diff_plans, blob32, derive_stage_ready, read_fixture_rows,
    verify_stage_ready, MINUS_SQL, PLUS_SQL,
};

const MAX_BUFFERED_ROWS: usize = 4096;
const MAX_BUFFERED_BYTES: usize = 256 * 1024;

/// Full-row identity is a non-null tagged tuple: two UTF-8 strings and one i64.
/// SQLite mirrors this with NOT NULL columns and a full-row primary key.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FixtureRow {
    scope: String,
    name: String,
    value: i64,
}

impl FixtureRow {
    fn new(scope: &str, name: &str, value: i64) -> Self {
        Self {
            scope: scope.into(),
            name: name.into(),
            value,
        }
    }

    fn encoded_bytes(&self) -> usize {
        16 + self.scope.len() + self.name.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ChangedKey {
    scope: String,
    name: String,
}

impl ChangedKey {
    fn new(scope: &str, name: &str) -> Self {
        Self {
            scope: scope.into(),
            name: name.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SinkLimits {
    max_rows: usize,
    max_bytes: usize,
}

impl Default for SinkLimits {
    fn default() -> Self {
        Self {
            max_rows: MAX_BUFFERED_ROWS,
            max_bytes: MAX_BUFFERED_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SinkStats {
    flushes: usize,
    rows_flushed: usize,
    peak_rows: usize,
    peak_bytes: usize,
}

struct SqliteStageSink<'a> {
    db: &'a Db,
    limits: SinkLimits,
    rows: Vec<FixtureRow>,
    buffered_bytes: usize,
    stats: SinkStats,
}

impl<'a> SqliteStageSink<'a> {
    fn new(db: &'a Db, limits: SinkLimits) -> Result<Self, StageError> {
        if limits.max_rows == 0
            || limits.max_bytes == 0
            || limits.max_rows > MAX_BUFFERED_ROWS
            || limits.max_bytes > MAX_BUFFERED_BYTES
        {
            return Err(StageError::InvalidLimits);
        }
        Ok(Self {
            db,
            limits,
            rows: Vec::with_capacity(limits.max_rows),
            buffered_bytes: 0,
            stats: SinkStats::default(),
        })
    }

    fn push(&mut self, row: FixtureRow) -> Result<(), StageError> {
        let bytes = row.encoded_bytes();
        if bytes > self.limits.max_bytes {
            return Err(StageError::RowTooLarge {
                bytes,
                limit: self.limits.max_bytes,
            });
        }
        if !self.rows.is_empty()
            && (self.rows.len() == self.limits.max_rows
                || self.buffered_bytes + bytes > self.limits.max_bytes)
        {
            self.flush()?;
        }
        self.buffered_bytes += bytes;
        self.rows.push(row);
        self.stats.peak_rows = self.stats.peak_rows.max(self.rows.len());
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.buffered_bytes);
        if self.rows.len() == self.limits.max_rows || self.buffered_bytes == self.limits.max_bytes {
            self.flush()?;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<SinkStats, StageError> {
        self.flush()?;
        Ok(self.stats)
    }

    fn flush(&mut self) -> Result<(), StageError> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let attempted = self.rows.len();
        let values: Vec<Vec<Value>> = self
            .rows
            .drain(..)
            .map(|row| vec![Value::Text(row.scope), Value::Text(row.name), Value::Int(row.value)])
            .collect();
        self.db.insert_rows("_next", &["scope", "name", "value"], &values)?;
        self.stats.rows_flushed += attempted;
        self.buffered_bytes = 0;
        self.stats.flushes += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StageReady {
    generation: i64,
    key_count: usize,
    row_count: usize,
    digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailPoint {
    None,
    AfterPlus,
    AfterMinus,
    AfterDelete,
    AfterInsert,
    AfterManifestWatermark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ApplyReport {
    plus: usize,
    minus: usize,
    sink: SinkStats,
}

struct StagedDeltaHarness {
    db: Db,
}

impl StagedDeltaHarness {
    fn new() -> Result<Self, StageError> {
        Self::from_db(crate::db::open(None)?)
    }

    fn open_file(path: &Path) -> Result<Self, StageError> {
        let path_str = path.to_str().expect("stage harness path must be valid UTF-8");
        Self::from_db(crate::db::open(Some(path_str))?)
    }

    fn from_db(db: Db) -> Result<Self, StageError> {
        sql::initialize(&db)?;
        Ok(Self { db })
    }

    fn seed(&self, rows: &[FixtureRow]) -> Result<(), StageError> {
        if rows.is_empty() {
            return Ok(());
        }
        let values: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| vec![
                Value::Text(row.scope.clone()),
                Value::Text(row.name.clone()),
                Value::Int(row.value),
            ])
            .collect();
        self.db.insert_rows("rel_fixture", &["scope", "name", "value"], &values)?;
        Ok(())
    }

    /// Autocommit staging. Until `seal_stage` succeeds there is deliberately no
    /// consume token and no persistent relation is writable.
    fn stage_unsealed(
        &self,
        changed: &[ChangedKey],
        next: impl IntoIterator<Item = FixtureRow>,
        limits: SinkLimits,
    ) -> Result<SinkStats, StageError> {
        self.db.execute_batch_on(
            "_stage_ready",
            "DELETE FROM _stage_ready;
                                 DELETE FROM _next;
                                 DELETE FROM _changed_key;
                                 DELETE FROM _plus;
                                 DELETE FROM _minus;",
        )?;
        if !changed.is_empty() {
            let values: Vec<Vec<Value>> = changed
                .iter()
                .map(|key| vec![Value::Text(key.scope.clone()), Value::Text(key.name.clone())])
                .collect();
            self.db.insert_rows("_changed_key", &["scope", "name"], &values)?;
        }
        let mut sink = SqliteStageSink::new(&self.db, limits)?;
        for row in next {
            sink.push(row)?;
        }
        sink.finish()
    }

    fn seal_stage(&self, generation: i64) -> Result<StageReady, StageError> {
        let ready = derive_stage_ready(&self.db, generation)?;
        self.db.exec_params(
            "_stage_ready",
            "INSERT INTO _stage_ready(singleton, generation, key_count, row_count, digest)
             VALUES (1, ?1, ?2, ?3, ?4)",
            &[
                ready.generation.into(),
                ready.key_count.into(),
                ready.row_count.into(),
                ready.digest.to_vec().into(),
            ],
        )?;
        Ok(ready)
    }

    fn stage_generation(
        &self,
        generation: i64,
        changed: &[ChangedKey],
        next: impl IntoIterator<Item = FixtureRow>,
        limits: SinkLimits,
    ) -> Result<(StageReady, SinkStats), StageError> {
        let stats = self.stage_unsealed(changed, next, limits)?;
        Ok((self.seal_stage(generation)?, stats))
    }

    fn abort_stage(&self) -> Result<(), StageError> {
        self.db.exec_on("_stage_ready", "DELETE FROM _stage_ready")?;
        Ok(())
    }

    /// Statement sequence run inside the `BEGIN IMMEDIATE` consume transaction.
    /// Split out of `consume_stage` so the caller controls commit/rollback: any
    /// `Err` returned here (a seam failure or an injected `FailPoint`) must roll
    /// the whole transaction back, mirroring the Drop-rollback a raw SQL
    /// transaction guard gave for free before this seam migration.
    fn consume_stage_in_tx(
        &self,
        ready: &StageReady,
        fail: FailPoint,
    ) -> Result<(usize, usize), StageError> {
        self.db
            .execute_batch_on("_plus", "DELETE FROM _plus; DELETE FROM _minus;")?;

        let plus = self.db.exec_on("_plus", PLUS_SQL)?;
        if fail == FailPoint::AfterPlus {
            return Err(StageError::Injected(fail));
        }
        let minus = self.db.exec_on("_minus", MINUS_SQL)?;
        if fail == FailPoint::AfterMinus {
            return Err(StageError::Injected(fail));
        }
        let deleted = apply_minus(&self.db)?;
        if deleted != minus {
            return Err(StageError::ApplyCount {
                expected: minus,
                actual: deleted,
            });
        }
        if fail == FailPoint::AfterDelete {
            return Err(StageError::Injected(fail));
        }
        let inserted = apply_plus(&self.db)?;
        if inserted != plus {
            return Err(StageError::ApplyCount {
                expected: plus,
                actual: inserted,
            });
        }
        if fail == FailPoint::AfterInsert {
            return Err(StageError::Injected(fail));
        }

        self.db.exec_params(
            "_delta_manifest",
            "INSERT INTO _delta_manifest(
                 generation, key_count, row_count, plus_count, minus_count, digest, exact)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            &[
                ready.generation.into(),
                ready.key_count.into(),
                ready.row_count.into(),
                plus.into(),
                minus.into(),
                ready.digest.to_vec().into(),
            ],
        )?;
        self.db.exec_params(
            "_delta_watermark",
            "INSERT INTO _delta_watermark(singleton, generation, digest) VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET
                 generation = excluded.generation, digest = excluded.digest",
            &[ready.generation.into(), ready.digest.to_vec().into()],
        )?;
        self.db.exec_on("_stage_ready", "DELETE FROM _stage_ready")?;
        if fail == FailPoint::AfterManifestWatermark {
            return Err(StageError::Injected(fail));
        }
        Ok((plus, minus))
    }

    fn consume_stage(
        &mut self,
        ready: &StageReady,
        sink: SinkStats,
        fail: FailPoint,
    ) -> Result<ApplyReport, StageError> {
        verify_stage_ready(&self.db, ready)?;
        assert_diff_plans(&self.db)?;
        self.db.begin_immediate()?;
        match self.consume_stage_in_tx(ready, fail) {
            Ok((plus, minus)) => {
                self.db.commit()?;
                Ok(ApplyReport { plus, minus, sink })
            }
            Err(error) => {
                // @rusqlite-ok: rollback failure is unactionable; the original error wins.
                let _ = self.db.rollback();
                Err(error)
            }
        }
    }

    fn apply_generation(
        &mut self,
        generation: i64,
        changed: &[ChangedKey],
        next: impl IntoIterator<Item = FixtureRow>,
        limits: SinkLimits,
        fail: FailPoint,
    ) -> Result<ApplyReport, StageError> {
        let (ready, sink) = self.stage_generation(generation, changed, next, limits)?;
        self.consume_stage(&ready, sink, fail)
    }

    fn rows(&self) -> Result<Vec<FixtureRow>, StageError> {
        Ok(read_fixture_rows(&self.db)?)
    }

    fn manifest(&self, generation: i64) -> Result<Option<Manifest>, StageError> {
        Ok(self.db.query_opt(
            "_delta_manifest",
            "SELECT key_count, row_count, plus_count, minus_count, digest, exact
             FROM _delta_manifest WHERE generation = ?1",
            &[generation.into()],
            |row| {
                Ok(Manifest {
                    key_count: row.get(0)?,
                    row_count: row.get(1)?,
                    plus: row.get(2)?,
                    minus: row.get(3)?,
                    digest: blob32(row.get(4)?)?,
                    exact: row.get::<_, i64>(5)? == 1,
                })
            },
        )?)
    }

    fn watermark(&self) -> Result<Option<(i64, [u8; 32])>, StageError> {
        Ok(self.db.query_opt(
            "_delta_watermark",
            "SELECT generation, digest FROM _delta_watermark WHERE singleton = 1",
            &[],
            |row| Ok((row.get(0)?, blob32(row.get(1)?)?)),
        )?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Manifest {
    key_count: usize,
    row_count: usize,
    plus: usize,
    minus: usize,
    digest: [u8; 32],
    exact: bool,
}

#[derive(Debug)]
enum StageError {
    /// Any Db-seam statement failure (`anyhow::Error` from `src/db.rs`, already
    /// carrying the failing rel + statement head — see `db::sql_err`).
    Sqlite(anyhow::Error),
    InvalidLimits,
    RowTooLarge { bytes: usize, limit: usize },
    IdentityTooLarge,
    StageNotReady,
    StageSealMismatch,
    RowOutsideChangedKey { scope: String, name: String },
    ApplyCount { expected: usize, actual: usize },
    Plan { label: &'static str, detail: String },
    Injected(FailPoint),
}

impl From<anyhow::Error> for StageError {
    fn from(error: anyhow::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "{error}"),
            Self::InvalidLimits => write!(f,
                "stage sink limits must be within 1..={MAX_BUFFERED_ROWS} rows and 1..={MAX_BUFFERED_BYTES} bytes"),
            Self::RowTooLarge { bytes, limit } => {
                write!(f, "single staged row is {bytes} bytes, over {limit}-byte limit")
            }
            Self::IdentityTooLarge => f.write_str("staged identity string exceeds u32 length"),
            Self::StageNotReady => f.write_str("stage has no committed StageReady seal"),
            Self::StageSealMismatch => f.write_str("StageReady does not match staged keys/rows"),
            Self::RowOutsideChangedKey { scope, name } => {
                write!(f, "staged row key ({scope:?}, {name:?}) is not sealed as changed")
            }
            Self::ApplyCount { expected, actual } => {
                write!(f, "staged apply affected {actual} rows; expected {expected}")
            }
            Self::Plan { label, detail } => write!(f, "unsafe {label} query plan: {detail}"),
            Self::Injected(point) => write!(f, "injected failure at {point:?}"),
        }
    }
}

impl std::error::Error for StageError {}
