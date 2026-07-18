//! Bounded bridge from source extraction into the sealed SQLite stage.
//!
//! This module owns no filesystem work. The reconciler feeds it completed rows
//! and owners, then applies sealed fact batches inside its short transaction.

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ast::{Rule, Value};
use crate::db::Db;
use crate::engine::Engine;
use crate::spine;

use super::source_stage::{
    read::SourceRowCursor, SealedSourceStage, SourceStage, StageBase, StageId, StageLimits,
    StageWriter,
};

const APPLY_BATCH_ROWS: usize = 4096;
const APPLY_BATCH_BYTES: usize = 256 * 1024;
const WHERE_RELATION: &str = "@where";
static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);

fn fresh_stage_id() -> StageId {
    let serial = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
    let mut hash = blake3::Hasher::new();
    hash.update(b"sprefa.full-source-stage.v1\0");
    hash.update(&std::process::id().to_be_bytes());
    hash.update(&serial.to_be_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&hash.finalize().as_bytes()[..16]);
    StageId::from_bytes(id)
}

/// Conservative stamp for the first source-only transaction cut. It covers the
/// persisted generation coordinates, the live file catalog, source rules, and
/// the source-stage row-marking codec domain. The whole-tick BaseStamp will add
/// every external EDB before delta execution is enabled beyond this seam.
pub(crate) fn source_stage_base(engine: &Engine, rules: &[&Rule]) -> Result<(i64, [u8; 32])> {
    let watermark = engine.read_generation_watermark()?;
    let files = engine.load_file_meta()?;
    let mut hash = blake3::Hasher::new();
    hash.update(b"sprefa.source-apply-base.v1\0");
    hash.update(&watermark.generation.to_be_bytes());
    hash.update(&(watermark.plan_fingerprint.len() as u64).to_be_bytes());
    hash.update(&watermark.plan_fingerprint);
    hash.update(&(watermark.schema_fingerprint.len() as u64).to_be_bytes());
    hash.update(&watermark.schema_fingerprint);
    hash.update(&watermark.data_version.to_be_bytes());
    hash.update(&watermark.user_version.to_be_bytes());
    hash.update(&watermark.carry_tx.to_be_bytes());

    let mut file_rows: Vec<_> = files.into_iter().collect();
    file_rows.sort_by(|a, b| a.0.cmp(&b.0));
    for ((repo, path, rev), (content, mtime, size, lines)) in file_rows {
        for value in [repo, path, rev, content] {
            hash.update(&(value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        hash.update(&mtime.to_be_bytes());
        hash.update(&size.to_be_bytes());
        hash.update(&lines.to_be_bytes());
    }
    let mut rule_keys: Vec<String> = rules.iter().map(|rule| rule.structural_key()).collect();
    rule_keys.sort();
    for key in rule_keys {
        hash.update(&(key.len() as u64).to_be_bytes());
        hash.update(key.as_bytes());
    }
    Ok((
        watermark.generation.saturating_add(1),
        *hash.finalize().as_bytes(),
    ))
}

pub(crate) struct FullSourceStageBuilder<'a> {
    db: &'a Db,
    writer: Option<StageWriter<'a>>,
    stage_id: StageId,
    generation: i64,
    base: StageBase,
    sealed: bool,
}

impl<'a> FullSourceStageBuilder<'a> {
    pub(crate) fn new(db: &'a Db, generation: i64, base: [u8; 32]) -> Result<Self> {
        let stage = SourceStage::open(db)?;
        let stage_id = fresh_stage_id();
        let writer = stage.begin(stage_id, StageLimits::default())?;
        Ok(Self {
            db,
            writer: Some(writer),
            stage_id,
            generation,
            base: StageBase::from_bytes(base),
            sealed: false,
        })
    }

    pub(crate) fn push(
        &mut self,
        relation: &str,
        repo: &str,
        path: &str,
        ordinal: u64,
        values: &[Value],
    ) -> Result<()> {
        if relation == WHERE_RELATION {
            anyhow::bail!("{WHERE_RELATION} is reserved for staged span metadata");
        }
        self.writer
            .as_mut()
            .expect("source stage writer is active")
            .push(relation, repo, path, ordinal, values)?;
        Ok(())
    }

    pub(crate) fn complete_owner(&mut self, relation: &str, repo: &str, path: &str) -> Result<()> {
        self.writer
            .as_mut()
            .expect("source stage writer is active")
            .complete_owner(relation, repo, path)?;
        Ok(())
    }

    pub(crate) fn push_where(
        &mut self,
        repo: &str,
        path: &str,
        ordinal: u64,
        span: spine::WhereBytes,
        text: String,
    ) -> Result<()> {
        let values = [
            Value::Int(span.string.sqlite()),
            Value::Int(i64::from(span.repo.0)),
            Value::Int(i64::from(span.rev.0)),
            Value::Int(span.file.0 as i64),
            Value::Int(i64::from(span.lo)),
            Value::Int(i64::from(span.hi)),
            Value::Text(text),
        ];
        self.writer
            .as_mut()
            .expect("source stage writer is active")
            .push(WHERE_RELATION, repo, path, ordinal, &values)?;
        Ok(())
    }

    pub(crate) fn complete_where_owner(&mut self, repo: &str, path: &str) -> Result<()> {
        self.complete_owner(WHERE_RELATION, repo, path)
    }

    pub(crate) fn seal(mut self) -> Result<PreparedSourceFacts> {
        let writer = self.writer.take().expect("source stage writer is active");
        writer.finish()?;
        let sealed =
            SourceStage::open(self.db)?.seal(self.stage_id, self.generation, self.base)?;
        self.sealed = true;
        Ok(PreparedSourceFacts { sealed })
    }
}

impl Drop for FullSourceStageBuilder<'_> {
    fn drop(&mut self) {
        if !self.sealed {
            let _ = SourceStage::existing(self.db).discard(self.stage_id);
        }
    }
}

pub(crate) struct PreparedSourceFacts {
    sealed: SealedSourceStage,
}

impl PreparedSourceFacts {
    /// Verify and insert fact rows in bounded relation-local batches.
    /// Retraction, metadata, digests, and commit remain the caller's transaction.
    pub(crate) fn apply(&self, engine: &Engine, current_base: [u8; 32]) -> Result<usize> {
        let stage = SourceStage::existing(&engine.db);
        stage.verify_seal(&self.sealed, StageBase::from_bytes(current_base))?;
        let mut relation: Option<String> = None;
        let mut batch: Vec<(String, String, Vec<Value>)> = Vec::with_capacity(APPLY_BATCH_ROWS);
        let mut inserted = 0usize;
        let mut cursor: Option<SourceRowCursor> = None;
        loop {
            let rows = stage.read_ready_rows_after(
                &self.sealed,
                cursor.as_ref(),
                APPLY_BATCH_ROWS,
                APPLY_BATCH_BYTES,
            )?;
            if rows.is_empty() {
                break;
            }
            cursor = rows.last().map(SourceRowCursor::from_row);
            for row in rows {
                let needs_flush = relation.as_deref().is_some_and(|rel| rel != row.relation)
                    || batch.len() == APPLY_BATCH_ROWS;
                if needs_flush {
                    let rel = relation.as_deref().expect("non-empty batch has a relation");
                    inserted += flush_batch(engine, rel, &mut batch)?;
                }
                relation = Some(row.relation);
                batch.push((row.repo, row.path, row.values));
            }
            if let Some(rel) = relation.as_deref() {
                inserted += flush_batch(engine, rel, &mut batch)?;
            }
        }
        Ok(inserted)
    }

    pub(crate) fn verify(&self, engine: &Engine, current_base: [u8; 32]) -> Result<()> {
        SourceStage::existing(&engine.db)
            .verify_seal(&self.sealed, StageBase::from_bytes(current_base))?;
        Ok(())
    }

    /// TEMP cleanup is outside the main WAL transaction by construction.
    pub(crate) fn discard(self, db: &Db) -> Result<()> {
        SourceStage::open(db)?.discard(self.sealed.stage_id)?;
        Ok(())
    }
}

fn flush_batch(
    engine: &Engine,
    relation: &str,
    batch: &mut Vec<(String, String, Vec<Value>)>,
) -> Result<usize> {
    if batch.is_empty() {
        return Ok(0);
    }
    if relation == WHERE_RELATION {
        flush_where_batch(engine, batch)?;
        return Ok(0);
    }
    let meta = engine
        .rels
        .get(relation)
        .ok_or_else(|| anyhow::anyhow!("unknown staged source relation {relation}"))?
        .clone();
    let inserted = engine.insert_source_rows_for_paths(relation, &meta, batch)?;
    batch.clear();
    Ok(inserted)
}

fn flush_where_batch(engine: &Engine, batch: &mut Vec<(String, String, Vec<Value>)>) -> Result<()> {
    let mut wheres = Vec::with_capacity(batch.len());
    for (repo, path, values) in batch.drain(..) {
        let values: [Value; 7] = values
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid staged where-bytes row"))?;
        let [Value::Int(string), Value::Int(repo_id), Value::Int(rev), Value::Int(file), Value::Int(lo), Value::Int(hi), Value::Text(text)] =
            values
        else {
            anyhow::bail!("invalid staged where-bytes row");
        };
        wheres.push((
            repo,
            path,
            spine::WhereBytes {
                string: spine::StringId::from_sqlite(string),
                repo: spine::RepoId(u32::try_from(repo_id)?),
                rev: spine::RevId(u32::try_from(rev)?),
                file: spine::FileId(file as u64),
                lo: u32::try_from(lo)?,
                hi: u32::try_from(hi)?,
            },
            Some(text),
        ));
    }
    engine.insert_spine_where_bytes(&wheres)?;
    Ok(())
}

#[cfg(test)]
#[path = "full_sources_tests.rs"]
mod tests;
