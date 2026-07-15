use super::*;

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
        verify_ready(self.conn, ready)
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
        if let Some(after) = after {
            let mut stmt = self.conn.prepare(
                "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
                 WHERE stage_id=?1 AND (
                   relation>?2
                   OR (relation=?2 AND ordinal>?3)
                   OR (relation=?2 AND ordinal=?3 AND repo>?4)
                   OR (relation=?2 AND ordinal=?3 AND repo=?4 AND path>?5))
                 ORDER BY relation,ordinal,repo,path LIMIT ?6",
            )?;
            let mut rows = stmt.query(params![
                ready.stage_id.0.as_slice(),
                after.relation,
                i64::try_from(after.ordinal).map_err(|_| SourceStageError::EncodingTooLarge)?,
                after.repo,
                after.path,
                i64::try_from(row_limit).map_err(|_| SourceStageError::EncodingTooLarge)?,
            ])?;
            collect_page(&mut rows, row_limit, byte_limit)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT relation,repo,path,ordinal,encoded FROM _source_stage_row
                 WHERE stage_id=?1 ORDER BY relation,ordinal,repo,path LIMIT ?2",
            )?;
            let mut rows = stmt.query(params![
                ready.stage_id.0.as_slice(),
                i64::try_from(row_limit).map_err(|_| SourceStageError::EncodingTooLarge)?,
            ])?;
            collect_page(&mut rows, row_limit, byte_limit)
        }
    }
}

fn collect_page(
    rows: &mut rusqlite::Rows<'_>,
    row_limit: usize,
    byte_limit: usize,
) -> Result<Vec<SourceStageRow>, SourceStageError> {
    let mut out = Vec::with_capacity(row_limit.min(4096));
    let mut page_bytes = 0usize;
    while let Some(row) = rows.next()? {
        let relation: String = row.get(0)?;
        let repo: String = row.get(1)?;
        let path: String = row.get(2)?;
        let encoded: Vec<u8> = row.get(4)?;
        let bytes = 24usize
            .checked_add(relation.len())
            .and_then(|n| n.checked_add(repo.len()))
            .and_then(|n| n.checked_add(path.len()))
            .and_then(|n| n.checked_add(encoded.len()))
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
        out.push(SourceStageRow {
            relation,
            repo,
            path,
            ordinal: row.get::<_, i64>(3)? as u64,
            values: decode_values(&encoded)?,
        });
        if out.len() == row_limit {
            break;
        }
    }
    Ok(out)
}
