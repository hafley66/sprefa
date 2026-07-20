//! Bounded, sealed source-row staging on the engine's SQLite connection.

use crate::ast::Value;
use crate::db::{Db, SqlVal};
use std::collections::HashSet;

const MAX_BUFFERED_ROWS: usize = 4096;
const MAX_BUFFERED_BYTES: usize = 256 * 1024;
const MAX_STAGE_ROWS: usize = 1_000_000;
/// Total staged bytes across the whole stage. These rows live in the SQLite
/// TEMP store (`temp_store=FILE`), so this is a disk-volume cap, not a memory
/// cap — resident memory is bounded by `MAX_BUFFERED_BYTES` alone. The prior
/// 64MiB made `MAX_STAGE_ROWS` unreachable for any row averaging over 64
/// bytes, so a real per-line source rule over a real repository tripped the
/// byte term at roughly 650k rows and the engine refused to scan its own
/// source tree. Sized so the row bound binds first at up to 256 bytes/row.
const MAX_STAGE_BYTES: usize = 256 * 1024 * 1024;
const MAX_STAGE_OWNERS: usize = 100_000;
/// Cap on the per-owner duplicate-row filter. Beyond this many distinct rows
/// for one (relation, repo, path), dedup stops and staging degrades to
/// passing every row through, so the filter's memory stays bounded.
const MAX_OWNER_DEDUP_KEYS: usize = 250_000;
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
    /// Rows dropped by the per-owner duplicate filter before staging.
    pub(super) deduped: usize,
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
    db: &'a Db,
}

impl<'a> SourceStage<'a> {
    pub(super) fn open(db: &'a Db) -> Result<Self, SourceStageError> {
        let temp_store: i64 = db
            .query_one("_pragma", "PRAGMA temp_store", &[], |row| Ok(row.get(0)?))
            .map_err(SourceStageError::Db)?;
        if temp_store != 1 {
            return Err(SourceStageError::TempStoreNotFile);
        }
        db.execute_batch_on(
            "_source_stage",
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
        )
        .map_err(SourceStageError::Db)?;
        Ok(Self { db })
    }

    /// Borrow an already-open stage without issuing TEMP DDL or PRAGMAs.
    /// Apply uses this inside the main WAL transaction, where TEMP is read-only.
    pub(super) const fn existing(db: &'a Db) -> Self {
        Self { db }
    }

    pub(super) fn begin(
        &self,
        stage_id: StageId,
        limits: StageLimits,
    ) -> Result<StageWriter<'a>, SourceStageError> {
        validate_limits(limits)?;
        let id_val = SqlVal::Blob(stage_id.0.to_vec());
        self.db
            .exec_params(
                "_source_stage_ready",
                "DELETE FROM _source_stage_ready WHERE stage_id = ?1",
                &[id_val.clone()],
            )
            .map_err(SourceStageError::Db)?;
        self.db
            .exec_params(
                "_source_stage_row",
                "DELETE FROM _source_stage_row WHERE stage_id = ?1",
                &[id_val.clone()],
            )
            .map_err(SourceStageError::Db)?;
        self.db
            .exec_params(
                "_source_stage_owner",
                "DELETE FROM _source_stage_owner WHERE stage_id = ?1",
                &[id_val],
            )
            .map_err(SourceStageError::Db)?;
        Ok(StageWriter {
            db: self.db,
            stage_id,
            limits,
            buffered_bytes: 0,
            buffered: Vec::with_capacity(limits.max_rows),
            buffered_owners: Vec::new(),
            owner_set: HashSet::new(),
            dedup_owner: None,
            dedup_keys: HashSet::new(),
            stats: StageStats::default(),
        })
    }

    pub(super) fn seal(
        &self,
        stage_id: StageId,
        generation: i64,
        base: StageBase,
    ) -> Result<SealedSourceStage, SourceStageError> {
        let ready = derive_ready(self.db, stage_id, generation, base)?;
        self.db
            .exec_params(
                "_source_stage_ready",
                "INSERT INTO _source_stage_ready(
                     stage_id,generation,base_stamp,key_count,row_count,encoded_bytes,digest)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                &[
                    SqlVal::Blob(stage_id.0.to_vec()),
                    SqlVal::Int(generation),
                    SqlVal::Blob(base.0.to_vec()),
                    SqlVal::Int(ready.key_count as i64),
                    SqlVal::Int(ready.row_count as i64),
                    SqlVal::Int(ready.encoded_bytes as i64),
                    SqlVal::Blob(ready.digest.to_vec()),
                ],
            )
            .map_err(SourceStageError::Db)?;
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
        verify_ready(self.db, ready)?;
        let rows = self
            .db
            .query_rows(
                "_source_stage_row",
                "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
                 WHERE stage_id=?1 ORDER BY relation,repo,path,ordinal",
                &[SqlVal::Blob(ready.stage_id.0.to_vec())],
                |row| {
                    Ok(SourceStageRow {
                        relation: row.get(0)?,
                        repo: row.get(1)?,
                        path: row.get(2)?,
                        ordinal: row.get::<_, i64>(3)? as u64,
                        values: decode_values(&row.get::<_, Vec<u8>>(4)?)?,
                    })
                },
            )
            .map_err(SourceStageError::Db)?;
        let mut visited = 0;
        for row in rows {
            visit(row).map_err(|_| SourceStageError::VisitorFailed)?;
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
        verify_ready(self.db, ready)?;
        let rows = self
            .db
            .query_rows(
                "_source_stage_owner",
                "SELECT relation,repo,path FROM _source_stage_owner
                 WHERE stage_id=?1 ORDER BY relation,repo,path",
                &[SqlVal::Blob(ready.stage_id.0.to_vec())],
                |row| {
                    Ok(SourceStageOwner {
                        relation: row.get(0)?,
                        repo: row.get(1)?,
                        path: row.get(2)?,
                    })
                },
            )
            .map_err(SourceStageError::Db)?;
        let mut visited = 0;
        for row in rows {
            visit(row).map_err(|_| SourceStageError::VisitorFailed)?;
            visited += 1;
        }
        Ok(visited)
    }

    /// Cleanup is deliberately separate from consume. Call it only after the
    /// main WAL semantic transaction commits or aborts; consume itself never
    /// mutates TEMP seal/row state inside that transaction.
    pub(super) fn discard(&self, stage_id: StageId) -> Result<(), SourceStageError> {
        let id_val = SqlVal::Blob(stage_id.0.to_vec());
        self.db
            .exec_params(
                "_source_stage_ready",
                "DELETE FROM _source_stage_ready WHERE stage_id=?1",
                &[id_val.clone()],
            )
            .map_err(SourceStageError::Db)?;
        self.db
            .exec_params(
                "_source_stage_row",
                "DELETE FROM _source_stage_row WHERE stage_id=?1",
                &[id_val.clone()],
            )
            .map_err(SourceStageError::Db)?;
        self.db
            .exec_params(
                "_source_stage_owner",
                "DELETE FROM _source_stage_owner WHERE stage_id=?1",
                &[id_val],
            )
            .map_err(SourceStageError::Db)?;
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
    db: &'a Db,
    stage_id: StageId,
    limits: StageLimits,
    buffered_bytes: usize,
    buffered: Vec<BufferedRow>,
    /// Owner completions accumulated since the last flush; landed as one
    /// chunked INSERT alongside the row buffer (N+1 law — `complete_owner`
    /// fires once per (relation, repo, path), which is once per file-rel pair
    /// per tick, and a per-call INSERT was the `_source_stage_owner` runtime
    /// N+1 scream).
    buffered_owners: Vec<(String, String, String)>,
    /// Every owner completed over the writer's lifetime — the in-memory twin
    /// of the table's INSERT OR IGNORE dedup, so repeat completions neither
    /// re-buffer nor double-count `stats.owners`, across flushes.
    owner_set: HashSet<(String, String, String)>,
    /// Encoded rows already staged for `dedup_owner`. A source relation is a
    /// set and an owner's rows are replaced wholesale, so an extractor that
    /// emits the same tuple twice for one file is emitting waste: a per-line
    /// rule whose regex matches per character produced ~34 identical
    /// `(path, line)` rows per line, 10x the distinct row count, and the
    /// staged-byte cap fired long before the scan finished. Downstream
    /// `insert_source_rows_for_paths` drops them at the rel table's unique
    /// constraint anyway; dropping them here saves the staging write too.
    dedup_owner: Option<(String, String, String)>,
    dedup_keys: HashSet<Vec<u8>>,
    stats: StageStats,
}

#[derive(PartialEq, Eq)]
enum RowSeen {
    First,
    Duplicate,
}

impl StageWriter<'_> {
    fn limit_error(&self, term: &'static str, value: usize, limit: usize) -> SourceStageError {
        SourceStageError::StageLimitExceeded {
            term,
            value,
            limit,
            rows: self.stats.rows,
            staged_bytes: self.stats.staged_bytes,
            owners: self.stats.owners,
        }
    }

    /// Duplicate filter scoped to the owner currently being staged. Rows for
    /// one (relation, repo, path) arrive contiguously from `stage_parsed_file`;
    /// a change of owner resets the key set, so the filter holds one owner's
    /// rows at most, and it gives up entirely past `MAX_OWNER_DEDUP_KEYS`.
    fn note_owner_row(
        &mut self,
        relation: &str,
        repo: &str,
        path: &str,
        encoded: &[u8],
    ) -> RowSeen {
        let owner_changed = self
            .dedup_owner
            .as_ref()
            .is_none_or(|(rel, owner_repo, owner_path)| {
                rel != relation || owner_repo != repo || owner_path != path
            });
        if owner_changed {
            self.dedup_owner = Some((relation.to_string(), repo.to_string(), path.to_string()));
            self.dedup_keys.clear();
        }
        if self.dedup_keys.len() >= MAX_OWNER_DEDUP_KEYS {
            return RowSeen::First;
        }
        if self.dedup_keys.insert(encoded.to_vec()) {
            RowSeen::First
        } else {
            RowSeen::Duplicate
        }
    }

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
        if self.note_owner_row(relation, repo, path, &encoded) == RowSeen::Duplicate {
            self.stats.deduped += 1;
            return Ok(());
        }
        if self.stats.rows >= self.limits.max_stage_rows {
            return Err(self.limit_error("rows", self.stats.rows, self.limits.max_stage_rows));
        }
        let staged_total = self
            .stats
            .staged_bytes
            .checked_add(bytes)
            .ok_or(SourceStageError::EncodingTooLarge)?;
        if staged_total > self.limits.max_stage_bytes {
            return Err(self.limit_error("bytes", staged_total, self.limits.max_stage_bytes));
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
        let key = (relation.to_string(), repo.to_string(), path.to_string());
        if self.owner_set.contains(&key) {
            return Ok(());
        }
        if self.stats.owners == self.limits.max_owners {
            return Err(self.limit_error("owners", self.stats.owners + 1, self.limits.max_owners));
        }
        self.buffered_owners.push(key.clone());
        self.owner_set.insert(key);
        self.stats.owners += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SourceStageError> {
        if !self.buffered.is_empty() {
            insert_source_rows(self.db, self.stage_id, &self.buffered)
                .map_err(SourceStageError::Db)?;
            for row in self.buffered.drain(..) {
                self.buffered_bytes -= row.bytes;
            }
            // `flushes` keeps meaning ROW-buffer flushes (pinned by the
            // stats tests); an owner-only flush does not count.
            self.stats.flushes += 1;
        }
        if !self.buffered_owners.is_empty() {
            insert_source_owners(self.db, self.stage_id, &self.buffered_owners)
                .map_err(SourceStageError::Db)?;
            self.buffered_owners.clear();
        }
        Ok(())
    }
}

/// Chunked multi-row insert of buffered owner completions — the batched twin
/// of `insert_source_rows` for `_source_stage_owner`. OR IGNORE is belt-and-
/// suspenders: `owner_set` already dedups within the writer's lifetime, and
/// `begin` cleared the stage's prior rows.
fn insert_source_owners(
    db: &Db,
    stage_id: StageId,
    owners: &[(String, String, String)],
) -> anyhow::Result<()> {
    if owners.is_empty() {
        return Ok(());
    }
    const NCOL: usize = 4;
    let chunk_rows = (crate::db::PARAM_BUDGET / NCOL).max(1);
    let tuple = "(?,?,?,?,1)";
    for chunk in owners.chunks(chunk_rows) {
        let values = vec![tuple; chunk.len()].join(", ");
        let sql = format!(
            "INSERT OR IGNORE INTO _source_stage_owner(stage_id,relation,repo,path,completed) VALUES {values}"
        );
        let mut params: Vec<SqlVal> = Vec::with_capacity(chunk.len() * NCOL);
        for (relation, repo, path) in chunk {
            params.push(SqlVal::Blob(stage_id.0.to_vec()));
            params.push(SqlVal::Text(relation.clone()));
            params.push(SqlVal::Text(repo.clone()));
            params.push(SqlVal::Text(path.clone()));
        }
        db.exec_params("_source_stage_owner", &sql, &params)?;
    }
    Ok(())
}

/// Chunked multi-row insert into the temp staging table. One logical statement
/// per chunk (N+1 law); never recorded in the source write ledger because this
/// is internal staging, not a public relation.
fn insert_source_rows(
    db: &Db,
    stage_id: StageId,
    rows: &[BufferedRow],
) -> anyhow::Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    const NCOL: usize = 6;
    let chunk_rows = (crate::db::PARAM_BUDGET / NCOL).max(1);
    let tuple = "(?,?,?,?,?,?)";
    for chunk in rows.chunks(chunk_rows) {
        let values = vec![tuple; chunk.len()].join(", ");
        let sql = format!(
            "INSERT INTO _source_stage_row(stage_id,relation,repo,path,ordinal,encoded) VALUES {values}"
        );
        let mut params: Vec<SqlVal> = Vec::with_capacity(chunk.len() * NCOL);
        for row in chunk {
            params.push(SqlVal::Blob(stage_id.0.to_vec()));
            params.push(SqlVal::Text(row.relation.clone()));
            params.push(SqlVal::Text(row.repo.clone()));
            params.push(SqlVal::Text(row.path.clone()));
            params.push(SqlVal::Int(row.ordinal as i64));
            params.push(SqlVal::Blob(row.encoded.clone()));
        }
        db.exec_params("_source_stage_row", &sql, &params)?;
    }
    Ok(())
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
    db: &Db,
    stage_id: StageId,
    generation: i64,
    base: StageBase,
) -> Result<SealedSourceStage, SourceStageError> {
    let incomplete: Option<(String, String, String)> = db
        .query_opt(
            "_source_stage_row",
            "SELECT r.relation,r.repo,r.path FROM _source_stage_row AS r
         LEFT JOIN _source_stage_owner AS o
           ON o.stage_id=r.stage_id AND o.relation=r.relation
          AND o.repo=r.repo AND o.path=r.path
         WHERE r.stage_id=?1 AND o.stage_id IS NULL LIMIT 1",
            &[SqlVal::Blob(stage_id.0.to_vec())],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(SourceStageError::Db)?;
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
        let owners = db
            .query_rows(
                "_source_stage_owner",
                "SELECT relation,repo,path FROM _source_stage_owner
                 WHERE stage_id=?1 ORDER BY relation,repo,path",
                &[SqlVal::Blob(stage_id.0.to_vec())],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(SourceStageError::Db)?;
        for (relation, repo, path) in owners {
            hash.update(&[1]);
            hash_str(&mut hash, &relation)?;
            hash_str(&mut hash, &repo)?;
            hash_str(&mut hash, &path)?;
            key_count += 1;
        }
    }
    let rows = db
        .query_rows(
            "_source_stage_row",
            "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
             WHERE stage_id=?1 ORDER BY relation,repo,path,ordinal",
            &[SqlVal::Blob(stage_id.0.to_vec())],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                ))
            },
        )
        .map_err(SourceStageError::Db)?;
    let (mut row_count, mut encoded_bytes) = (0usize, 0usize);
    for (relation, repo, path, ordinal, encoded) in rows {
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

fn verify_ready(db: &Db, expected: &SealedSourceStage) -> Result<(), SourceStageError> {
    let stored = db
        .query_opt(
            "_source_stage_ready",
            "SELECT generation,base_stamp,key_count,row_count,encoded_bytes,digest
         FROM _source_stage_ready WHERE stage_id=?1",
            &[SqlVal::Blob(expected.stage_id.0.to_vec())],
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
        .map_err(SourceStageError::Db)?
        .ok_or(SourceStageError::Unsealed)?;
    let actual = derive_ready(db, expected.stage_id, expected.generation, expected.base)?;
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

fn blob_array<const N: usize>(bytes: Vec<u8>) -> Result<[u8; N], SourceStageError> {
    bytes
        .try_into()
        .map_err(|_| SourceStageError::EncodingTooLarge)
}

#[derive(Debug)]
pub(super) enum SourceStageError {
    Db(anyhow::Error),
    InvalidLimits,
    TempStoreNotFile,
    RowTooLarge {
        bytes: usize,
        limit: usize,
    },
    /// One of the three total-stage terms hit its cap. The counters travel
    /// with the error so the CLI line names the binding term; a bare
    /// "exceeds total row/byte/owner bounds" cost hours of misattribution.
    StageLimitExceeded {
        term: &'static str,
        value: usize,
        limit: usize,
        rows: usize,
        staged_bytes: usize,
        owners: usize,
    },
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

impl From<anyhow::Error> for SourceStageError {
    fn from(error: anyhow::Error) -> Self {
        Self::Db(error)
    }
}

impl std::fmt::Display for SourceStageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(error) => error.fmt(f),
            Self::InvalidLimits => f.write_str("source-stage limits exceed hard bounds"),
            Self::TempStoreNotFile => f.write_str("source staging requires PRAGMA temp_store=FILE"),
            Self::RowTooLarge { bytes, limit } => {
                write!(f, "source row {bytes} exceeds {limit} bytes")
            }
            Self::StageLimitExceeded {
                term,
                value,
                limit,
                rows,
                staged_bytes,
                owners,
            } => write!(
                f,
                "source stage bound `{term}` exceeded: {value} > {limit} \
                 (staged rows={rows}, bytes={staged_bytes}, owners={owners})"
            ),
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
