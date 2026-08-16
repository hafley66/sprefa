//! sprefa-extract: a corpus at a version -> normalized graph facts. ONE sync leaf.
//!
//! Pure, SYNC, CPU-bound, arena-mastered. No database, no async, no reactor; the
//! async-eval flip + reactivity live in other crates (this iteration the
//! reactivity is an RxJS prototype that drives the CLI bin). The store sits
//! ABOVE this crate; extract never names a store id or a storage type (the
//! crate-map boundary rail).
//!
//! The lock and the build sequence live in
//! `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`; the canonical current
//! mind is `v6/sprefa-seed/src/_3_extract/_7_tasks.rs`.
//!
//! Commit 1 (this crate's first commit) is the PIPING PROOF: one Parser
//! (`AstGrepParser`, ast-grep grammars cover rust/ts/go) + `Project<CstF>` (the
//! lossless named-node tree) + the flat wire + a clap bin streaming JSONL +
//! `--bench`. Proves bin -> seams -> flat wire -> stdout end to end.
#![allow(dead_code)]

pub mod deps;
pub mod dispatch;
pub mod family;
pub mod lang;
pub mod project;
pub mod rows;
pub mod schema;
pub mod scip;
pub mod scip_decode;
pub mod scip_ensure;
pub mod scip_rows;
pub mod scip_v5_rels;
pub mod seams;
pub mod shape;
pub mod source;
pub mod types;
pub mod wire;

pub use deps::{resolve_specifier, Policy, TsconfigPaths};
pub use dispatch::dispatch;
pub use family::{
    flow_edges, CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, DfArg, DfEdgeKind, DfF,
    DfFAux, DfNodeKind, DfParam, DocFact, DocTag, Family, FlowEdge, FlowEdgeKind, FlowF,
    ProjectEdge, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind,
    TypeF, TypeFAux, TypeSig,
};
pub use lang::{
    query_patterns, source_for, sources, AstCaptureFact, AstPatternQuery, AstgrepSource, DlSource,
    GoSource, KotlinSource, MarkdownSource, PrologSource, PythonSource, RustSource, TsSource,
};
pub use project::{
    diet_scip, diet_scip_jsonl, resolve_project, resolve_project_jsonl, scip_facts,
    scip_facts_jsonl, scip_family, scip_family_jsonl, scip_file_edges_jsonl, scip_index_location,
    FsBlobSource, ProjectError, ResolveArm, ResolveArms, ResolveRequest, ScipFamilyRequest,
    ScipMode, SourceTreeBlobSource, RESOLVE_ARMS,
};
pub use soopy::{ContentId as SourceContentId, Pattern as SourcePattern,
    ReadRequest as SourceReadRequest, RepositoryId as SourceRepositoryId,
    Revision as SourceRevision, RevisionId as SourceRevisionId, SourceEntry, SourceRef};
pub use rows::{Edge, FamilyBundle, Node};
pub use scip::{
    byte_range, definition_of, join_documents, site_occurrence, Fallback, IndexerSpec, ScipClang,
    ScipGo, ScipJava, ScipPython, ScipRust, ScipTypescript, Staging,
};
pub use scip_ensure::{
    default_cache_dir, detect, ensure_index, index_path, EnsureReport, IndexBudget, Indexer,
    IndexerSkip, SkipReason, INDEXERS,
};
pub use scip_rows::{flatten_scip_records, ScipRecords, SCIP_RECORD_KINDS};
pub use scip_v5_rels::v5_rel_rows;
pub use seams::{
    build_def_index, containing_def_site, corpus_defs, covering_def, def_named, own_blob,
    BlobSource, DefIndex, DefSite, FileSet, IndexBag, ManifestMap, OccurrenceRole, ParseError,
    Parser, PositionEncoding, Project, ProjectCx, ProjectDigest, Resolve, ScipDiagnostic,
    ScipDocument, ScipError, ScipIndex, ScipMetadata, ScipOccurrence, ScipRelationship,
    ScipSignature, ScipSource, ScipSymbolInfo,
};
pub use shape::{BlobHash, FamilyTag, NameId, NodeRef, Span, Strings};
pub use source::{ExtractOutput, FamilyMask, Source};
pub use wire::{
    file_fact, flatten, flatten_flow, flatten_jsonl, flatten_scip, scip_file_edges, FlatFact,
    SpanOut, SCHEMA,
};
