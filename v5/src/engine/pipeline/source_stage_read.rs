use super::*;
use crate::db::SqlVal;

#[derive(Clone, Debug)]
pub(in crate::engine::pipeline) struct SourceRowCursor {
    relation: String,
    ordinal: u64,
    repo: String,
    path: String,
}

impl SourceRowCursor {
    pub(in crate::engine::pipeline) fn from_row(row: &SourceStageRow) -> Self {
        Self {
            relation: row.relation.clone(),
            ordinal: row.ordinal,
            repo: row.repo.clone(),
            path: row.path.clone(),
        }
    }
}

impl SourceStage<'_> {
    pub(in crate::engine::pipeline) fn verify_seal(
        &self,
        ready: &SealedSourceStage,
        current_base: StageBase,
    ) -> Result<(), SourceStageError> {
        if current_base != ready.base {
            return Err(SourceStageError::StaleBase);
        }
        verify_ready(self.db, ready)
    }

    /// Materialize one bounded page, closing its SQLite cursor before the
    /// caller performs live-table writes on this connection.
    pub(in crate::engine::pipeline) fn read_ready_rows_after(
        &self,
        ready: &SealedSourceStage,
        after: Option<&SourceRowCursor>,
        row_limit: usize,
        byte_limit: usize,
    ) -> Result<Vec<SourceStageRow>, SourceStageError> {
        let stage_id = SqlVal::Blob(ready.stage_id.0.to_vec());
        let limit = i64::try_from(row_limit).map_err(|_| SourceStageError::EncodingTooLarge)?;
        let rows = if let Some(after) = after {
            self.db.query_rows(
                "_source_stage_row",
                "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
                 WHERE stage_id=?1 AND (
                   relation>?2
                   OR (relation=?2 AND ordinal>?3)
                   OR (relation=?2 AND ordinal=?3 AND repo>?4)
                   OR (relation=?2 AND ordinal=?3 AND repo=?4 AND path>?5))
                 ORDER BY relation,ordinal,repo,path LIMIT ?6",
                &[
                    stage_id,
                    SqlVal::Text(after.relation.clone()),
                    SqlVal::Int(after.ordinal as i64),
                    SqlVal::Text(after.repo.clone()),
                    SqlVal::Text(after.path.clone()),
                    SqlVal::Int(limit),
                ],
                |row| {
                    let encoded: Vec<u8> = row.get(4)?;
                    Ok((
                        SourceStageRow {
                            relation: row.get(0)?,
                            repo: row.get(1)?,
                            path: row.get(2)?,
                            ordinal: row.get::<_, i64>(3)? as u64,
                            values: decode_values(&encoded)?,
                        },
                        encoded.len(),
                    ))
                },
            )
        } else {
            self.db.query_rows(
                "_source_stage_row",
                "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
                 WHERE stage_id=?1 ORDER BY relation,ordinal,repo,path LIMIT ?2",
                &[stage_id, SqlVal::Int(limit)],
                |row| {
                    let encoded: Vec<u8> = row.get(4)?;
                    Ok((
                        SourceStageRow {
                            relation: row.get(0)?,
                            repo: row.get(1)?,
                            path: row.get(2)?,
                            ordinal: row.get::<_, i64>(3)? as u64,
                            values: decode_values(&encoded)?,
                        },
                        encoded.len(),
                    ))
                },
            )
        }
        .map_err(SourceStageError::Db)?;
        collect_page(rows, row_limit, byte_limit)
    }
}

fn collect_page(
    rows: Vec<(SourceStageRow, usize)>,
    row_limit: usize,
    byte_limit: usize,
) -> Result<Vec<SourceStageRow>, SourceStageError> {
    let mut out = Vec::with_capacity(row_limit.min(4096));
    let mut page_bytes = 0usize;
    for (row, encoded_len) in rows {
        let bytes = 24usize
            .checked_add(row.relation.len())
            .and_then(|n| n.checked_add(row.repo.len()))
            .and_then(|n| n.checked_add(row.path.len()))
            .and_then(|n| n.checked_add(encoded_len))
            .ok_or(SourceStageError::EncodingTooLarge)?;
        if !out.is_empty()
            && page_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > byte_limit)
        {
            break;
        }
        page_bytes = page_bytes
            .checked_add(bytes)
            .ok_or(SourceStageError::EncodingTooLarge)?;
        out.push(row);
        if out.len() == row_limit {
            break;
        }
    }
    Ok(out)
}
