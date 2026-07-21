use anyhow::Result;

use super::Storage;
use crate::ast::Value;
use crate::db::Db;
use crate::spine::{StringId, SymSink};

const SQLITE_CALL_DELTA_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS _call_owner (
    owner_id INTEGER PRIMARY KEY,
    repo_sid INTEGER NOT NULL REFERENCES _strings(id),
    rev_sid INTEGER NOT NULL REFERENCES _strings(id),
    path_sid INTEGER NOT NULL REFERENCES _strings(id),
    fact_digest_sid INTEGER NOT NULL REFERENCES _strings(id),
    generation INTEGER NOT NULL CHECK(generation >= 0),
    def_digest BLOB NOT NULL CHECK(length(def_digest) = 32),
    UNIQUE(repo_sid, rev_sid, path_sid)
);

CREATE TABLE IF NOT EXISTS _call_raw_site (
    site_id INTEGER PRIMARY KEY,
    owner_id INTEGER NOT NULL REFERENCES _call_owner(owner_id) ON DELETE CASCADE,
    occurrence_sid INTEGER NOT NULL REFERENCES _strings(id),
    caller_sid INTEGER NOT NULL REFERENCES _strings(id),
    callee_sid INTEGER NOT NULL REFERENCES _strings(id),
    classification_sid INTEGER REFERENCES _strings(id),
    file_sid INTEGER NOT NULL REFERENCES _strings(id),
    line INTEGER NOT NULL CHECK(line >= 0),
    UNIQUE(owner_id, occurrence_sid)
);
CREATE INDEX IF NOT EXISTS _call_raw_site_callee_owner
    ON _call_raw_site(callee_sid, owner_id);
CREATE INDEX IF NOT EXISTS _call_raw_site_caller_owner
    ON _call_raw_site(caller_sid, owner_id);

CREATE TABLE IF NOT EXISTS _call_resolution (
    site_id INTEGER NOT NULL REFERENCES _call_raw_site(site_id) ON DELETE CASCADE,
    callee_sid INTEGER NOT NULL REFERENCES _strings(id),
    kind_sid INTEGER NOT NULL REFERENCES _strings(id),
    resolution_sid INTEGER NOT NULL REFERENCES _strings(id),
    dependency_sid INTEGER NOT NULL REFERENCES _strings(id),
    PRIMARY KEY(site_id, callee_sid, kind_sid)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS _call_edge_support (
    caller_sid INTEGER NOT NULL REFERENCES _strings(id),
    callee_sid INTEGER NOT NULL REFERENCES _strings(id),
    kind_sid INTEGER NOT NULL REFERENCES _strings(id),
    rev_sid INTEGER NOT NULL REFERENCES _strings(id),
    support_count INTEGER NOT NULL CHECK(support_count > 0),
    PRIMARY KEY(caller_sid, callee_sid, kind_sid, rev_sid)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS _call_def_bucket (
    repo_sid INTEGER NOT NULL REFERENCES _strings(id),
    rev_sid INTEGER NOT NULL REFERENCES _strings(id),
    name_sid INTEGER NOT NULL REFERENCES _strings(id),
    candidate_count INTEGER NOT NULL CHECK(candidate_count > 0),
    unique_sym_sid INTEGER REFERENCES _strings(id),
    PRIMARY KEY(repo_sid, rev_sid, name_sid)
) WITHOUT ROWID;

-- Owned def-site store: one row per (sym, rev) definition. Carries the bare
-- name the `call_name` family projects PLUS the full def-site tuple the
-- `call_def_rev`/`call_def` families project (repo, kind, file, line, end,
-- rev). Rewritten only on a full refresh (defs change together); an owner/site
-- delta requires the def set unchanged, so it never touches this table. That
-- disjointness is the point — a site-only delta leaves _call_def untouched, so
-- the def families' rel footprint ({_call_def}) misses the delta's changed-set
-- and the router skips them. See src/engine/family/call_name.rs. PK is
-- (sym_sid, rev_sid): def_rev_rows is deduped on (repo, sym, rev) upstream and
-- sym is repo-qualified, so (sym, rev) is unique — no def row is lost.
-- `end` is a SQL keyword, so it is quoted throughout.
CREATE TABLE IF NOT EXISTS _call_def (
    sym_sid INTEGER NOT NULL REFERENCES _strings(id),
    name_sid INTEGER NOT NULL REFERENCES _strings(id),
    repo_sid INTEGER NOT NULL REFERENCES _strings(id),
    kind_sid INTEGER NOT NULL REFERENCES _strings(id),
    file_sid INTEGER NOT NULL REFERENCES _strings(id),
    line INTEGER NOT NULL CHECK(line >= 0),
    "end" INTEGER NOT NULL CHECK("end" >= 0),
    rev_sid INTEGER NOT NULL REFERENCES _strings(id),
    PRIMARY KEY(sym_sid, rev_sid)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS _call_delta_marker (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    schema_version INTEGER NOT NULL,
    extractor_digest BLOB NOT NULL CHECK(length(extractor_digest) = 32),
    generation INTEGER NOT NULL CHECK(generation >= 0),
    complete INTEGER NOT NULL CHECK(complete IN (0, 1)),
    module_digest BLOB NOT NULL CHECK(length(module_digest) = 32),
    scip_digest BLOB NOT NULL CHECK(length(scip_digest) = 32)
);
"#;

fn ensure_sqlite_call_delta_schema(db: &Db) -> Result<()> {
    ensure_sqlite_call_def_shape(db)?;
    Storage::execute_batch(db, SQLITE_CALL_DELTA_SCHEMA)
}

/// Migrate a pre-existing `_call_def` to the 8-col def-site shape. Probes the
/// live column count; anything other than 8 (the old 2-col name store, or a
/// partial/absent table) is dropped so the schema batch's
/// `CREATE TABLE IF NOT EXISTS` rebuilds the 8-col table. Data loss is fine:
/// `_call_def` is rewritten wholesale on every full refresh, and the delta path
/// bails via the `schema_version` bump until that rebuild lands.
fn ensure_sqlite_call_def_shape(db: &Db) -> Result<()> {
    let col_count: i64 = db.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('_call_def')",
        [],
        |row| row.get(0),
    )?;
    if col_count != 8 {
        Storage::execute(db, "DROP TABLE IF EXISTS _call_def")?;
    }
    Ok(())
}

pub(crate) struct CallFamilyWrite<'a> {
    pub(crate) owners: &'a [CallOwnerBaseline],
    pub(crate) def_buckets: &'a [CallDefBucketBaseline],
    pub(crate) defs: &'a [CallDefBaseline],
    pub(crate) scip_dependency_digest: [u8; 32],
    pub(crate) module_dependency_digest: [u8; 32],
}

/// One def-site fact, the source `replace_sqlite_call_def` interns and writes
/// to the owned `_call_def` store. Replaces the old public-rel-shaped
/// `def_rev_rows`/`name_rows` pair (P4, capstone cutover): those two vectors
/// existed only to feed the now-deleted legacy `call_def_rev`/`call_name`
/// writes, and `_call_def`'s population piggybacked on them. This is the
/// direct owned-input shape instead.
pub(crate) struct CallDefBaseline {
    pub(crate) repo: String,
    pub(crate) sym: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) end: u32,
    pub(crate) rev: String,
}

pub(crate) struct CallOwnerBaseline {
    pub(crate) repo: String,
    pub(crate) rev: String,
    pub(crate) path: String,
    pub(crate) fact_digest: String,
    pub(crate) def_digest: [u8; 32],
    pub(crate) sites: Vec<CallSiteBaseline>,
}

pub(crate) struct CallSiteBaseline {
    pub(crate) occurrence: String,
    pub(crate) caller: String,
    pub(crate) callee: String,
    pub(crate) classification: Option<String>,
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) edge: Option<CallResolvedEdge>,
}

pub(crate) struct CallDefBucketBaseline {
    pub(crate) repo: String,
    pub(crate) rev: String,
    pub(crate) name: String,
    pub(crate) candidate_count: usize,
    pub(crate) unique_sym: Option<String>,
}

pub(crate) struct CallResolvedEdge {
    pub(crate) caller: String,
    pub(crate) callee: String,
}

pub(crate) struct CallOwnerDelta {
    pub(crate) owner: CallOwnerBaseline,
    pub(crate) scip_dependency_digest: [u8; 32],
    pub(crate) module_dependency_digest: [u8; 32],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CallDeltaOutcome {
    Applied,
    Unsupported(&'static str),
}

/// Backend contract for persisting the logical call-relation family. The
/// public `call_*` rels are no longer written here at all (P4, capstone
/// cutover): the family router (`Engine::flip_call_rels_via_router`) is the
/// SOLE writer of every public call rel, deriving them from the owned
/// `_call_*` tables this trait's methods populate.
pub(crate) trait CallStore {
    fn persist_call_family(&self, write: CallFamilyWrite<'_>) -> Result<()>;
    /// Delete a gone rev's rows straight from the 6 owned `_call_*` tables;
    /// returns the total row count deleted so the caller can skip the router
    /// flip when nothing moved. See `sweep_sqlite_gone_call_inputs`.
    fn sweep_gone_call_inputs(&self) -> Result<usize>;
    fn apply_call_owner_delta(&self, delta: CallOwnerDelta) -> Result<CallDeltaOutcome>;
}

impl CallStore for Db {
    fn persist_call_family(&self, write: CallFamilyWrite<'_>) -> Result<()> {
        persist_sqlite_call_family(self, write)
    }

    fn sweep_gone_call_inputs(&self) -> Result<usize> {
        sweep_sqlite_gone_call_inputs(self)
    }

    fn apply_call_owner_delta(&self, delta: CallOwnerDelta) -> Result<CallDeltaOutcome> {
        apply_sqlite_call_owner_delta(self, delta)
    }
}

fn persist_sqlite_call_family(db: &Db, write: CallFamilyWrite<'_>) -> Result<()> {
    ensure_sqlite_call_delta_schema(db)?;
    let owns_transaction = Storage::is_autocommit(db);
    if owns_transaction {
        Storage::begin(db)?;
    }

    let result = (|| {
        replace_sqlite_call_def(db, write.defs)?;
        replace_sqlite_call_baseline(
            db,
            write.owners,
            write.def_buckets,
            &write.scip_dependency_digest,
            &write.module_dependency_digest,
        )
    })();

    if owns_transaction {
        match result {
            Ok(()) => Storage::commit(db),
            Err(error) => {
                let _ = Storage::rollback(db);
                Err(error)
            }
        }
    } else {
        result
    }
}

fn apply_sqlite_call_owner_delta(db: &Db, delta: CallOwnerDelta) -> Result<CallDeltaOutcome> {
    ensure_sqlite_call_delta_schema(db)?;
    let owns_transaction = Storage::is_autocommit(db);
    if owns_transaction {
        Storage::begin_immediate(db)?;
    }
    let result = apply_sqlite_call_owner_delta_inner(db, &delta);
    match result {
        Ok(CallDeltaOutcome::Applied) => {
            if owns_transaction {
                Storage::commit(db)?;
            }
            Ok(CallDeltaOutcome::Applied)
        }
        Ok(unsupported @ CallDeltaOutcome::Unsupported(_)) => {
            if owns_transaction {
                Storage::rollback(db)?;
            }
            Ok(unsupported)
        }
        Err(error) => {
            if owns_transaction {
                let _ = Storage::rollback(db);
            }
            Err(error)
        }
    }
}

fn apply_sqlite_call_owner_delta_inner(
    db: &Db,
    delta: &CallOwnerDelta,
) -> Result<CallDeltaOutcome> {
    let marker = {
        use rusqlite::OptionalExtension;
        let mut statement = db.prepare(
            "SELECT schema_version, complete, generation, module_digest, scip_digest \
             FROM _call_delta_marker WHERE singleton = 1",
        )?;
        statement.query_row([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        }).optional()?
    };
    let Some((schema_version, complete, generation, module_digest, scip_digest)) = marker else {
        return Ok(CallDeltaOutcome::Unsupported("call-baseline-incomplete"));
    };
    if schema_version != 2 {
        return Ok(CallDeltaOutcome::Unsupported("call-schema-version"));
    }
    if complete != 1 {
        return Ok(CallDeltaOutcome::Unsupported("call-baseline-incomplete"));
    }
    if delta.scip_dependency_digest != [0; 32] || scip_digest.as_slice() != [0; 32] {
        return Ok(CallDeltaOutcome::Unsupported("call-scip-active"));
    }
    if scip_digest.as_slice() != delta.scip_dependency_digest {
        return Ok(CallDeltaOutcome::Unsupported("call-scip-dependency-changed"));
    }
    if module_digest.as_slice() != delta.module_dependency_digest {
        return Ok(CallDeltaOutcome::Unsupported("call-module-dependency-changed"));
    }

    let repo_sid = StringId::of(&delta.owner.repo).sqlite();
    let rev_sid = StringId::of(&delta.owner.rev).sqlite();
    let path_sid = StringId::of(&delta.owner.path).sqlite();
    let (owners, repos, non_work): (i64, i64, i64) = db.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT repo_sid), \
                COALESCE(SUM(CASE WHEN rev_sid = ?1 THEN 0 ELSE 1 END), 0) \
         FROM _call_owner",
        [rev_sid],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if owners == 0 || repos != 1 {
        return Ok(CallDeltaOutcome::Unsupported("call-multi-repo-baseline"));
    }
    if non_work != 0 {
        return Ok(CallDeltaOutcome::Unsupported("call-non-work-baseline"));
    }
    let existing = {
        use rusqlite::OptionalExtension;
        let mut statement = db.prepare(
            "SELECT owner_id, def_digest FROM _call_owner \
             WHERE repo_sid = ?1 AND rev_sid = ?2 AND path_sid = ?3",
        )?;
        statement.query_row([repo_sid, rev_sid, path_sid], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        }).optional()?
    };
    let Some((owner_id, old_def_digest)) = existing else {
        return Ok(CallDeltaOutcome::Unsupported("call-owner-missing"));
    };
    if old_def_digest.as_slice() != delta.owner.def_digest {
        return Ok(CallDeltaOutcome::Unsupported("call-definition-changed"));
    }

    ensure_sqlite_call_delta_temp_tables(db)?;
    for table in [
        "_call_delta_callee",
        "_call_affected_site",
        "_call_affected_kind",
        "_call_affected_edge_rev",
        "_call_affected_edge",
    ] {
        Storage::execute(db, &format!("DELETE FROM {table}"))?;
    }
    Storage::execute(db, &format!(
        "INSERT OR IGNORE INTO _call_delta_callee(callee_sid) \
         SELECT callee_sid FROM _call_raw_site WHERE owner_id = {owner_id}"
    ))?;
    let new_callees: Vec<Vec<Value>> = delta.owner.sites.iter()
        .map(|site| vec![Value::Int(StringId::of(&site.callee).sqlite())])
        .collect();
    Storage::insert_rows(db, "_call_delta_callee", &["callee_sid"], &new_callees)?;
    let unsupported_buckets: i64 = db.query_row(
        "SELECT COUNT(*) FROM _call_delta_callee AS c \
         LEFT JOIN _call_def_bucket AS b \
           ON b.repo_sid = ?1 AND b.rev_sid = ?2 AND b.name_sid = c.callee_sid \
         WHERE b.candidate_count IS NULL OR b.candidate_count != 1 OR b.unique_sym_sid IS NULL",
        [repo_sid, rev_sid],
        |row| row.get(0),
    )?;
    if unsupported_buckets != 0 {
        return Ok(CallDeltaOutcome::Unsupported("call-ambiguous-callee"));
    }

    collect_sqlite_call_affected_keys(db, owner_id)?;
    Storage::execute(db, &format!(
        "DELETE FROM _call_resolution WHERE site_id IN (\
         SELECT site_id FROM _call_raw_site WHERE owner_id = {owner_id})"
    ))?;
    Storage::execute(db, &format!("DELETE FROM _call_raw_site WHERE owner_id = {owner_id}"))?;
    insert_sqlite_call_delta_sites(db, owner_id, repo_sid, rev_sid, &delta.owner)?;
    collect_sqlite_call_affected_keys(db, owner_id)?;
    reproject_sqlite_call_affected_keys(db)?;

    let mut sink = SymSink::new();
    let fact_digest_sid = sink.sym(&delta.owner.fact_digest).cell();
    Storage::flush_syms_keyed(db, &mut sink, "INSERT _strings (spine/call)")?;
    let next_generation = generation + 1;
    let mut update_owner = db.prepare(
        "UPDATE _call_owner SET fact_digest_sid = ?1, generation = ?2 WHERE owner_id = ?3",
    )?;
    update_owner.execute([fact_digest_sid, next_generation, owner_id])?;
    let mut update_marker = db.prepare(
        "UPDATE _call_delta_marker SET generation = ?1 WHERE singleton = 1",
    )?;
    update_marker.execute([next_generation])?;
    Ok(CallDeltaOutcome::Applied)
}

const SQLITE_CALL_DELTA_TEMP_SCHEMA: &str = r#"
CREATE TEMP TABLE IF NOT EXISTS _call_delta_callee(
    callee_sid INTEGER PRIMARY KEY
);
CREATE TEMP TABLE IF NOT EXISTS _call_affected_site(
    repo_sid INTEGER NOT NULL,
    caller_sid INTEGER NOT NULL,
    callee_sid INTEGER NOT NULL,
    file_sid INTEGER NOT NULL,
    line INTEGER NOT NULL,
    PRIMARY KEY(repo_sid, caller_sid, callee_sid, file_sid, line)
) WITHOUT ROWID;
CREATE TEMP TABLE IF NOT EXISTS _call_affected_kind(
    caller_sid INTEGER NOT NULL,
    classification_sid INTEGER NOT NULL,
    PRIMARY KEY(caller_sid, classification_sid)
) WITHOUT ROWID;
CREATE TEMP TABLE IF NOT EXISTS _call_affected_edge_rev(
    caller_sid INTEGER NOT NULL,
    callee_sid INTEGER NOT NULL,
    kind_sid INTEGER NOT NULL,
    rev_sid INTEGER NOT NULL,
    PRIMARY KEY(caller_sid, callee_sid, kind_sid, rev_sid)
) WITHOUT ROWID;
CREATE TEMP TABLE IF NOT EXISTS _call_affected_edge(
    caller_sid INTEGER NOT NULL,
    callee_sid INTEGER NOT NULL,
    kind_sid INTEGER NOT NULL,
    PRIMARY KEY(caller_sid, callee_sid, kind_sid)
) WITHOUT ROWID;
"#;

fn ensure_sqlite_call_delta_temp_tables(db: &Db) -> Result<()> {
    Storage::execute_batch(db, SQLITE_CALL_DELTA_TEMP_SCHEMA)
}

fn collect_sqlite_call_affected_keys(db: &Db, owner_id: i64) -> Result<()> {
    Storage::execute(db, &format!(
        "INSERT OR IGNORE INTO _call_affected_site \
         SELECT o.repo_sid, s.caller_sid, s.callee_sid, s.file_sid, s.line \
         FROM _call_raw_site AS s JOIN _call_owner AS o USING(owner_id) \
         WHERE s.owner_id = {owner_id}"
    ))?;
    Storage::execute(db, &format!(
        "INSERT OR IGNORE INTO _call_affected_kind \
         SELECT caller_sid, classification_sid FROM _call_raw_site \
         WHERE owner_id = {owner_id} AND classification_sid IS NOT NULL AND caller_sid != 0"
    ))?;
    Storage::execute(db, &format!(
        "INSERT OR IGNORE INTO _call_affected_edge_rev \
         SELECT s.caller_sid, r.callee_sid, r.kind_sid, o.rev_sid \
         FROM _call_resolution AS r \
         JOIN _call_raw_site AS s USING(site_id) \
         JOIN _call_owner AS o USING(owner_id) \
         WHERE s.owner_id = {owner_id}"
    ))?;
    Storage::execute(
        db,
        "INSERT OR IGNORE INTO _call_affected_edge \
         SELECT caller_sid, callee_sid, kind_sid FROM _call_affected_edge_rev",
    )?;
    Ok(())
}

fn insert_sqlite_call_delta_sites(
    db: &Db,
    owner_id: i64,
    repo_sid: i64,
    rev_sid: i64,
    owner: &CallOwnerBaseline,
) -> Result<()> {
    let owner_key = format!("{}\0{}\0{}", owner.repo, owner.rev, owner.path);
    let mut sink = SymSink::new();
    let mut rows = Vec::with_capacity(owner.sites.len());
    for site in &owner.sites {
        let site_id = StringId::of(&format!("{owner_key}\0{}", site.occurrence)).sqlite();
        rows.push(vec![
            Value::Int(site_id),
            Value::Int(owner_id),
            Value::Int(sink.sym(&site.occurrence).cell()),
            Value::Int(sink.sym(&site.caller).cell()),
            Value::Int(sink.sym(&site.callee).cell()),
            site.classification.as_deref()
                .map(|kind| Value::Int(sink.sym(kind).cell()))
                .unwrap_or(Value::Null),
            Value::Int(sink.sym(&site.file).cell()),
            Value::Int(site.line as i64),
        ]);
    }
    let call_sid = sink.sym("call").cell();
    Storage::flush_syms_keyed(db, &mut sink, "INSERT _strings (spine/call)")?;
    Storage::insert_rows(
        db,
        "_call_raw_site",
        &["site_id", "owner_id", "occurrence_sid", "caller_sid", "callee_sid", "classification_sid", "file_sid", "line"],
        &rows,
    )?;
    Storage::execute(db, &format!(
        "INSERT INTO _call_resolution(site_id, callee_sid, kind_sid, resolution_sid, dependency_sid) \
         SELECT s.site_id, b.unique_sym_sid, {call_sid}, s.occurrence_sid, s.callee_sid \
         FROM _call_raw_site AS s \
         JOIN _call_def_bucket AS b \
           ON b.repo_sid = {repo_sid} AND b.rev_sid = {rev_sid} AND b.name_sid = s.callee_sid \
         WHERE s.owner_id = {owner_id} AND s.caller_sid != 0 \
           AND b.candidate_count = 1 AND b.unique_sym_sid IS NOT NULL"
    ))?;
    Ok(())
}

/// Recompute the OWNED `_call_edge_support` aggregate over the affected keys
/// an owner/site delta just touched. P4 (capstone cutover) deleted the 4
/// public-rel DELETE/INSERT blocks this function used to also run
/// (`call_site`/`call_kind`/`call_edge_rev`/`call_edge`): the family router
/// is now the sole writer of every public call rel, reprojecting them fresh
/// from `_call_owner`/`_call_raw_site`/`_call_resolution`/`_call_def` via
/// `Engine::flip_call_rels_via_router` right after this returns. Retaining
/// `_call_edge_support` here (rather than folding its GROUP BY into the
/// `CallEdge`/`CallEdgeRev` families, which already recompute the same
/// aggregate from `_call_resolution` directly) is a deliberate deferral — see
/// the cutover plan's Risk 2.
fn reproject_sqlite_call_affected_keys(db: &Db) -> Result<()> {
    Storage::execute(
        db,
        "DELETE FROM _call_edge_support WHERE EXISTS (SELECT 1 FROM _call_affected_edge_rev AS a \
         WHERE a.caller_sid = _call_edge_support.caller_sid \
           AND a.callee_sid = _call_edge_support.callee_sid \
           AND a.kind_sid = _call_edge_support.kind_sid \
           AND a.rev_sid = _call_edge_support.rev_sid)",
    )?;
    Storage::execute(
        db,
        "INSERT INTO _call_edge_support(caller_sid, callee_sid, kind_sid, rev_sid, support_count) \
         SELECT s.caller_sid, r.callee_sid, r.kind_sid, o.rev_sid, COUNT(*) \
         FROM _call_resolution AS r \
         JOIN _call_raw_site AS s USING(site_id) \
         JOIN _call_owner AS o USING(owner_id) \
         JOIN _call_affected_edge_rev AS a \
           ON a.caller_sid = s.caller_sid AND a.callee_sid = r.callee_sid \
          AND a.kind_sid = r.kind_sid AND a.rev_sid = o.rev_sid \
         GROUP BY s.caller_sid, r.callee_sid, r.kind_sid, o.rev_sid",
    )?;
    Ok(())
}

fn replace_sqlite_call_baseline(
    db: &Db,
    owners: &[CallOwnerBaseline],
    def_buckets: &[CallDefBucketBaseline],
    scip_dependency_digest: &[u8; 32],
    module_dependency_digest: &[u8; 32],
) -> Result<()> {
    let generation: i64 = db.query_row(
        "SELECT COALESCE((SELECT generation FROM _call_delta_marker WHERE singleton = 1), 0) + 1",
        [],
        |row| row.get(0),
    )?;
    Storage::execute(db, "DELETE FROM _call_delta_marker")?;
    Storage::execute(db, "DELETE FROM _call_edge_support")?;
    Storage::execute(db, "DELETE FROM _call_def_bucket")?;
    Storage::execute(db, "DELETE FROM _call_resolution")?;
    Storage::execute(db, "DELETE FROM _call_raw_site")?;
    Storage::execute(db, "DELETE FROM _call_owner")?;

    let mut sink = SymSink::new();
    let mut owner_rows = Vec::with_capacity(owners.len());
    let mut site_rows = Vec::new();
    let mut resolution_rows = Vec::new();
    let mut bucket_rows = Vec::with_capacity(def_buckets.len());
    for owner in owners {
        let owner_key = format!("{}\0{}\0{}", owner.repo, owner.rev, owner.path);
        let owner_id = StringId::of(&owner_key).sqlite();
        owner_rows.push((
            owner_id,
            sink.sym(&owner.repo).cell(),
            sink.sym(&owner.rev).cell(),
            sink.sym(&owner.path).cell(),
            sink.sym(&owner.fact_digest).cell(),
            owner.def_digest,
        ));
        for site in &owner.sites {
            let site_key = format!("{owner_key}\0{}", site.occurrence);
            let site_id = StringId::of(&site_key).sqlite();
            let caller_sid = sink.sym(&site.caller).cell();
            let raw_callee_sid = sink.sym(&site.callee).cell();
            site_rows.push(vec![
                Value::Int(site_id),
                Value::Int(owner_id),
                Value::Int(sink.sym(&site.occurrence).cell()),
                Value::Int(caller_sid),
                Value::Int(raw_callee_sid),
                site.classification
                    .as_deref()
                    .map(|kind| Value::Int(sink.sym(kind).cell()))
                    .unwrap_or(Value::Null),
                Value::Int(sink.sym(&site.file).cell()),
                Value::Int(site.line as i64),
            ]);
            if let Some(edge) = &site.edge {
                let resolution = format!("{}\0{}\0call", edge.caller, edge.callee);
                resolution_rows.push(vec![
                    Value::Int(site_id),
                    Value::Int(sink.sym(&edge.callee).cell()),
                    Value::Int(sink.sym("call").cell()),
                    Value::Int(sink.sym(&resolution).cell()),
                    Value::Int(raw_callee_sid),
                ]);
            }
        }
    }
    for bucket in def_buckets {
        bucket_rows.push(vec![
            Value::Int(sink.sym(&bucket.repo).cell()),
            Value::Int(sink.sym(&bucket.rev).cell()),
            Value::Int(sink.sym(&bucket.name).cell()),
            Value::Int(bucket.candidate_count as i64),
            bucket.unique_sym
                .as_deref()
                .map(|sym| Value::Int(sink.sym(sym).cell()))
                .unwrap_or(Value::Null),
        ]);
    }
    Storage::flush_syms_keyed(db, &mut sink, "INSERT _strings (spine/call)")?;
    insert_sqlite_call_owners(db, generation, &owner_rows)?;
    Storage::insert_rows(
        db,
        "_call_raw_site",
        &["site_id", "owner_id", "occurrence_sid", "caller_sid", "callee_sid", "classification_sid", "file_sid", "line"],
        &site_rows,
    )?;
    Storage::insert_rows(
        db,
        "_call_resolution",
        &["site_id", "callee_sid", "kind_sid", "resolution_sid", "dependency_sid"],
        &resolution_rows,
    )?;
    Storage::insert_rows(
        db,
        "_call_def_bucket",
        &["repo_sid", "rev_sid", "name_sid", "candidate_count", "unique_sym_sid"],
        &bucket_rows,
    )?;
    Storage::execute(db,
        "INSERT INTO _call_edge_support(caller_sid, callee_sid, kind_sid, rev_sid, support_count) \
         SELECT s.caller_sid, r.callee_sid, r.kind_sid, o.rev_sid, COUNT(*) \
         FROM _call_resolution AS r \
         JOIN _call_raw_site AS s USING(site_id) \
         JOIN _call_owner AS o USING(owner_id) \
         GROUP BY s.caller_sid, r.callee_sid, r.kind_sid, o.rev_sid")?;
    insert_sqlite_call_marker(
        db,
        generation,
        &call_baseline_digest(owners, def_buckets),
        scip_dependency_digest,
        module_dependency_digest,
    )?;
    Ok(())
}

fn insert_sqlite_call_owners(
    db: &Db,
    generation: i64,
    rows: &[(i64, i64, i64, i64, i64, [u8; 32])],
) -> Result<()> {
    for chunk in rows.chunks(400) {
        let values = vec!["(?, ?, ?, ?, ?, ?, ?)"; chunk.len()].join(", ");
        let sql = format!(
            "INSERT INTO _call_owner(owner_id, repo_sid, rev_sid, path_sid, fact_digest_sid, generation, def_digest) VALUES {values}"
        );
        let mut params = Vec::with_capacity(chunk.len() * 7);
        for (owner, repo, rev, path, fact, digest) in chunk {
            params.push(rusqlite::types::Value::Integer(*owner));
            params.push(rusqlite::types::Value::Integer(*repo));
            params.push(rusqlite::types::Value::Integer(*rev));
            params.push(rusqlite::types::Value::Integer(*path));
            params.push(rusqlite::types::Value::Integer(*fact));
            params.push(rusqlite::types::Value::Integer(generation));
            params.push(rusqlite::types::Value::Blob(digest.to_vec()));
        }
        db.prepare(&sql)?.execute(rusqlite::params_from_iter(params))?;
    }
    Ok(())
}

fn insert_sqlite_call_marker(
    db: &Db,
    generation: i64,
    digest: &[u8; 32],
    scip_dependency_digest: &[u8; 32],
    module_dependency_digest: &[u8; 32],
) -> Result<()> {
    let mut statement = db.prepare(
        "INSERT INTO _call_delta_marker(singleton, schema_version, extractor_digest, generation, complete, module_digest, scip_digest) \
         VALUES (1, 2, ?1, ?2, 1, ?3, ?4)",
    )?;
    statement.execute(rusqlite::params![
        digest.as_slice(),
        generation,
        module_dependency_digest.as_slice(),
        scip_dependency_digest.as_slice(),
    ])?;
    Ok(())
}

fn call_baseline_digest(
    owners: &[CallOwnerBaseline],
    def_buckets: &[CallDefBucketBaseline],
) -> [u8; 32] {
    let mut ordered: Vec<&CallOwnerBaseline> = owners.iter().collect();
    ordered.sort_by(|a, b| (&a.repo, &a.rev, &a.path).cmp(&(&b.repo, &b.rev, &b.path)));
    let mut hash = blake3::Hasher::new();
    for owner in ordered {
        hash.update(owner.repo.as_bytes());
        hash.update(&[0]);
        hash.update(owner.rev.as_bytes());
        hash.update(&[0]);
        hash.update(owner.path.as_bytes());
        hash.update(&[0]);
        hash.update(owner.fact_digest.as_bytes());
        hash.update(&owner.def_digest);
        for site in &owner.sites {
            hash.update(site.occurrence.as_bytes());
            hash.update(site.caller.as_bytes());
            hash.update(site.callee.as_bytes());
            if let Some(edge) = &site.edge {
                hash.update(edge.caller.as_bytes());
                hash.update(edge.callee.as_bytes());
            }
        }
    }
    let mut buckets: Vec<&CallDefBucketBaseline> = def_buckets.iter().collect();
    buckets.sort_by(|a, b| (&a.repo, &a.rev, &a.name).cmp(&(&b.repo, &b.rev, &b.name)));
    for bucket in buckets {
        hash.update(bucket.repo.as_bytes());
        hash.update(bucket.rev.as_bytes());
        hash.update(bucket.name.as_bytes());
        hash.update(&(bucket.candidate_count as u64).to_le_bytes());
        if let Some(sym) = &bucket.unique_sym {
            hash.update(sym.as_bytes());
        }
    }
    *hash.finalize().as_bytes()
}

/// P4's replacement for the legacy REV_TWINS-based call sweep (capstone
/// cutover): a gone rev's rows are deleted straight from the 6 owned
/// `_call_*` tables rather than surfacing via a public-rel twin delete +
/// legacy rebuild. Explicit per-table DELETEs, not FK CASCADE — FK
/// enforcement is not guaranteed on every connection (see
/// `call_owner_delta_without_foreign_keys_leaves_no_orphan_resolutions`),
/// so a gone rev must not rely on it to reach `_call_raw_site`/
/// `_call_resolution`. Deletion order respects the dependency chain
/// (`_call_resolution` -> `_call_raw_site` -> `_call_owner`: each DELETE's
/// WHERE clause still needs its parent row intact to compute the join);
/// `_call_def`/`_call_def_bucket`/`_call_edge_support` carry `rev_sid`
/// directly and delete independently of that chain. Returns the total row
/// count deleted, so the caller (`Engine::sweep_gone_call_inputs`) can skip
/// the family-router flip when a tick's sweep is a no-op.
///
/// Precondition: a temp table `_live_rev_scope(rev TEXT PRIMARY KEY)` holding
/// this tick's live revs already exists — the caller (`sweep_gone_revs`)
/// builds and populates it before sweeping any twin, call included.
fn sweep_sqlite_gone_call_inputs(db: &Db) -> Result<usize> {
    ensure_sqlite_call_delta_schema(db)?;
    const GONE: &str = "rev_sid NOT IN (SELECT sprf_sym(rev) FROM _live_rev_scope)";
    let mut moved = 0usize;
    moved += Storage::execute(db, &format!(
        "DELETE FROM _call_resolution WHERE site_id IN (\
         SELECT s.site_id FROM _call_raw_site AS s JOIN _call_owner AS o USING(owner_id) \
         WHERE o.{GONE})"
    ))?;
    moved += Storage::execute(db, &format!(
        "DELETE FROM _call_raw_site WHERE owner_id IN (\
         SELECT owner_id FROM _call_owner WHERE {GONE})"
    ))?;
    moved += Storage::execute(db, &format!("DELETE FROM _call_owner WHERE {GONE}"))?;
    moved += Storage::execute(db, &format!("DELETE FROM _call_def WHERE {GONE}"))?;
    moved += Storage::execute(db, &format!("DELETE FROM _call_def_bucket WHERE {GONE}"))?;
    moved += Storage::execute(db, &format!("DELETE FROM _call_edge_support WHERE {GONE}"))?;
    Ok(moved)
}

/// Rebuild the owned 8-col `_call_def` def-site store directly from the
/// extraction's def facts (P4, capstone cutover: replaces the old
/// public-rel-shaped `def_rev_rows`/`name_rows` split — those two vectors
/// existed only to feed the now-deleted legacy `call_def_rev`/`call_name`
/// writes). Interns each def's strings itself (one `SymSink` batch, flushed
/// once), then a single plural `insert_rows`.
///
/// PK is (sym_sid, rev_sid) — the caller already dedupes on (repo, sym, rev)
/// upstream (sym is repo-qualified, so (sym, rev) is unique); the `seen` set
/// here is a defensive second dedup, not the primary one. A full refresh
/// rewrites the table wholesale; owner/site deltas leave it alone (defs
/// unchanged), which is what lets the router skip the def families.
fn replace_sqlite_call_def(db: &Db, defs: &[CallDefBaseline]) -> Result<()> {
    Storage::execute(db, "DELETE FROM _call_def")?;
    let mut sink = SymSink::new();
    let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
    let mut rows: Vec<Vec<Value>> = Vec::with_capacity(defs.len());
    for def in defs {
        let sym_sid = sink.sym(&def.sym).cell();
        let name_sid = sink.sym(&def.name).cell();
        let repo_sid = sink.sym(&def.repo).cell();
        let kind_sid = sink.sym(&def.kind).cell();
        let file_sid = sink.sym(&def.file).cell();
        let rev_sid = sink.sym(&def.rev).cell();
        if seen.insert((sym_sid, rev_sid)) {
            rows.push(vec![
                Value::Int(sym_sid),
                Value::Int(name_sid),
                Value::Int(repo_sid),
                Value::Int(kind_sid),
                Value::Int(file_sid),
                Value::Int(def.line as i64),
                Value::Int(def.end as i64),
                Value::Int(rev_sid),
            ]);
        }
    }
    Storage::flush_syms_keyed(db, &mut sink, "INSERT _strings (spine/call)")?;
    // `insert_rows` quotes each column name, so `end` is passed bare here (it
    // becomes `"end"` in the INSERT) — unlike Ctx::scan, which needs it
    // pre-quoted.
    Storage::insert_rows(
        db,
        "_call_def",
        &["sym_sid", "name_sid", "repo_sid", "kind_sid", "file_sid", "line", "end", "rev_sid"],
        &rows,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct RecordingStore {
        operations: RefCell<Vec<String>>,
    }

    impl CallStore for RecordingStore {
        fn persist_call_family(&self, _write: CallFamilyWrite<'_>) -> Result<()> {
            self.operations.borrow_mut().push("persist_call_family".to_string());
            Ok(())
        }

        fn sweep_gone_call_inputs(&self) -> Result<usize> {
            self.operations.borrow_mut().push("sweep_gone_call_inputs".to_string());
            Ok(0)
        }

        fn apply_call_owner_delta(&self, _delta: CallOwnerDelta) -> Result<CallDeltaOutcome> {
            self.operations.borrow_mut().push("apply_call_owner_delta".to_string());
            Ok(CallDeltaOutcome::Applied)
        }
    }

    #[test]
    fn call_family_is_one_logical_storage_operation() {
        let store = RecordingStore::default();
        store.persist_call_family(CallFamilyWrite {
            owners: &[],
            def_buckets: &[],
            defs: &[],
            scip_dependency_digest: [0; 32],
            module_dependency_digest: [0; 32],
        }).unwrap();

        let operations = store.operations.borrow();
        assert_eq!(operations.as_slice(), ["persist_call_family"]);
    }

    fn table_info(db: &Db, table: &str) -> Vec<(String, String, i64)> {
        let mut statement = db.prepare(&format!("PRAGMA table_info('{table}')")).unwrap();
        statement.query_map([], |row| {
            Ok((row.get(1)?, row.get(2)?, row.get(5)?))
        }).unwrap().collect::<rusqlite::Result<_>>().unwrap()
    }

    fn unique_indexes(db: &Db, table: &str) -> Vec<Vec<String>> {
        let mut statement = db.prepare(&format!("PRAGMA index_list('{table}')")).unwrap();
        let names: Vec<String> = statement.query_map([], |row| {
            let unique: i64 = row.get(2)?;
            if unique != 0 {
                Ok(Some(row.get::<_, String>(1)?))
            } else {
                Ok(None)
            }
        }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
            .into_iter().flatten().collect();
        names.into_iter().map(|name| {
            let mut statement = db.prepare(&format!("PRAGMA index_info('{name}')")).unwrap();
            statement.query_map([], |row| row.get(2)).unwrap()
                .collect::<rusqlite::Result<_>>().unwrap()
        }).collect()
    }

    fn all_indexes(db: &Db, table: &str) -> Vec<Vec<String>> {
        let mut statement = db.prepare(&format!("PRAGMA index_list('{table}')")).unwrap();
        let names: Vec<String> = statement.query_map([], |row| row.get(1)).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap();
        names.into_iter().map(|name| {
            let mut statement = db.prepare(&format!("PRAGMA index_info('{name}')")).unwrap();
            statement.query_map([], |row| row.get(2)).unwrap()
                .collect::<rusqlite::Result<_>>().unwrap()
        }).collect()
    }

    fn foreign_keys(db: &Db, table: &str) -> Vec<(String, String, String, String)> {
        let mut statement = db.prepare(&format!("PRAGMA foreign_key_list('{table}')")).unwrap();
        statement.query_map([], |row| {
            Ok((row.get(3)?, row.get(2)?, row.get(4)?, row.get(6)?))
        }).unwrap().collect::<rusqlite::Result<_>>().unwrap()
    }

    fn delta_site(path: &str, line: u32, callee: &str, classification: &str) -> CallSiteBaseline {
        CallSiteBaseline {
            occurrence: format!("{path}:{line}:0"),
            caller: "repo::run".into(),
            callee: callee.into(),
            classification: Some(classification.into()),
            file: path.into(),
            line,
            edge: Some(CallResolvedEdge {
                caller: "repo::run".into(),
                callee: format!("repo::{callee}"),
            }),
        }
    }

    fn delta_test_db(extra_rev: bool) -> Db {
        let db = crate::db::open(None).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys = ON; \
             CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL); \
             INSERT INTO _strings(id, content) VALUES (0, ''); \
             CREATE TABLE rel_call_site(repo INTEGER, caller INTEGER, callee INTEGER, file INTEGER, line INTEGER, PRIMARY KEY(repo, caller, callee, file, line)); \
             CREATE TABLE rel_call_kind(fn INTEGER, kind INTEGER, PRIMARY KEY(fn, kind)); \
             CREATE TABLE rel_call_edge_rev(caller INTEGER, callee INTEGER, kind INTEGER, rev INTEGER, PRIMARY KEY(caller, callee, kind, rev)); \
             CREATE TABLE rel_call_edge(caller INTEGER, callee INTEGER, kind INTEGER, PRIMARY KEY(caller, callee, kind));",
        ).unwrap();
        ensure_sqlite_call_delta_schema(&db).unwrap();
        let mut owners = vec![
            CallOwnerBaseline {
                repo: "repo".into(), rev: "WORK".into(), path: "a.rs".into(),
                fact_digest: "old-a".into(), def_digest: [1; 32],
                sites: vec![delta_site("a.rs", 10, "target", "read")],
            },
            CallOwnerBaseline {
                repo: "repo".into(), rev: "WORK".into(), path: "b.rs".into(),
                fact_digest: "old-b".into(), def_digest: [2; 32],
                sites: vec![delta_site("b.rs", 20, "target", "read")],
            },
        ];
        if extra_rev {
            owners.push(CallOwnerBaseline {
                repo: "repo".into(), rev: "HEAD".into(), path: "head.rs".into(),
                fact_digest: "head".into(), def_digest: [3; 32], sites: Vec::new(),
            });
        }
        let mut buckets = vec![
            CallDefBucketBaseline {
                repo: "repo".into(), rev: "WORK".into(), name: "target".into(),
                candidate_count: 1, unique_sym: Some("repo::target".into()),
            },
            CallDefBucketBaseline {
                repo: "repo".into(), rev: "WORK".into(), name: "other".into(),
                candidate_count: 1, unique_sym: Some("repo::other".into()),
            },
            CallDefBucketBaseline {
                repo: "repo".into(), rev: "WORK".into(), name: "ambiguous".into(),
                candidate_count: 2, unique_sym: None,
            },
        ];
        if extra_rev {
            buckets.push(CallDefBucketBaseline {
                repo: "repo".into(), rev: "HEAD".into(), name: "target".into(),
                candidate_count: 1, unique_sym: Some("repo::target".into()),
            });
        }
        replace_sqlite_call_baseline(&db, &owners, &buckets, &[0; 32], &[5; 32]).unwrap();
        db.execute_batch(
            "INSERT OR IGNORE INTO rel_call_site SELECT o.repo_sid, s.caller_sid, s.callee_sid, s.file_sid, s.line FROM _call_raw_site s JOIN _call_owner o USING(owner_id); \
             INSERT OR IGNORE INTO rel_call_kind SELECT caller_sid, classification_sid FROM _call_raw_site WHERE caller_sid != 0 AND classification_sid IS NOT NULL; \
             INSERT OR IGNORE INTO rel_call_edge_rev SELECT caller_sid, callee_sid, kind_sid, rev_sid FROM _call_edge_support; \
             INSERT OR IGNORE INTO rel_call_edge SELECT DISTINCT caller_sid, callee_sid, kind_sid FROM _call_edge_support;",
        ).unwrap();

        // Seed the 8-col `_call_def` def-site store plus its legacy public twins
        // (`rel_call_def_rev` / `rel_call_def`) so the CallDefRev/CallDef
        // families derive non-vacuously and the flip-parity rail
        // (`family_flip_overwrites_legacy_call_rels_identically`) has an oracle
        // to diff the family output against. `replace_sqlite_call_def` (the
        // real, owned-input path) populates `_call_def`; the `rel_call_def_rev`/
        // `rel_call_def` INSERTs below are test-only fixture seeding standing in
        // for what the pre-P4 legacy writer used to produce.
        {
            // (sym, name, kind, file, line, end, rev); one def present at two
            // revs so CallDef collapses it while CallDefRev keeps both rows.
            let defs_data = [
                ("repo::run", "run", "fn", "a.rs", 1u32, 12u32, "WORK"),
                ("repo::target", "target", "fn", "a.rs", 5, 8, "WORK"),
                ("repo::other", "other", "fn", "b.rs", 3, 9, "WORK"),
                ("repo::target", "target", "fn", "a.rs", 5, 8, "HEAD"),
            ];
            let defs: Vec<CallDefBaseline> = defs_data.iter()
                .map(|&(sym, name, kind, file, line, end, rev)| CallDefBaseline {
                    repo: "repo".into(),
                    sym: sym.into(),
                    name: name.into(),
                    kind: kind.into(),
                    file: file.into(),
                    line,
                    end,
                    rev: rev.into(),
                })
                .collect();
            replace_sqlite_call_def(&db, &defs).unwrap();

            let mut sink = SymSink::new();
            let repo_cell = sink.sym("repo").cell();
            let def_rev_rows: Vec<Vec<Value>> = defs_data.iter()
                .map(|&(sym, _name, kind, file, line, end, rev)| vec![
                    Value::Int(repo_cell),
                    Value::Int(sink.sym(sym).cell()),
                    Value::Int(sink.sym(kind).cell()),
                    Value::Int(sink.sym(file).cell()),
                    Value::Int(line as i64),
                    Value::Int(end as i64),
                    Value::Int(sink.sym(rev).cell()),
                ])
                .collect();
            Storage::flush_syms(&db, &mut sink).unwrap();
            db.execute_batch(
                "CREATE TABLE rel_call_def_rev(repo INTEGER, sym INTEGER, kind INTEGER, file INTEGER, line INTEGER, \"end\" INTEGER, rev INTEGER, PRIMARY KEY(repo, sym, kind, file, line, \"end\", rev)); \
                 CREATE TABLE rel_call_def(repo INTEGER, sym INTEGER, kind INTEGER, file INTEGER, line INTEGER, \"end\" INTEGER, PRIMARY KEY(repo, sym, kind, file, line, \"end\"));",
            ).unwrap();
            Storage::insert_rows(
                &db,
                "rel_call_def_rev",
                &["repo", "sym", "kind", "file", "line", "end", "rev"],
                &def_rev_rows,
            ).unwrap();
            db.execute_batch(
                "INSERT OR IGNORE INTO rel_call_def SELECT DISTINCT repo, sym, kind, file, line, \"end\" FROM rel_call_def_rev;",
            ).unwrap();
        }
        db
    }

    fn delta_for(callee: &str, def_digest: [u8; 32], scip: [u8; 32]) -> CallOwnerDelta {
        CallOwnerDelta {
            owner: CallOwnerBaseline {
                repo: "repo".into(), rev: "WORK".into(), path: "a.rs".into(),
                fact_digest: "new-a".into(), def_digest,
                sites: vec![delta_site("a.rs", 11, callee, "write")],
            },
            scip_dependency_digest: scip,
            module_dependency_digest: [5; 32],
        }
    }

    /// Owned-table counters only — the public call rels are no longer written
    /// by this storage layer at all (P4, capstone cutover): they are the
    /// family router's job (`Engine::flip_call_rels_via_router`), which runs
    /// one layer up from `apply_call_owner_delta`. A storage-only test cannot
    /// observe that flip, so it has nothing to assert about `rel_call_*`.
    fn delta_snapshot(db: &Db) -> (i64, i64, i64, i64) {
        db.query_row(
            "SELECT \
               (SELECT generation FROM _call_delta_marker), \
               (SELECT COUNT(*) FROM _call_raw_site), \
               (SELECT COUNT(*) FROM _call_resolution), \
               (SELECT COUNT(*) FROM _call_edge_support)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap()
    }

    #[test]
    fn call_owner_delta_reprojects_affected_keys_and_retains_duplicate_support() {
        let db = delta_test_db(false);
        assert_eq!(CallStore::apply_call_owner_delta(&db, delta_for("other", [1; 32], [0; 32])).unwrap(), CallDeltaOutcome::Applied);
        assert_eq!(delta_snapshot(&db), (2, 2, 2, 2));
        let target_support: i64 = db.query_row(
            "SELECT support_count FROM _call_edge_support WHERE callee_sid = ?1",
            [StringId::of("repo::target").sqlite()],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(target_support, 1, "the untouched duplicate owner was retracted");
        // Owned-table check (not `rel_call_kind`, which the storage layer no
        // longer writes): a.rs's site reclassified read -> write, b.rs's stays
        // read, so the owned set carries both kinds.
        let kinds: Vec<i64> = {
            let mut statement = db.prepare(
                "SELECT DISTINCT classification_sid FROM _call_raw_site \
                 WHERE classification_sid IS NOT NULL ORDER BY classification_sid",
            ).unwrap();
            statement.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        let mut expected = vec![StringId::of("read").sqlite(), StringId::of("write").sqlite()];
        expected.sort();
        assert_eq!(kinds, expected);
        let fact_sid: i64 = db.query_row(
            "SELECT fact_digest_sid FROM _call_owner WHERE path_sid = ?1",
            [StringId::of("a.rs").sqlite()],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(fact_sid, StringId::of("new-a").sqlite());
    }

    #[test]
    fn call_owner_delta_without_foreign_keys_leaves_no_orphan_resolutions() {
        let db = delta_test_db(false);
        db.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
        assert_eq!(db.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0)).unwrap(), 0);

        assert_eq!(
            CallStore::apply_call_owner_delta(&db, delta_for("other", [1; 32], [0; 32])).unwrap(),
            CallDeltaOutcome::Applied,
        );
        let orphan_resolutions: i64 = db.query_row(
            "SELECT COUNT(*) FROM _call_resolution AS r \
             LEFT JOIN _call_raw_site AS s USING(site_id) WHERE s.site_id IS NULL",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(orphan_resolutions, 0);
        assert_eq!(delta_snapshot(&db), (2, 2, 2, 2));
    }

    #[test]
    fn call_owner_delta_unsupported_preflights_do_not_mutate() {
        for (db, delta, reason) in [
            (delta_test_db(false), delta_for("other", [9; 32], [0; 32]), "call-definition-changed"),
            (delta_test_db(false), delta_for("other", [1; 32], [7; 32]), "call-scip-active"),
            (delta_test_db(false), delta_for("ambiguous", [1; 32], [0; 32]), "call-ambiguous-callee"),
            (delta_test_db(true), delta_for("other", [1; 32], [0; 32]), "call-non-work-baseline"),
        ] {
            let before = delta_snapshot(&db);
            assert_eq!(
                CallStore::apply_call_owner_delta(&db, delta).unwrap(),
                CallDeltaOutcome::Unsupported(reason),
            );
            assert_eq!(delta_snapshot(&db), before, "unsupported {reason} mutated permanent state");
        }
    }

    #[test]
    fn full_refresh_populates_complete_call_delta_baseline() {
        let db = crate::db::open(None).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys = ON; \
             CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL); \
             INSERT INTO _strings(id, content) VALUES (0, '');",
        ).unwrap();
        ensure_sqlite_call_delta_schema(&db).unwrap();
        let owners = vec![CallOwnerBaseline {
            repo: "repo".into(),
            rev: "WORK".into(),
            path: "src/a.rs".into(),
            fact_digest: "fact-a".into(),
            def_digest: [7; 32],
            sites: vec![
                CallSiteBaseline {
                    occurrence: "src/a.rs:10:0".into(),
                    caller: "repo::src/a.rs::function::run".into(),
                    callee: "execute".into(),
                    classification: Some("write".into()),
                    file: "src/a.rs".into(),
                    line: 10,
                    edge: Some(CallResolvedEdge {
                        caller: "repo::src/a.rs::function::run".into(),
                        callee: "repo::src/b.rs::function::target".into(),
                    }),
                },
                CallSiteBaseline {
                    occurrence: "src/a.rs:11:0".into(),
                    caller: "repo::src/a.rs::function::run".into(),
                    callee: "prepare".into(),
                    classification: Some("read".into()),
                    file: "src/a.rs".into(),
                    line: 11,
                    edge: Some(CallResolvedEdge {
                        caller: "repo::src/a.rs::function::run".into(),
                        callee: "repo::src/b.rs::function::target".into(),
                    }),
                },
            ],
        }];
        let buckets = vec![
            CallDefBucketBaseline {
                repo: "repo".into(),
                rev: "WORK".into(),
                name: "target".into(),
                candidate_count: 1,
                unique_sym: Some("repo::src/b.rs::function::target".into()),
            },
            CallDefBucketBaseline {
                repo: "repo".into(),
                rev: "WORK".into(),
                name: "duplicate".into(),
                candidate_count: 2,
                unique_sym: None,
            },
        ];
        let scip_dependency_digest = [3; 32];
        let module_dependency_digest = [5; 32];
        replace_sqlite_call_baseline(
            &db,
            &owners,
            &buckets,
            &scip_dependency_digest,
            &module_dependency_digest,
        ).unwrap();

        for (table, expected) in [
            ("_call_owner", 1),
            ("_call_raw_site", 2),
            ("_call_resolution", 2),
            ("_call_edge_support", 1),
            ("_call_def_bucket", 2),
        ] {
            let count: i64 = db.query_row(
                &format!("SELECT COUNT(*) FROM {table}"),
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(count, expected, "unexpected {table} baseline count");
        }
        let support: i64 = db.query_row(
            "SELECT support_count FROM _call_edge_support",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(support, 2);
        let rev_sid: i64 = db.query_row(
            "SELECT rev_sid FROM _call_edge_support",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(rev_sid, StringId::of("WORK").sqlite());
        let classified: Vec<i64> = {
            let mut statement = db.prepare(
                "SELECT classification_sid FROM _call_raw_site ORDER BY line",
            ).unwrap();
            statement.query_map([], |row| row.get(0)).unwrap()
                .collect::<rusqlite::Result<_>>().unwrap()
        };
        assert_eq!(classified, [
            StringId::of("write").sqlite(),
            StringId::of("read").sqlite(),
        ]);
        let bucket_rows: Vec<(i64, Option<i64>)> = {
            let mut statement = db.prepare(
                "SELECT candidate_count, unique_sym_sid FROM _call_def_bucket ORDER BY candidate_count",
            ).unwrap();
            statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap()
                .collect::<rusqlite::Result<_>>().unwrap()
        };
        assert_eq!(bucket_rows, vec![
            (1, Some(StringId::of("repo::src/b.rs::function::target").sqlite())),
            (2, None),
        ]);
        let marker: (i64, i64) = db.query_row(
            "SELECT generation, complete FROM _call_delta_marker WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(marker, (1, 1));
        let dependency_digests: (Vec<u8>, Vec<u8>) = db.query_row(
            "SELECT scip_digest, module_digest FROM _call_delta_marker WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(dependency_digests.0, scip_dependency_digest);
        assert_eq!(dependency_digests.1, module_dependency_digest);
    }

    #[test]
    fn sqlite_call_delta_schema_invariants() {
        let db = crate::db::open(None).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys = ON; \
             CREATE TABLE _strings (id INTEGER PRIMARY KEY, content TEXT NOT NULL); \
             INSERT INTO _strings(id, content) VALUES (1, 'sid');",
        ).unwrap();
        ensure_sqlite_call_delta_schema(&db).unwrap();

        let tables = [
            "_call_owner",
            "_call_raw_site",
            "_call_resolution",
            "_call_edge_support",
            "_call_def_bucket",
            "_call_def",
            "_call_delta_marker",
        ];
        for table in tables {
            let columns = table_info(&db, table);
            assert!(!columns.is_empty(), "missing {table}");
            assert!(
                columns.iter().all(|(_, ty, _)| ty != "TEXT"),
                "{table} placed TEXT on the compact call-delta rail: {columns:?}",
            );
        }

        // _call_def carries the 8-col def-site shape (sym, name, repo, kind,
        // file, line, end, rev) that the CallDefRev/CallDef families project.
        let call_def_columns = table_info(&db, "_call_def");
        assert_eq!(
            call_def_columns.len(), 8,
            "_call_def must carry the 8-col def-site shape: {call_def_columns:?}",
        );

        for (table, id) in [
            ("_call_owner", "owner_id"),
            ("_call_raw_site", "site_id"),
        ] {
            let columns = table_info(&db, table);
            assert!(columns.iter().any(|(name, ty, pk)| {
                name == id && ty == "INTEGER" && *pk == 1
            }), "{table}.{id} is not an INTEGER PRIMARY KEY: {columns:?}");
        }

        assert!(unique_indexes(&db, "_call_owner").contains(&vec![
            "repo_sid".into(), "rev_sid".into(), "path_sid".into(),
        ]));
        assert!(unique_indexes(&db, "_call_raw_site").contains(&vec![
            "owner_id".into(), "occurrence_sid".into(),
        ]));
        let raw_site_indexes = all_indexes(&db, "_call_raw_site");
        assert!(raw_site_indexes.contains(&vec!["callee_sid".into(), "owner_id".into()]));
        assert!(raw_site_indexes.contains(&vec!["caller_sid".into(), "owner_id".into()]));

        for table in [
            "_call_owner",
            "_call_raw_site",
            "_call_resolution",
            "_call_edge_support",
            "_call_def_bucket",
            "_call_def",
        ] {
            let columns = table_info(&db, table);
            let foreign_keys = foreign_keys(&db, table);
            for (column, _, _) in columns.iter().filter(|(name, _, _)| name.ends_with("_sid")) {
                assert!(foreign_keys.iter().any(|(from, target, to, _)| {
                    from == column && target == "_strings" && to == "id"
                }), "{table}.{column} is not a canonical _strings foreign key: {foreign_keys:?}");
            }
        }
        assert!(foreign_keys(&db, "_call_raw_site").contains(&(
            "owner_id".into(), "_call_owner".into(), "owner_id".into(), "CASCADE".into(),
        )));
        assert!(foreign_keys(&db, "_call_resolution").contains(&(
            "site_id".into(), "_call_raw_site".into(), "site_id".into(), "CASCADE".into(),
        )));

        for (table, key_len) in [
            ("_call_resolution", 3),
            ("_call_edge_support", 4),
            ("_call_def_bucket", 3),
            ("_call_def", 2),
        ] {
            let sql: String = db.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            ).unwrap();
            assert!(sql.contains("WITHOUT ROWID"), "{table} retained a rowid: {sql}");
            assert!(unique_indexes(&db, table).iter().any(|cols| cols.len() == key_len));
        }

        let marker_rows: i64 = db.query_row(
            "SELECT count(*) FROM _call_delta_marker", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(marker_rows, 0, "slice one must not claim a populated baseline");

        assert!(db.exec(
            "INSERT INTO _call_owner VALUES (1, 1, 1, 1, 1, -1, zeroblob(32))",
        ).is_err(), "negative owner generation was accepted");
        db.exec(
            "INSERT INTO _call_owner VALUES (1, 1, 1, 1, 1, 0, zeroblob(32))",
        ).unwrap();
        assert!(db.exec(
            "INSERT INTO _call_raw_site VALUES (1, 1, 1, 1, 1, NULL, 1, -1)",
        ).is_err(), "negative source line was accepted");
        assert!(db.exec(
            "INSERT INTO _call_edge_support VALUES (1, 1, 1, 1, 0)",
        ).is_err(), "non-positive edge support was accepted");
        assert!(db.exec(
            "INSERT INTO _call_delta_marker VALUES (1, 1, zeroblob(32), -1, 0, zeroblob(32), zeroblob(32))",
        ).is_err(), "negative marker generation was accepted");
        assert!(db.exec(
            "INSERT INTO _call_owner VALUES (2, 99, 1, 1, 1, 0, zeroblob(32))",
        ).is_err(), "unknown StringId foreign key was accepted");
    }

    /// Differential rail for the family-derive engine (plan
    /// 2026-07-15-family-derive-reactive-engine, step 1). The `CallSite`
    /// family must produce the same `call_site` tuples as the legacy SQL
    /// projection (`reproject_sqlite_call_affected_keys`' call_site block),
    /// and must capture a dep on every owned row it read.
    #[test]
    fn family_call_site_matches_legacy_projection() {
        use crate::engine::family::{derive_family, CallSite};
        let db = delta_test_db(false);

        // Legacy projection: `delta_test_db` populates `rel_call_site` from
        // `_call_raw_site JOIN _call_owner` during setup.
        let legacy: Vec<[i64; 5]> = {
            let mut s = db.prepare(
                "SELECT repo, caller, callee, file, line FROM rel_call_site \
                 ORDER BY repo, caller, callee, file, line",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };

        // Engine path: derive the same relation from the owned tables.
        let (rows, deps) = derive_family(&db, &CallSite).unwrap();
        let ival = |v: &Value| match v { Value::Int(n) => *n, _ => 0 };
        let mut fam: Vec<[i64; 5]> = rows.iter().map(|r| {
            [ival(&r[0]), ival(&r[1]), ival(&r[2]), ival(&r[3]), ival(&r[4])]
        }).collect();
        fam.sort();

        assert_eq!(fam, legacy, "CallSite family diverged from legacy call_site projection");
        assert!(!fam.is_empty(), "family emitted no rows (rail is trivial)");

        // Dep capture: the family read both owned tables, by stable PK.
        let read_raw = deps.iter().any(|d| d.rel == "_call_raw_site");
        let read_owner = deps.iter().any(|d| d.rel == "_call_owner");
        assert!(read_raw, "family did not capture a dep on _call_raw_site");
        assert!(read_owner, "family did not capture a dep on _call_owner");
        // delta_test_db(false) seeds 2 owners + 2 sites.
        assert_eq!(deps.len(), 4, "expected 2 owner + 2 site deps, got {deps:?}");
    }

    /// Differential rail for the aggregation tier (plan step 2). `CallEdge`
    /// computes support (`GROUP BY caller, callee, kind, rev` with `COUNT`)
    /// in Rust, then emits `call_edge` as the distinct projection. Must match
    /// the legacy SQL population of `rel_call_edge`, and must capture a dep on
    /// the resolution table — the input the aggregation is over.
    #[test]
    fn family_call_edge_matches_legacy_projection() {
        use crate::engine::family::{derive_family, CallEdge};
        let db = delta_test_db(false);

        let legacy: Vec<[i64; 3]> = {
            let mut s = db.prepare(
                "SELECT caller, callee, kind FROM rel_call_edge \
                 ORDER BY caller, callee, kind",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };

        let (rows, deps) = derive_family(&db, &CallEdge).unwrap();
        let ival = |v: &Value| match v { Value::Int(n) => *n, _ => 0 };
        let mut fam: Vec<[i64; 3]> = rows.iter().map(|r| [ival(&r[0]), ival(&r[1]), ival(&r[2])]).collect();
        fam.sort();

        assert_eq!(fam, legacy, "CallEdge family diverged from legacy call_edge projection");
        assert!(!fam.is_empty(), "family emitted no rows (rail is trivial)");

        // Aggregation proof: the family read the resolution table it counts
        // over, plus the two join inputs.
        assert!(deps.iter().any(|d| d.rel == "_call_resolution"), "no dep on _call_resolution");
        assert!(deps.iter().any(|d| d.rel == "_call_raw_site"), "no dep on _call_raw_site");
        assert!(deps.iter().any(|d| d.rel == "_call_owner"), "no dep on _call_owner");
    }

    /// Selectivity rail (plan Verification): the family engine reacts to a real
    /// delta and matches the expected post-delta edge set, AND the deps
    /// captured before the delta flag the retracted input rows — the
    /// affected-set computation the engine would use to scope a rederive.
    /// This is the "reactive" proof the static rails above do not show.
    ///
    /// The cold (pre-delta) comparison still diffs against `rel_call_edge`
    /// (the fixture's one-time legacy snapshot, `delta_test_db`). The
    /// post-delta comparison CANNOT: `CallStore::apply_call_owner_delta` no
    /// longer writes `rel_call_edge` at all (P4, capstone cutover — that is
    /// the family router's job, one layer up, not exercised by this
    /// storage-only test), so the table would just be reasserting its own
    /// stale seed. Explicit expected values replace the legacy-table oracle
    /// there (plan Risk 4: "parity tests lose oracle").
    #[test]
    fn family_call_edge_rederives_after_delta_via_dep_capture() {
        use crate::engine::family::{derive_family, CallEdge, DepKey};
        let db = delta_test_db(false);
        let ival = |v: &Value| match v { Value::Int(n) => *n, _ => 0 };
        let snap = |db: &Db| -> Vec<[i64; 3]> {
            let mut s = db.prepare(
                "SELECT caller, callee, kind FROM rel_call_edge ORDER BY caller, callee, kind",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let sort3 = |rows: &[Vec<Value>]| -> Vec<[i64; 3]> {
            let mut v: Vec<[i64; 3]> = rows.iter()
                .map(|r| [ival(&r[0]), ival(&r[1]), ival(&r[2])]).collect();
            v.sort();
            v
        };

        // cold: derive edge + capture deps; matches legacy before any change.
        let (cold_rows, cold_deps) = derive_family(&db, &CallEdge).unwrap();
        assert_eq!(sort3(&cold_rows), snap(&db), "cold family diverged from legacy");

        // apply a real delta: owner a.rs switches its call target -> other.
        // The other call-family tests prove this delta is Accepted.
        CallStore::apply_call_owner_delta(&db, delta_for("other", [1; 32], [0; 32])).unwrap();

        // rederive the family against the now-updated _call_* tables. Expected
        // post-delta edges, computed directly (not from the now-stale
        // `rel_call_edge` fixture table): a.rs's site now resolves repo::run ->
        // repo::other, b.rs's untouched site still resolves repo::run ->
        // repo::target; both resolutions carry kind "call" (see
        // `insert_sqlite_call_delta_sites`).
        let (hot_rows, _hot_deps) = derive_family(&db, &CallEdge).unwrap();
        let hot = sort3(&hot_rows);
        let mut expected_hot = vec![
            [StringId::of("repo::run").sqlite(), StringId::of("repo::other").sqlite(), StringId::of("call").sqlite()],
            [StringId::of("repo::run").sqlite(), StringId::of("repo::target").sqlite(), StringId::of("call").sqlite()],
        ];
        expected_hot.sort();
        assert_eq!(hot, expected_hot, "family diverged from expected post-delta edges");

        // the family REACTED: support for repo::target dropped (a.rs left),
        // and a new repo::other edge appeared. cold was 1 edge, hot is 2.
        assert_ne!(hot, sort3(&cold_rows), "family did not react to the delta");
        assert_eq!(hot.len(), 2, "expected 2 post-delta edges (target via b.rs + other via a.rs)");

        // SELECTIVITY: the cold deps include _call_raw_site rows the delta
        // retracted. The engine's affected-set = captured deps whose row is
        // gone (or new rows not captured). Find the retracted site ids.
        let cold_site_pks: Vec<i64> = cold_deps.iter()
            .filter(|d| d.rel == "_call_raw_site").map(|d: &DepKey| d.pk).collect();
        assert_eq!(cold_site_pks.len(), 2, "cold derive should have read 2 sites");
        let present: i64 = {
            let qs = cold_site_pks.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!("SELECT COUNT(*) FROM _call_raw_site WHERE site_id IN ({qs})");
            db.query_row(&sql, rusqlite::params_from_iter(cold_site_pks.iter()), |r| r.get(0))
                .unwrap()
        };
        // a.rs's old site was retracted by the delta; only b.rs's site remains.
        assert_eq!(present, 1, "dep-capture did not flag the retracted row");
    }

    /// Flip rail (plan step 3): the router flip wipes + re-derives every
    /// public call rel from the owned `_call_*` tables. The result must be
    /// identical to the legacy SQL population fixture seeds. This is the
    /// direct evidence the family path is a correct live producer — the
    /// property P4 (capstone cutover) relies on making it the SOLE one.
    #[test]
    fn family_flip_overwrites_legacy_call_rels_identically() {
        use crate::engine::family::router::FamilyRouter;
        use crate::engine::family::call_families;
        use crate::lower::tbl;
        use crate::storage::Storage;
        let db = delta_test_db(false);

        let legacy_site: Vec<[i64; 5]> = {
            let mut s = db.prepare(
                "SELECT repo, caller, callee, file, line FROM rel_call_site \
                 ORDER BY repo, caller, callee, file, line",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let legacy_edge: Vec<[i64; 3]> = {
            let mut s = db.prepare(
                "SELECT caller, callee, kind FROM rel_call_edge ORDER BY caller, callee, kind",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let legacy_kind: Vec<[i64; 2]> = {
            let mut s = db.prepare(
                "SELECT fn, kind FROM rel_call_kind ORDER BY fn, kind",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let legacy_edge_rev: Vec<[i64; 4]> = {
            let mut s = db.prepare(
                "SELECT caller, callee, kind, rev FROM rel_call_edge_rev \
                 ORDER BY caller, callee, kind, rev",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let legacy_def_rev: Vec<[i64; 7]> = {
            let mut s = db.prepare(
                "SELECT repo, sym, kind, file, line, \"end\", rev FROM rel_call_def_rev \
                 ORDER BY repo, sym, kind, file, line, \"end\", rev",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let legacy_def: Vec<[i64; 6]> = {
            let mut s = db.prepare(
                "SELECT repo, sym, kind, file, line, \"end\" FROM rel_call_def \
                 ORDER BY repo, sym, kind, file, line, \"end\"",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert!(!legacy_site.is_empty(), "fixture precondition: legacy call_site populated");
        assert!(!legacy_edge.is_empty(), "fixture precondition: legacy call_edge populated");
        assert!(!legacy_kind.is_empty(), "fixture precondition: legacy call_kind populated");
        assert!(!legacy_edge_rev.is_empty(), "fixture precondition: legacy call_edge_rev populated");
        assert!(!legacy_def_rev.is_empty(), "fixture precondition: legacy call_def_rev populated");
        assert!(!legacy_def.is_empty(), "fixture precondition: legacy call_def populated");
        // The multi-rev fixture makes the collapse observable: one repo::target
        // def at two revs -> two rev rows, one collapsed def row.
        assert!(
            legacy_def_rev.len() > legacy_def.len(),
            "fixture precondition: a rev-collapsed def must exist ({} rev rows, {} def rows)",
            legacy_def_rev.len(), legacy_def.len(),
        );

        // flip: the router derives every hosted family, and we overwrite the
        // public rels from their rows — the same path flip_call_rels_via_router
        // runs live, minus the Engine (this rail owns a raw Db).
        let mut router = FamilyRouter::new(call_families());
        let rerun = router.cold(&db).unwrap();
        assert_eq!(
            rerun,
            vec![
                "call_def", "call_def_rev", "call_edge", "call_edge_rev",
                "call_kind", "call_name", "call_site",
            ],
            "cold derives every hosted family (registry order = sorted by name)",
        );
        db.reload_rel(
            &tbl("call_site"),
            &["repo", "caller", "callee", "file", "line"],
            router.rows("call_site").unwrap(),
        )
        .unwrap();
        db.reload_rel(&tbl("call_edge"), &["caller", "callee", "kind"], router.rows("call_edge").unwrap())
            .unwrap();
        db.reload_rel(&tbl("call_kind"), &["fn", "kind"], router.rows("call_kind").unwrap())
            .unwrap();
        db.reload_rel(
            &tbl("call_edge_rev"),
            &["caller", "callee", "kind", "rev"],
            router.rows("call_edge_rev").unwrap(),
        )
        .unwrap();
        db.reload_rel(
            &tbl("call_def_rev"),
            &["repo", "sym", "kind", "file", "line", "end", "rev"],
            router.rows("call_def_rev").unwrap(),
        )
        .unwrap();
        db.reload_rel(
            &tbl("call_def"),
            &["repo", "sym", "kind", "file", "line", "end"],
            router.rows("call_def").unwrap(),
        )
        .unwrap();

        let family_site: Vec<[i64; 5]> = {
            let mut s = db.prepare(
                "SELECT repo, caller, callee, file, line FROM rel_call_site \
                 ORDER BY repo, caller, callee, file, line",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let family_edge: Vec<[i64; 3]> = {
            let mut s = db.prepare(
                "SELECT caller, callee, kind FROM rel_call_edge ORDER BY caller, callee, kind",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };

        let family_kind: Vec<[i64; 2]> = {
            let mut s = db.prepare(
                "SELECT fn, kind FROM rel_call_kind ORDER BY fn, kind",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let family_edge_rev: Vec<[i64; 4]> = {
            let mut s = db.prepare(
                "SELECT caller, callee, kind, rev FROM rel_call_edge_rev \
                 ORDER BY caller, callee, kind, rev",
            ).unwrap();
            s.query_map([], |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?]))
                .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };

        let family_def_rev: Vec<[i64; 7]> = {
            let mut s = db.prepare(
                "SELECT repo, sym, kind, file, line, \"end\", rev FROM rel_call_def_rev \
                 ORDER BY repo, sym, kind, file, line, \"end\", rev",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        let family_def: Vec<[i64; 6]> = {
            let mut s = db.prepare(
                "SELECT repo, sym, kind, file, line, \"end\" FROM rel_call_def \
                 ORDER BY repo, sym, kind, file, line, \"end\"",
            ).unwrap();
            s.query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?])
            }).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };

        assert_eq!(family_site, legacy_site, "flipped rel_call_site diverged from legacy");
        assert_eq!(family_edge, legacy_edge, "flipped rel_call_edge diverged from legacy");
        assert_eq!(family_kind, legacy_kind, "flipped rel_call_kind diverged from legacy");
        assert_eq!(
            family_edge_rev, legacy_edge_rev,
            "flipped rel_call_edge_rev diverged from legacy",
        );
        assert_eq!(family_def_rev, legacy_def_rev, "flipped rel_call_def_rev diverged from legacy");
        assert_eq!(family_def, legacy_def, "flipped rel_call_def diverged from legacy");
    }

    // ---- reactive router rails -------------------------------------------
    // The router memoizes each family's rows + rel footprint and reruns only
    // families whose inputs a delta touched. These prove the SKIP decision
    // (the piece the step-2 session left unbuilt), not just output-reactivity.

    fn ival(v: &Value) -> i64 {
        match v {
            Value::Int(n) => *n,
            _ => 0,
        }
    }
    fn sorted3(rows: &[Vec<Value>]) -> Vec<[i64; 3]> {
        let mut v: Vec<[i64; 3]> =
            rows.iter().map(|r| [ival(&r[0]), ival(&r[1]), ival(&r[2])]).collect();
        v.sort();
        v
    }
    fn sorted5(rows: &[Vec<Value>]) -> Vec<[i64; 5]> {
        let mut v: Vec<[i64; 5]> = rows
            .iter()
            .map(|r| [ival(&r[0]), ival(&r[1]), ival(&r[2]), ival(&r[3]), ival(&r[4])])
            .collect();
        v.sort();
        v
    }

    fn snap_site(db: &Db) -> Vec<[i64; 5]> {
        let mut s = db
            .prepare("SELECT repo, caller, callee, file, line FROM rel_call_site")
            .unwrap();
        let mut v: Vec<[i64; 5]> = s
            .query_map([], |row| {
                Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?])
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        v.sort();
        v
    }

    /// The discriminating test: a `_call_resolution`-only change reruns
    /// `CallEdge` (which scans it) and SKIPS `CallSite` (which does not).
    /// `CallSite` keeps its memoized rows; `CallEdge`'s rerun rows match a
    /// fresh derive against the mutated tables.
    #[test]
    fn family_router_skips_unaffected_family_on_resolution_only_change() {
        use crate::engine::family::router::FamilyRouter;
        use crate::engine::family::{derive_family, CallEdge, CallSite};

        let db = delta_test_db(false);
        let call_site = CallSite;
        let call_edge = CallEdge;
        let mut router = FamilyRouter::new(vec![&call_site, &call_edge]);

        let cold = router.cold(&db).unwrap();
        assert_eq!(cold, vec!["call_site", "call_edge"], "cold should derive both, in order");
        let site_cold = sorted5(router.rows("call_site").unwrap());
        let edge_cold = sorted3(router.rows("call_edge").unwrap());
        assert!(!site_cold.is_empty(), "fixture: call_site non-empty");
        assert_eq!(edge_cold.len(), 1, "fixture: one distinct edge (both sites -> target)");

        // resolution-only change: drop every resolution. call_edge collapses to
        // empty; call_site is untouched (it never reads _call_resolution).
        db.conn().execute("DELETE FROM _call_resolution", []).unwrap();

        let mut changed = std::collections::HashSet::new();
        changed.insert("_call_resolution");
        let rerun = router.react(&db, &changed).unwrap();

        assert_eq!(rerun, vec!["call_edge"], "only CallEdge reads _call_resolution");
        assert_eq!(
            sorted5(router.rows("call_site").unwrap()),
            site_cold,
            "CallSite was skipped; its memoized rows must be unchanged",
        );
        let edge_hot = sorted3(router.rows("call_edge").unwrap());
        assert!(edge_hot.is_empty(), "CallEdge reran: all resolutions gone -> no edges");
        assert_ne!(edge_hot, edge_cold, "CallEdge output must reflect the change");

        // consistency: the router's rerun rows equal a fresh full derive.
        let (fresh_edge, _) = derive_family(&db, &CallEdge).unwrap();
        assert_eq!(edge_hot, sorted3(&fresh_edge), "memoized rerun diverged from fresh derive");
    }

    /// A change to a SHARED input relation (`_call_owner`, read by both
    /// families) reruns both.
    #[test]
    fn family_router_reruns_all_on_shared_input_change() {
        use crate::engine::family::router::FamilyRouter;
        use crate::engine::family::{CallEdge, CallSite};

        let db = delta_test_db(false);
        let call_site = CallSite;
        let call_edge = CallEdge;
        let mut router = FamilyRouter::new(vec![&call_site, &call_edge]);
        router.cold(&db).unwrap();

        let mut changed = std::collections::HashSet::new();
        changed.insert("_call_owner");
        let rerun = router.react(&db, &changed).unwrap();
        assert_eq!(rerun, vec!["call_site", "call_edge"], "both families scan _call_owner");
    }

    /// A change touching NO family's inputs reruns nothing — every family keeps
    /// its memo.
    #[test]
    fn family_router_reruns_none_on_untouched_input() {
        use crate::engine::family::router::FamilyRouter;
        use crate::engine::family::{CallEdge, CallSite};

        let db = delta_test_db(false);
        let call_site = CallSite;
        let call_edge = CallEdge;
        let mut router = FamilyRouter::new(vec![&call_site, &call_edge]);
        router.cold(&db).unwrap();

        let mut changed = std::collections::HashSet::new();
        changed.insert("_some_other_table");
        let rerun = router.react(&db, &changed).unwrap();
        assert!(rerun.is_empty(), "no family reads _some_other_table; nothing reruns");
    }

    /// Incremental RETRACTION through the reconcile/render path: removing an
    /// input site must surface as a RETRACTED output row in `call_site`'s
    /// RowDelta (not a silent full rebuild), and applying that delta row-level
    /// (retract + insert) must leave rel_call_site matching a fresh derive.
    #[test]
    fn family_router_retracts_output_row_on_input_retraction() {
        use crate::engine::family::router::FamilyRouter;
        use crate::engine::family::{call_families, derive_family, CallSite};
        use crate::lower::tbl;
        use crate::storage::Storage;

        let db = delta_test_db(false);
        let site_cols = ["repo", "caller", "callee", "file", "line"];
        let call_site = CallSite;
        let mut router = FamilyRouter::new(call_families());
        router.cold(&db).unwrap();

        // Establish the table from the cold memo (base state = 2 site rows).
        db.reload_rel(&tbl("call_site"), &site_cols, router.rows("call_site").unwrap())
            .unwrap();
        let cold_rows = snap_site(&db);
        assert_eq!(cold_rows.len(), 2, "fixture: two call sites");

        // Retract the a.rs site (line 10): drop its resolution then its raw site.
        db.conn()
            .execute(
                "DELETE FROM _call_resolution WHERE site_id IN \
                 (SELECT site_id FROM _call_raw_site WHERE line = 10)",
                [],
            )
            .unwrap();
        db.conn().execute("DELETE FROM _call_raw_site WHERE line = 10", []).unwrap();

        // React + reconcile: call_site's delta must carry exactly one retracted
        // row (the a.rs projection) and no inserts.
        let mut changed = std::collections::HashSet::new();
        changed.insert("_call_raw_site");
        changed.insert("_call_resolution");
        let deltas = router.react_deltas(&db, &changed).unwrap();
        let (_, site_delta) = deltas
            .iter()
            .find(|(name, _)| *name == "call_site")
            .expect("call_site reran");
        assert_eq!(site_delta.retracted.len(), 1, "exactly one output row retracted");
        assert!(site_delta.inserted.is_empty(), "no inserts on a pure retraction");

        // Render the delta incrementally (retract + insert), NOT a full reload.
        db.retract_rows(&tbl("call_site"), &site_cols, &site_delta.retracted).unwrap();
        db.insert_rows(&tbl("call_site"), &site_cols, &site_delta.inserted).unwrap();

        // The incrementally-maintained table equals a fresh full derive.
        let after = snap_site(&db);
        assert_eq!(after.len(), 1, "one site row remains after retraction");
        let (fresh, _) = derive_family(&db, &CallSite).unwrap();
        assert_eq!(after, sorted5(&fresh), "incremental render diverged from fresh derive");
    }

    /// A family with no memo yet (never cold-loaded) always derives on the
    /// first `react`, regardless of the changed set.
    #[test]
    fn family_router_derives_unmemoized_family() {
        use crate::engine::family::router::FamilyRouter;
        use crate::engine::family::CallSite;

        let db = delta_test_db(false);
        let call_site = CallSite;
        let mut router = FamilyRouter::new(vec![&call_site]);

        // no cold(): the memo is empty, so react must derive it even though the
        // changed set names nothing CallSite reads.
        let mut changed = std::collections::HashSet::new();
        changed.insert("_unrelated");
        let rerun = router.react(&db, &changed).unwrap();
        assert_eq!(rerun, vec!["call_site"], "unmemoized family derives on first react");
        assert!(!router.rows("call_site").unwrap().is_empty(), "derive populated rows");
    }
}
