//! Bounded, sealed source-row staging on the engine's SQLite connection.

use rusqlite::{params, Connection, OptionalExtension};

use crate::ast::Value;

const MAX_BUFFERED_ROWS: usize = 4096;
const MAX_BUFFERED_BYTES: usize = 256 * 1024;
const MAX_STAGE_ROWS: usize = 1_000_000;
const MAX_STAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_STAGE_OWNERS: usize = 100_000;
#[path = "source_codec.rs"]
mod codec;
#[path = "source_stage_read.rs"]
pub(super) mod read;

pub(super) use codec::{decode_values, encode_values};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StageId([u8; 16]);

impl StageId {
    pub(super) const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StageBase([u8; 32]);

impl StageBase {
    pub(super) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StageLimits {
    pub(super) max_rows: usize,
    pub(super) max_bytes: usize,
    pub(super) max_stage_rows: usize,
    pub(super) max_stage_bytes: usize,
    pub(super) max_owners: usize,
}

impl Default for StageLimits {
    fn default() -> Self {
        Self {
            max_rows: MAX_BUFFERED_ROWS,
            max_bytes: MAX_BUFFERED_BYTES,
            max_stage_rows: MAX_STAGE_ROWS,
            max_stage_bytes: MAX_STAGE_BYTES,
            max_owners: MAX_STAGE_OWNERS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct StageStats {
    pub(super) flushes: usize,
    pub(super) rows: usize,
    pub(super) peak_rows: usize,
    pub(super) peak_bytes: usize,
    pub(super) staged_bytes: usize,
    pub(super) owners: usize,
}

/// A completed source-stage seal, not the pipeline's transaction-ready token.
/// The richer semantic BaseStamp and main-database revalidation are owned by
/// the later prepare/apply boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SealedSourceStage {
    pub(super) stage_id: StageId,
    pub(super) generation: i64,
    pub(super) base: StageBase,
    pub(super) key_count: usize,
    pub(super) row_count: usize,
    pub(super) encoded_bytes: usize,
    pub(super) digest: [u8; 32],
}

#[derive(Clone, Debug)]
pub(super) struct SourceStageRow {
    pub(super) relation: String,
    pub(super) repo: String,
    pub(super) path: String,
    pub(super) ordinal: u64,
    pub(super) values: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(super) struct SourceStageOwner {
    pub(super) relation: String,
    pub(super) repo: String,
    pub(super) path: String,
}

pub(super) struct SourceStage<'a> {
    conn: &'a Connection,
}

impl<'a> SourceStage<'a> {
    pub(super) fn open(conn: &'a Connection) -> Result<Self, SourceStageError> {
        let temp_store: i64 = conn.query_row("PRAGMA temp_store", [], |row| row.get(0))?;
        if temp_store != 1 {
            return Err(SourceStageError::TempStoreNotFile);
        }
        conn.execute_batch(
            "PRAGMA temp.cache_size=-256;
            PRAGMA cache_spill=ON;
            CREATE TEMP TABLE IF NOT EXISTS _source_stage_row(
                stage_id BLOB NOT NULL CHECK(length(stage_id) = 16),
                relation TEXT NOT NULL,
                repo TEXT NOT NULL,
                path TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                encoded BLOB NOT NULL,
                PRIMARY KEY(stage_id, relation, repo, path, ordinal)
            ) WITHOUT ROWID;
            CREATE TEMP TABLE IF NOT EXISTS _source_stage_owner(
                stage_id BLOB NOT NULL CHECK(length(stage_id) = 16),
                relation TEXT NOT NULL,
                repo TEXT NOT NULL,
                path TEXT NOT NULL,
                completed INTEGER NOT NULL CHECK(completed = 1),
                PRIMARY KEY(stage_id, relation, repo, path)
            ) WITHOUT ROWID;
            CREATE TEMP TABLE IF NOT EXISTS _source_stage_ready(
                stage_id BLOB PRIMARY KEY CHECK(length(stage_id) = 16),
                generation INTEGER NOT NULL,
                base_stamp BLOB NOT NULL CHECK(length(base_stamp) = 32),
                key_count INTEGER NOT NULL,
                row_count INTEGER NOT NULL,
                encoded_bytes INTEGER NOT NULL,
                digest BLOB NOT NULL CHECK(length(digest) = 32)
            ) WITHOUT ROWID;",
        )?;
        Ok(Self { conn })
    }

    /// Borrow an already-open stage without issuing TEMP DDL or PRAGMAs.
    /// Apply uses this inside the main WAL transaction, where TEMP is read-only.
    pub(super) const fn existing(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub(super) fn begin(
        &self,
        stage_id: StageId,
        limits: StageLimits,
    ) -> Result<StageWriter<'a>, SourceStageError> {
        validate_limits(limits)?;
        self.conn.execute(
            "DELETE FROM _source_stage_ready WHERE stage_id = ?1",
            [stage_id.0.as_slice()],
        )?;
        self.conn.execute(
            "DELETE FROM _source_stage_row WHERE stage_id = ?1",
            [stage_id.0.as_slice()],
        )?;
        self.conn.execute(
            "DELETE FROM _source_stage_owner WHERE stage_id = ?1",
            [stage_id.0.as_slice()],
        )?;
        Ok(StageWriter {
            conn: self.conn,
            stage_id,
            limits,
            buffered_bytes: 0,
            buffered: Vec::with_capacity(limits.max_rows),
            stats: StageStats::default(),
        })
    }

    pub(super) fn seal(
        &self,
        stage_id: StageId,
        generation: i64,
        base: StageBase,
    ) -> Result<SealedSourceStage, SourceStageError> {
        let ready = derive_ready(self.conn, stage_id, generation, base)?;
        self.conn.execute(
            "INSERT INTO _source_stage_ready(
                 stage_id,generation,base_stamp,key_count,row_count,encoded_bytes,digest)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                stage_id.0.as_slice(),
                generation,
                base.0.as_slice(),
                ready.key_count,
                ready.row_count,
                ready.encoded_bytes,
                ready.digest.as_slice()
            ],
        )?;
        Ok(ready)
    }

    /// Revalidates the seal before exposing any row to later reconcile code.
    pub(super) fn visit_ready_rows(
        &self,
        ready: &SealedSourceStage,
        current_base: StageBase,
        mut visit: impl FnMut(SourceStageRow) -> Result<(), SourceStageError>,
    ) -> Result<usize, SourceStageError> {
        if current_base != ready.base {
            return Err(SourceStageError::StaleBase);
        }
        verify_ready(self.conn, ready)?;
        let mut stmt = self.conn.prepare(
            "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
             WHERE stage_id=?1 ORDER BY relation,repo,path,ordinal",
        )?;
        let mut rows = stmt.query([ready.stage_id.0.as_slice()])?;
        let mut visited = 0;
        while let Some(row) = rows.next()? {
            visit(SourceStageRow {
                relation: row.get(0)?,
                repo: row.get(1)?,
                path: row.get(2)?,
                ordinal: row.get::<_, i64>(3)? as u64,
                values: decode_values(&row.get::<_, Vec<u8>>(4)?)?,
            })?;
            visited += 1;
        }
        Ok(visited)
    }

    pub(super) fn visit_ready_owners(
        &self,
        ready: &SealedSourceStage,
        current_base: StageBase,
        mut visit: impl FnMut(SourceStageOwner) -> Result<(), SourceStageError>,
    ) -> Result<usize, SourceStageError> {
        if current_base != ready.base {
            return Err(SourceStageError::StaleBase);
        }
        verify_ready(self.conn, ready)?;
        let mut stmt = self.conn.prepare(
            "SELECT relation,repo,path FROM _source_stage_owner
             WHERE stage_id=?1 ORDER BY relation,repo,path",
        )?;
        let mut rows = stmt.query([ready.stage_id.0.as_slice()])?;
        let mut visited = 0;
        while let Some(row) = rows.next()? {
            visit(SourceStageOwner {
                relation: row.get(0)?,
                repo: row.get(1)?,
                path: row.get(2)?,
            })?;
            visited += 1;
        }
        Ok(visited)
    }

    /// Cleanup is deliberately separate from consume. Call it only after the
    /// main WAL semantic transaction commits or aborts; consume itself never
    /// mutates TEMP seal/row state inside that transaction.
    pub(super) fn discard(&self, stage_id: StageId) -> Result<(), SourceStageError> {
        self.conn.execute(
            "DELETE FROM _source_stage_ready WHERE stage_id=?1",
            [stage_id.0.as_slice()],
        )?;
        self.conn.execute(
            "DELETE FROM _source_stage_row WHERE stage_id=?1",
            [stage_id.0.as_slice()],
        )?;
        self.conn.execute(
            "DELETE FROM _source_stage_owner WHERE stage_id=?1",
            [stage_id.0.as_slice()],
        )?;
        Ok(())
    }
}

struct BufferedRow {
    relation: String,
    repo: String,
    path: String,
    ordinal: u64,
    encoded: Vec<u8>,
    bytes: usize,
}

pub(super) struct StageWriter<'a> {
    conn: &'a Connection,
    stage_id: StageId,
    limits: StageLimits,
    buffered_bytes: usize,
    buffered: Vec<BufferedRow>,
    stats: StageStats,
}

impl StageWriter<'_> {
    pub(super) fn push(
        &mut self,
        relation: &str,
        repo: &str,
        path: &str,
        ordinal: u64,
        values: &[Value],
    ) -> Result<(), SourceStageError> {
        let encoded = encode_values(values)?;
        let bytes = 24usize
            .checked_add(relation.len())
            .and_then(|n| n.checked_add(repo.len()))
            .and_then(|n| n.checked_add(path.len()))
            .and_then(|n| n.checked_add(encoded.len()))
            .ok_or(SourceStageError::EncodingTooLarge)?;
        if bytes > self.limits.max_bytes {
            return Err(SourceStageError::RowTooLarge {
                bytes,
                limit: self.limits.max_bytes,
            });
        }
        if ordinal > i64::MAX as u64 {
            return Err(SourceStageError::EncodingTooLarge);
        }
        if self.stats.rows >= self.limits.max_stage_rows
            || self
                .stats
                .staged_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > self.limits.max_stage_bytes)
        {
            return Err(SourceStageError::StageLimitExceeded);
        }
        if !self.buffered.is_empty()
            && (self.buffered.len() == self.limits.max_rows
                || self.buffered_bytes + bytes > self.limits.max_bytes)
        {
            self.flush()?;
        }
        self.buffered.push(BufferedRow {
            relation: relation.into(),
            repo: repo.into(),
            path: path.into(),
            ordinal,
            encoded,
            bytes,
        });
        self.buffered_bytes += bytes;
        self.stats.rows += 1;
        self.stats.staged_bytes += bytes;
        self.stats.peak_rows = self.stats.peak_rows.max(self.buffered.len());
        self.stats.peak_bytes = self.stats.peak_bytes.max(self.buffered_bytes);
        if self.buffered.len() == self.limits.max_rows
            || self.buffered_bytes == self.limits.max_bytes
        {
            self.flush()?;
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<StageStats, SourceStageError> {
        self.flush()?;
        Ok(self.stats)
    }

    /// Explicit producer completion for one relation/repo/path owner. A seal
    /// refuses rows from owners that never reached this boundary; an owner may
    /// complete with zero rows to represent an exact empty replacement.
    pub(super) fn complete_owner(
        &mut self,
        relation: &str,
        repo: &str,
        path: &str,
    ) -> Result<(), SourceStageError> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO _source_stage_owner(
                 stage_id,relation,repo,path,completed) VALUES (?1,?2,?3,?4,1)",
            params![self.stage_id.0.as_slice(), relation, repo, path],
        )?;
        if inserted != 0 {
            if self.stats.owners == self.limits.max_owners {
                self.conn.execute(
                    "DELETE FROM _source_stage_owner
                     WHERE stage_id=?1 AND relation=?2 AND repo=?3 AND path=?4",
                    params![self.stage_id.0.as_slice(), relation, repo, path],
                )?;
                return Err(SourceStageError::StageLimitExceeded);
            }
            self.stats.owners += 1;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SourceStageError> {
        if self.buffered.is_empty() {
            return Ok(());
        }
        let mut insert = self.conn.prepare_cached(
            "INSERT INTO _source_stage_row(stage_id,relation,repo,path,ordinal,encoded)
             VALUES (?1,?2,?3,?4,?5,?6)",
        )?;
        for row in self.buffered.drain(..) {
            insert.execute(params![
                self.stage_id.0.as_slice(),
                row.relation,
                row.repo,
                row.path,
                row.ordinal,
                row.encoded
            ])?;
            self.buffered_bytes -= row.bytes;
        }
        self.stats.flushes += 1;
        Ok(())
    }
}

fn validate_limits(limits: StageLimits) -> Result<(), SourceStageError> {
    if limits.max_rows == 0
        || limits.max_bytes == 0
        || limits.max_rows > MAX_BUFFERED_ROWS
        || limits.max_bytes > MAX_BUFFERED_BYTES
        || limits.max_stage_rows == 0
        || limits.max_stage_rows > MAX_STAGE_ROWS
        || limits.max_stage_bytes == 0
        || limits.max_stage_bytes > MAX_STAGE_BYTES
        || limits.max_owners == 0
        || limits.max_owners > MAX_STAGE_OWNERS
    {
        return Err(SourceStageError::InvalidLimits);
    }
    Ok(())
}

fn derive_ready(
    conn: &Connection,
    stage_id: StageId,
    generation: i64,
    base: StageBase,
) -> Result<SealedSourceStage, SourceStageError> {
    let incomplete = conn
        .query_row(
            "SELECT r.relation,r.repo,r.path FROM _source_stage_row AS r
         LEFT JOIN _source_stage_owner AS o
           ON o.stage_id=r.stage_id AND o.relation=r.relation
          AND o.repo=r.repo AND o.path=r.path
         WHERE r.stage_id=?1 AND o.stage_id IS NULL LIMIT 1",
            [stage_id.0.as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((relation, repo, path)) = incomplete {
        return Err(SourceStageError::OwnerIncomplete {
            relation,
            repo,
            path,
        });
    }
    let mut hash = blake3::Hasher::new();
    hash.update(b"SPRF-SOURCE-STAGE\0\0\x01");
    hash.update(&stage_id.0);
    hash.update(&generation.to_be_bytes());
    hash.update(&base.0);
    let mut key_count = 0usize;
    {
        let mut stmt = conn.prepare(
            "SELECT relation,repo,path FROM _source_stage_owner
             WHERE stage_id=?1 ORDER BY relation,repo,path",
        )?;
        let mut owners = stmt.query([stage_id.0.as_slice()])?;
        while let Some(owner) = owners.next()? {
            hash.update(&[1]);
            hash_str(&mut hash, &owner.get::<_, String>(0)?)?;
            hash_str(&mut hash, &owner.get::<_, String>(1)?)?;
            hash_str(&mut hash, &owner.get::<_, String>(2)?)?;
            key_count += 1;
        }
    }
    let mut stmt = conn.prepare(
        "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
         WHERE stage_id=?1 ORDER BY relation,repo,path,ordinal",
    )?;
    let mut rows = stmt.query([stage_id.0.as_slice()])?;
    let (mut row_count, mut encoded_bytes) = (0usize, 0usize);
    while let Some(row) = rows.next()? {
        let relation: String = row.get(0)?;
        let repo: String = row.get(1)?;
        let path: String = row.get(2)?;
        let ordinal: i64 = row.get(3)?;
        let encoded: Vec<u8> = row.get(4)?;
        let values = decode_values(&encoded)?;
        if encode_values(&values)? != encoded {
            return Err(SourceStageError::NonCanonical);
        }
        hash.update(&[2]);
        hash_str(&mut hash, &relation)?;
        hash_str(&mut hash, &repo)?;
        hash_str(&mut hash, &path)?;
        hash.update(&ordinal.to_be_bytes());
        hash.update(&(encoded.len() as u64).to_be_bytes());
        hash.update(&encoded);
        row_count += 1;
        let row_bytes = 24usize
            .checked_add(relation.len())
            .and_then(|n| n.checked_add(repo.len()))
            .and_then(|n| n.checked_add(path.len()))
            .and_then(|n| n.checked_add(encoded.len()))
            .ok_or(SourceStageError::EncodingTooLarge)?;
        encoded_bytes = encoded_bytes
            .checked_add(row_bytes)
            .ok_or(SourceStageError::EncodingTooLarge)?;
    }
    Ok(SealedSourceStage {
        stage_id,
        generation,
        base,
        key_count,
        row_count,
        encoded_bytes,
        digest: *hash.finalize().as_bytes(),
    })
}

fn verify_ready(conn: &Connection, expected: &SealedSourceStage) -> Result<(), SourceStageError> {
    let stored = conn
        .query_row(
            "SELECT generation,base_stamp,key_count,row_count,encoded_bytes,digest
         FROM _source_stage_ready WHERE stage_id=?1",
            [expected.stage_id.0.as_slice()],
            |row| {
                Ok(SealedSourceStage {
                    stage_id: expected.stage_id,
                    generation: row.get(0)?,
                    base: StageBase(blob_array::<32>(row.get(1)?)?),
                    key_count: row.get(2)?,
                    row_count: row.get(3)?,
                    encoded_bytes: row.get(4)?,
                    digest: blob_array::<32>(row.get(5)?)?,
                })
            },
        )
        .optional()?
        .ok_or(SourceStageError::Unsealed)?;
    let actual = derive_ready(conn, expected.stage_id, expected.generation, expected.base)?;
    if stored != *expected || actual != *expected {
        return Err(SourceStageError::SealMismatch);
    }
    Ok(())
}

fn hash_str(hash: &mut blake3::Hasher, value: &str) -> Result<(), SourceStageError> {
    let len = u32::try_from(value.len()).map_err(|_| SourceStageError::EncodingTooLarge)?;
    hash.update(&len.to_be_bytes());
    hash.update(value.as_bytes());
    Ok(())
}

fn blob_array<const N: usize>(bytes: Vec<u8>) -> rusqlite::Result<[u8; N]> {
    bytes
        .try_into()
        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(N, N as i64))
}

#[derive(Debug)]
pub(super) enum SourceStageError {
    Sqlite(rusqlite::Error),
    InvalidLimits,
    TempStoreNotFile,
    RowTooLarge {
        bytes: usize,
        limit: usize,
    },
    StageLimitExceeded,
    EncodingTooLarge,
    BadCodec,
    NonCanonical,
    Unsealed,
    StaleBase,
    SealMismatch,
    OwnerIncomplete {
        relation: String,
        repo: String,
        path: String,
    },
    VisitorFailed,
}

impl From<rusqlite::Error> for SourceStageError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl std::fmt::Display for SourceStageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(error) => error.fmt(f),
            Self::InvalidLimits => f.write_str("source-stage limits exceed hard bounds"),
            Self::TempStoreNotFile => f.write_str("source staging requires PRAGMA temp_store=FILE"),
            Self::RowTooLarge { bytes, limit } => {
                write!(f, "source row {bytes} exceeds {limit} bytes")
            }
            Self::StageLimitExceeded => {
                f.write_str("source stage exceeds total row/byte/owner bounds")
            }
            Self::EncodingTooLarge => f.write_str("source-stage encoding is too large"),
            Self::BadCodec => f.write_str("corrupt source-stage value codec"),
            Self::NonCanonical => f.write_str("non-canonical source-stage value codec"),
            Self::Unsealed => f.write_str("source stage is not sealed"),
            Self::StaleBase => f.write_str("source stage base stamp is stale"),
            Self::SealMismatch => f.write_str("source stage seal does not match staged rows"),
            Self::OwnerIncomplete {
                relation,
                repo,
                path,
            } => write!(
                f,
                "source owner ({relation:?}, {repo:?}, {path:?}) did not complete"
            ),
            Self::VisitorFailed => f.write_str("source-stage visitor failed"),
        }
    }
}

impl std::error::Error for SourceStageError {}

#[cfg(test)]
#[path = "source_stage_tests.rs"]
mod tests;
