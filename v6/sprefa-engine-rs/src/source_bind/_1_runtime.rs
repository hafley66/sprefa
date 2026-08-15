use std::collections::BTreeMap;
use std::path::Path;

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
    source_row, DenseSourceCache, DenseSpanCache, GitSourceRegistration, SourceBindError,
    SourceBindFrame, SourceBindRelations, SourceBindTickFrame, SourceInputs,
};

/// Long-lived filesystem, Git, identity-store, and extraction state for one
/// engine runtime. The identity store's dense keys only index the two caches
/// needed to turn later deletion receipts back into authored source values.
pub struct SourceBind {
    host: SourceHost,
    relations: SourceBindRelations,
    inputs: SourceInputs,
    roots: BTreeMap<soopy::RepositoryId, String>,
    sources: DenseSourceCache,
    contents: BTreeMap<i64, soopy::ContentId>,
    spans: DenseSpanCache,
    extracted: BTreeMap<i64, Vec<Arrival>>,
}

impl SourceBind {
    pub fn in_memory(relations: SourceBindRelations) -> anyhow::Result<Self> {
        Ok(Self::with_host(SourceHost::in_memory()?, relations))
    }

    pub fn open(
        database: impl AsRef<Path>,
        relations: SourceBindRelations,
    ) -> anyhow::Result<Self> {
        Ok(Self::with_host(SourceHost::open(database)?, relations))
    }

    fn with_host(host: SourceHost, relations: SourceBindRelations) -> Self {
        Self {
            host,
            relations,
            inputs: SourceInputs::default(),
            roots: BTreeMap::new(),
            sources: BTreeMap::new(),
            contents: BTreeMap::new(),
            spans: BTreeMap::new(),
            extracted: BTreeMap::new(),
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
            let content = self.contents.remove(&retired.rev_file_id.0);
            if let Some(source) = self.sources.remove(&retired.rev_file_id.0) {
                if let Some(content) = content.as_ref() {
                    arrivals.push(source_row(
                        self.relations.file.clone(),
                        ArrivalSign::Del,
                        file_values(&source, content, &self.roots),
                    ));
                }
            }
            if let Some(extracted) = self.extracted.remove(&retired.rev_file_id.0) {
                arrivals.extend(
                    extracted
                        .into_iter()
                        .map(|arrival| source_row(arrival.rel, ArrivalSign::Del, arrival.row)),
                );
            }
            for id in &retired.file_span_ids {
                if let Some(span) = self.spans.remove(&id.0) {
                    if let Some(content) = content.as_ref() {
                        arrivals.push(source_row(
                            self.relations.span.clone(),
                            ArrivalSign::Del,
                            span_values(&span, content, &self.roots),
                        ));
                    }
                }
            }
        }
        for mapped in &result.added_sources {
            self.sources
                .insert(mapped.rev_file_id.0, mapped.source.clone());
            self.contents
                .insert(mapped.rev_file_id.0, mapped.content.clone());
            arrivals.push(source_row(
                self.relations.file.clone(),
                ArrivalSign::Add,
                file_values(&mapped.source, &mapped.content, &self.roots),
            ));
            if let Some(extracted) = extracted.get(&mapped.source) {
                let extracted = extracted.clone();
                self.extracted
                    .insert(mapped.rev_file_id.0, extracted.clone());
                arrivals.extend(extracted);
            }
        }
        for mapped in &result.added_spans {
            self.spans
                .insert(mapped.file_span_id.0, mapped.span.clone());
            if let Some(content) = result
                .sources
                .iter()
                .find(|source| source.source == mapped.span.source)
                .map(|source| source.content.clone())
            {
                arrivals.push(source_row(
                    self.relations.span.clone(),
                    ArrivalSign::Add,
                    span_values(&mapped.span, &content, &self.roots),
                ));
            }
        }
        Ok(arrivals)
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
