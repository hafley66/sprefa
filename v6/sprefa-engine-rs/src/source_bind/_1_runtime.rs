use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{params, Connection};
use serde_json::json;
use sprefa_rust_runtime_host::{
    ClockedSourceHostRequest, ReadRequestWire, SourceHost, SourceHostDemand, SourceHostEnvelope,
    SourceHostOutcome, SourceHostSuccess,
};

use crate::{
    program::GenProgram,
    sql::SqliteSeam,
    types::{Arrival, ArrivalSign, Value},
};

use super::{
    source_row, GitSourceRegistration, SourceBindError, SourceBindFrame, SourceBindRelations,
    SourceBindTickFrame, SourceInputs,
};

/// Long-lived filesystem, Git, identity-store, and extraction state for one
/// engine runtime. Receipt rows live beside the source host's identity store:
/// a fresh process can reconstruct each authored deletion without carrying
/// source structs or extracted facts in process memory.
pub struct SourceBind {
    host: SourceHost,
    relations: SourceBindRelations,
    inputs: SourceInputs,
    roots: BTreeMap<soopy::RepositoryId, String>,
    receipts: ReceiptStore,
}

impl SourceBind {
    pub fn in_memory(relations: SourceBindRelations) -> anyhow::Result<Self> {
        Ok(Self::with_host(
            SourceHost::in_memory()?,
            relations,
            ReceiptStore::in_memory()?,
        ))
    }

    pub fn open(
        database: impl AsRef<Path>,
        relations: SourceBindRelations,
    ) -> anyhow::Result<Self> {
        let database = database.as_ref();
        Ok(Self::with_host(
            SourceHost::open(database)?,
            relations,
            ReceiptStore::open(database)?,
        ))
    }

    fn with_host(host: SourceHost, relations: SourceBindRelations, receipts: ReceiptStore) -> Self {
        Self {
            host,
            relations,
            inputs: SourceInputs::default(),
            roots: BTreeMap::new(),
            receipts,
        }
    }

    pub fn register_root(&mut self, root: impl AsRef<Path>) -> anyhow::Result<soopy::RepositoryId> {
        let registration = self.inputs.register_git(root.as_ref())?;
        let id = registration.repository;
        self.roots
            .insert(id.clone(), root.as_ref().to_string_lossy().to_string());
        self.host.register_root(root)?;
        Ok(id)
    }

    pub fn register_directory(
        &mut self,
        root: impl AsRef<Path>,
    ) -> anyhow::Result<soopy::DirectoryId> {
        self.inputs.register_directory(root)
    }

    pub fn register_git(
        &mut self,
        root: impl AsRef<Path>,
    ) -> anyhow::Result<GitSourceRegistration> {
        let registration = self.inputs.register_git(root.as_ref())?;
        self.roots.insert(
            registration.repository.clone(),
            root.as_ref().to_string_lossy().to_string(),
        );
        self.host.register_root(root)?;
        Ok(registration)
    }

    pub fn directory_snapshot(
        &mut self,
        directory: &soopy::DirectoryId,
        query: &soopy::FileQuery,
    ) -> anyhow::Result<soopy::FileSnapshot> {
        self.inputs.directory_snapshot(directory, query)
    }

    pub fn tracked_state(
        &mut self,
        worktree: &soopy::WorktreeId,
        query: &soopy::GitFileQuery,
    ) -> anyhow::Result<soopy::TrackedStateResult> {
        self.inputs.tracked_state(worktree, query)
    }

    pub fn relations(&self) -> &SourceBindRelations {
        &self.relations
    }
    pub fn host(&self) -> &SourceHost {
        &self.host
    }

    /// Execute Soopy source mechanics, then project identity changes to the
    /// same nested source structs that authored DL6 names. Extraction happens
    /// against the already-requested source bytes in-process.
    pub fn execute(
        &mut self,
        request: ClockedSourceHostRequest,
    ) -> Result<SourceBindFrame, SourceBindError> {
        let reads = match &request.demand {
            SourceHostDemand::Identity(demand) => demand.reads.clone(),
            _ => Vec::new(),
        };
        let extracted = self
            .extract_many(&reads)
            .map_err(|error| SourceBindError::SourceRead {
                message: error.to_string(),
            })?;
        let envelope: SourceHostEnvelope = self.host.execute(request.clone());
        let arrivals = self.project(&request, &envelope, &extracted)?;
        Ok(SourceBindFrame {
            clock: request.clock,
            envelope,
            arrivals,
        })
    }

    /// Resolve one typed source demand and apply its signed relation rows as
    /// the input batch for one engine tick.
    pub fn run_tick(
        &mut self,
        program: &GenProgram,
        seam: &SqliteSeam,
        request: ClockedSourceHostRequest,
    ) -> anyhow::Result<SourceBindTickFrame> {
        self.validate_program(program)?;
        let source = self.execute(request)?;
        let deltas = program.run_tick(seam, &source.arrivals)?;
        Ok(SourceBindTickFrame { source, deltas })
    }

    pub fn validate_program(&self, program: &GenProgram) -> anyhow::Result<()> {
        for declaration in self.relations.arrival_declarations() {
            let columns = program.rel_columns.get(&declaration.name).ok_or_else(|| {
                anyhow::anyhow!("source bind relation {} is missing", declaration.name)
            })?;
            let expected_columns = declaration
                .columns
                .iter()
                .map(|column| (*column).to_string())
                .collect::<Vec<_>>();
            anyhow::ensure!(
                columns == &expected_columns,
                "source bind relation {} columns are {:?}, expected {:?}",
                declaration.name,
                columns,
                expected_columns
            );
            let types = program
                .rel_column_types
                .get(&declaration.name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "source bind relation {} types are missing",
                        declaration.name
                    )
                })?;
            anyhow::ensure!(
                types == declaration.column_types,
                "source bind relation {} types are {:?}, expected {:?}",
                declaration.name,
                types,
                declaration.column_types
            );
            let relation = program
                .relations
                .iter()
                .find(|relation| relation.rel == declaration.name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "source bind relation {} has no runtime plan",
                        declaration.name
                    )
                })?;
            anyhow::ensure!(
                relation.arrival_add_sql.is_some() && relation.arrival_del_sql.is_some(),
                "source bind relation {} does not accept signed arrivals",
                declaration.name
            );
        }
        Ok(())
    }

    /// Soopy owns the crawl read loop and hands each file through one reusable
    /// buffer. This retains only FlatFact-derived relation rows, never corpus
    /// bytes, while the identity host separately persists its required source
    /// evidence.
    fn extract_many(
        &mut self,
        reads: &[ReadRequestWire],
    ) -> anyhow::Result<BTreeMap<soopy::SourceRef, Vec<Arrival>>> {
        let requests = reads
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<soopy::ReadRequest>>();
        let relations = self.relations.clone();
        let roots = self.roots.clone();
        let mut extracted = BTreeMap::new();
        let mut buffer = Vec::new();
        self.inputs.read_each(&requests, &mut buffer, |result| {
            extracted.insert(
                result.source.clone(),
                extract_specifiers(
                    &relations,
                    result.source,
                    result.content,
                    result.bytes,
                    &roots,
                ),
            );
            Ok(())
        })?;
        Ok(extracted)
    }

    fn project(
        &mut self,
        request: &ClockedSourceHostRequest,
        envelope: &SourceHostEnvelope,
        extracted: &BTreeMap<soopy::SourceRef, Vec<Arrival>>,
    ) -> Result<Vec<Arrival>, SourceBindError> {
        if request.clock != envelope.clock {
            return Ok(Vec::new());
        }
        let SourceHostOutcome::Success(SourceHostSuccess::Identity(result)) = &envelope.outcome
        else {
            return Ok(Vec::new());
        };
        let mut arrivals = Vec::new();
        for retired in result
            .worktree_replacements
            .iter()
            .map(|change| &change.retired)
            .chain(result.retired.iter())
        {
            arrivals.extend(
                self.receipts
                    .take(retired.rev_file_id.0)?
                    .into_iter()
                    .map(|arrival| source_row(arrival.rel, ArrivalSign::Del, arrival.row)),
            );
        }
        for mapped in &result.added_sources {
            let mut authored = vec![source_row(
                self.relations.file.clone(),
                ArrivalSign::Add,
                file_values(&mapped.source, &mapped.content, &self.roots),
            )];
            if let Some(extracted) = extracted.get(&mapped.source) {
                authored.extend(extracted.iter().cloned());
            }
            self.receipts.append(mapped.rev_file_id.0, &authored)?;
            arrivals.extend(authored);
        }
        // Spans collect per source identity first: one replacement writes one
        // receipt batch however many spans it carries.
        let mut span_batches: BTreeMap<i64, Vec<Arrival>> = BTreeMap::new();
        for mapped in &result.added_spans {
            if let Some(source) = result
                .sources
                .iter()
                .find(|source| source.source == mapped.span.source)
            {
                span_batches
                    .entry(source.rev_file_id.0)
                    .or_default()
                    .push(source_row(
                        self.relations.span.clone(),
                        ArrivalSign::Add,
                        span_values(&mapped.span, &source.content, &self.roots),
                    ));
            }
        }
        for (rev_file_id, spans) in span_batches {
            self.receipts.append(rev_file_id, &spans)?;
            arrivals.extend(spans);
        }
        Ok(arrivals)
    }
}

/// Durable authored-arrival receipt keyed by the source host's dense
/// `rev_file_id`. `ordinal` preserves the original addition order; `take`
/// reads it before clearing it, so every replacement projects deletion rows
/// before its new additions. `(rev_file_id, ordinal)` is the only uniqueness
/// key: one source identity owns one ordered set of authored input rows.
struct ReceiptStore {
    conn: Connection,
}

impl ReceiptStore {
    fn in_memory() -> anyhow::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        // The identity store owns its own connection to this same file, so a
        // receipt write can meet an identity write mid-statement.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> anyhow::Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _source_bind_receipt_v1 (
                rev_file_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                rel TEXT NOT NULL,
                row_json TEXT NOT NULL,
                PRIMARY KEY (rev_file_id, ordinal)
            ) WITHOUT ROWID",
        )?;
        Ok(Self { conn })
    }

    // One transaction, one ordinal read, one prepared insert for the whole
    // batch. An INSERT whose own ordinal came from a subquery over this table
    // read its insert target, which costs a transient ephemeral write per row
    // (.claude/skills/sqlite-costs), and a per-row implicit commit fsyncs the
    // identity store once per authored fact.
    fn append(&mut self, rev_file_id: i64, arrivals: &[Arrival]) -> Result<(), SourceBindError> {
        if arrivals.is_empty() {
            return Ok(());
        }
        let transaction = self.conn.transaction().map_err(receipt_error)?;
        {
            let first_ordinal: i64 = transaction
                .query_row(
                    "SELECT COALESCE(max(ordinal) + 1, 0) FROM _source_bind_receipt_v1
                     WHERE rev_file_id = ?",
                    [rev_file_id],
                    |row| row.get(0),
                )
                .map_err(receipt_error)?;
            let mut insert = transaction
                .prepare_cached(
                    "INSERT INTO _source_bind_receipt_v1 (rev_file_id, ordinal, rel, row_json)
                     VALUES (?, ?, ?, ?)",
                )
                .map_err(receipt_error)?;
            for (ordinal, arrival) in (first_ordinal..).zip(arrivals) {
                let row_json = serde_json::to_string(&arrival.row).map_err(|error| {
                    SourceBindError::Receipt {
                        message: error.to_string(),
                    }
                })?;
                insert
                    .execute(params![rev_file_id, ordinal, arrival.rel, row_json])
                    .map_err(receipt_error)?;
            }
        }
        transaction.commit().map_err(receipt_error)
    }

    fn take(&mut self, rev_file_id: i64) -> Result<Vec<Arrival>, SourceBindError> {
        let transaction = self.conn.transaction().map_err(receipt_error)?;
        let rows = {
            let mut statement = transaction
                .prepare(
                    "SELECT rel, row_json FROM _source_bind_receipt_v1
                     WHERE rev_file_id = ? ORDER BY ordinal",
                )
                .map_err(receipt_error)?;
            let rows = statement
                .query_map([rev_file_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(receipt_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(receipt_error)?;
            rows
        };
        transaction
            .execute(
                "DELETE FROM _source_bind_receipt_v1 WHERE rev_file_id = ?",
                [rev_file_id],
            )
            .map_err(receipt_error)?;
        transaction.commit().map_err(receipt_error)?;
        rows.into_iter()
            .map(|(rel, row_json)| {
                serde_json::from_str(&row_json)
                    .map(|row| Arrival {
                        rel,
                        sign: ArrivalSign::Add,
                        row,
                    })
                    .map_err(|error| SourceBindError::Receipt {
                        message: error.to_string(),
                    })
            })
            .collect()
    }
}

fn receipt_error(error: rusqlite::Error) -> SourceBindError {
    SourceBindError::Receipt {
        message: error.to_string(),
    }
}

fn repo_value(
    source: &soopy::SourceRef,
    roots: &BTreeMap<soopy::RepositoryId, String>,
) -> serde_json::Value {
    json!({"root": roots.get(&source.repository).cloned().unwrap_or_else(|| source.repository.0.to_string())})
}

fn revision_oid(revision: &soopy::RevisionId) -> String {
    match revision {
        soopy::RevisionId::Commit(oid) => oid.0.to_string(),
        soopy::RevisionId::Worktree {
            worktree,
            head,
            dirty,
        } => format!(
            "worktree:{}:{}:{}",
            worktree.0,
            head.as_ref().map(|oid| oid.0.as_ref()).unwrap_or(""),
            dirty
        ),
    }
}

fn file_value(
    source: &soopy::SourceRef,
    content: &soopy::ContentId,
    roots: &BTreeMap<soopy::RepositoryId, String>,
) -> serde_json::Value {
    json!({
        "rev": {"repo": repo_value(source, roots), "oid": revision_oid(&source.revision)},
        "path": source.path.0.as_ref(),
        "blob": {"oid": content.to_string()},
    })
}

fn file_values(
    source: &soopy::SourceRef,
    content: &soopy::ContentId,
    roots: &BTreeMap<soopy::RepositoryId, String>,
) -> Vec<Value> {
    let file = file_value(source, content, roots);
    vec![
        Value::Text(file["rev"].to_string()),
        Value::Text(source.path.0.to_string()),
        Value::Text(file["blob"].to_string()),
    ]
}

fn span_values(
    span: &soopy::SourceSpan,
    content: &soopy::ContentId,
    roots: &BTreeMap<soopy::RepositoryId, String>,
) -> Vec<Value> {
    vec![
        Value::Text(file_value(&span.source, content, roots).to_string()),
        Value::Integer(span.start as i64),
        Value::Integer(span.end as i64),
    ]
}

fn extract_specifiers(
    relations: &SourceBindRelations,
    source: &soopy::SourceRef,
    content: &soopy::ContentId,
    bytes: &[u8],
    roots: &BTreeMap<soopy::RepositoryId, String>,
) -> Vec<Arrival> {
    let Some(output) = sprefa_extract::dispatch(
        source.path.0.as_ref(),
        bytes,
        sprefa_extract::FamilyMask::ALL,
    ) else {
        return Vec::new();
    };
    sprefa_extract::flatten(&output).into_iter().filter_map(|fact| match fact {
        sprefa_extract::FlatFact::Specifier { span, name, kind, module, .. } => {
            let owner = json!({"file": file_value(source, content, roots), "start_byte": span.start, "end_byte": span.end});
            Some(vec![
                source_row(
                    relations.span.clone(),
                    ArrivalSign::Add,
                    vec![
                        Value::Text(owner["file"].to_string()),
                        Value::Integer(i64::from(span.start)),
                        Value::Integer(i64::from(span.end)),
                    ],
                ),
                source_row(
                    relations.specifier.clone(),
                    ArrivalSign::Add,
                    vec![
                        Value::Text(owner.to_string()),
                        Value::Text(module.unwrap_or_else(|| name.clone())),
                        Value::Text(name),
                        Value::Text(kind),
                    ],
                ),
            ])
        }
        _ => None,
    }).flatten().collect()
}
