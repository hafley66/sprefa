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

pub mod cache;
pub mod cfg;
pub mod cpg_decode;
pub mod cpg_types;
pub mod deps;
pub mod dispatch;
pub mod drain;
pub mod family;
pub mod lang;
pub mod manifests;
pub mod move_cx;
pub mod move_scip;
pub mod move_stage;
pub mod project;
pub mod rename_cx;
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
pub mod trace;
/// The run trail rides the same subscriber the `cli` feature installs.
#[cfg(feature = "cli")]
pub mod trail;
pub mod tsi;
pub mod types;
pub mod wire;

pub use cfg::{
    build_cfg, cfg_bundle, cfg_facts, roles_for, CfgRole, RoleRule, GO_ROLES, KOTLIN_ROLES,
    RUST_ROLES, TS_ROLES,
};
pub use cpg_decode::decode_cpg_struct;
pub use cpg_types::{
    CpgEdge, CpgEdgeKind, CpgImport, CpgImportError, CpgNode, CpgNodeKind, CpgProperty,
    CpgPropertyValue,
};
pub use deps::{resolve_specifier, Policy, TsconfigPaths};
pub use dispatch::dispatch;
pub use drain::{
    bind_action, directory_path, directory_source, drain_edits, replace_action, source_rel,
    stage_edits, BoundEdit, PendingReplaceDoc,
};
pub use family::{
    flow_edges, CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, DfArg, DfEdgeKind, DfF,
    DfFAux, DfField, DfLit, DfNodeKind, DfParam, DocFact, DocTag, Family, FlowEdge, FlowEdgeKind,
    FlowF, MethodOwner, ProjectEdge, ResolutionOrigin, SigSlot, Specifier, SpecifierKind,
    TypeEdgeCandidate,
    TypeEdgeKind, TypeEntityKind, TypeF, TypeFAux, TypeSig,
};
pub use lang::{
    build_paths, compiled_spellings, decode_ast_rule_yaml, dl6_db_path, open_dl6_readonly,
    open_readonly, query_ast_rule, query_ast_rule_with_content, query_patterns, rehome_for,
    rehomes, rename_for, renames, respell, source_for, sources, ts_specifiers, AstCaptureFact,
    AstPatternQuery, AstRule, AstRuleCapture, AstRuleError, AstRuleMatch, AstRuleMutationProposal,
    AstRuleRequest, AstgrepSource, BuildPaths, DataSource, DlSource, ExtractLang, FactError,
    FactMatcher, FactSet, GoSource, KotlinSource, MarkdownSource, NamedAstRule, PrologSource,
    PythonSource, RustSource, StopBy, TsResolver, TsSource, TsSpecifier, DL6_DB_RELATIVE_PATH,
};
pub use manifests::{
    fold_package_edges, package_edges, package_edges_jsonl, Manifest, ManifestKind,
};
pub use move_cx::{dirname, join_rel, normalize, relative_between, MoveCx, SKIP_DIRS};
pub use move_scip::{
    scip_import_sites, verify_import_refs, ScipDisagreement, ScipSite, MISSED_BY_IMPL,
    UNKNOWN_TO_SCIP,
};
pub use project::{
    diet_scip, diet_scip_jsonl, extract_pool, resolve_project, resolve_project_jsonl, scip_facts,
    scip_facts_jsonl, scip_family, scip_family_jsonl, scip_file_edges_jsonl, scip_index_location,
    FsBlobSource, ProjectError, ResolveArm, ResolveArms, ResolveRequest, ScipFamilyRequest,
    ScipMode, SourceTreeBlobSource, RESOLVE_ARMS,
};
pub use rename_cx::{RenameCx, RenameRequest};
pub use rows::{Edge, FamilyBundle, Node};
pub use scip::{
    byte_range, byte_range_cached, copy_sources, definition_of, join_documents, site_occurrence,
    Fallback, IndexerSpec, ScipClang, ScipGo, ScipJava, ScipPython, ScipRust, ScipTypescript,
    Staging,
};
pub use scip_ensure::{
    default_cache_dir, detect, ensure_index, ensure_index_for_set, external_cache_dir,
    fresh_index_for_set, index_path, index_path_for_set, record_index_set, root_key, EnsureReport,
    IndexBudget, IndexSet, Indexer, IndexerSkip, SkipReason, INDEXERS,
};
pub use scip_rows::{flatten_scip_records, ScipRecords, SCIP_RECORD_KINDS};
pub use scip_v5_rels::v5_rel_rows;
pub use seams::{
    build_def_index, containing_def_site, containing_def_site_in, corpus_defs, covering_def,
    def_named, own_blob, BlobSource, DefIndex, DefSite, FileSet, IndexBag, ManifestMap,
    OccurrenceRole, ParseError, Parser, PositionEncoding, Project, ProjectCx, ProjectDigest,
    Resolve, ScipDiagnostic, ScipDocument, ScipError, ScipIndex, ScipMetadata, ScipOccurrence,
    ScipRelationship, ScipSignature, ScipSource, ScipSymbolInfo,
};
pub use shape::{
    content_id_of, ContentId, FamilyTag, NameId, NodeRef, Span, Strings, ZERO_CONTENT_ID,
};
pub use soopy::{
    ContentId as SourceContentId, Pattern as SourcePattern, ReadRequest as SourceReadRequest,
    RepositoryId as SourceRepositoryId, Revision as SourceRevision, RevisionId as SourceRevisionId,
    SourceEntry, SourceRef,
};
pub use source::{ExtractOutput, FamilyMask, Source};
pub use types::{
    CfgEdgeKind, CfgF, CfgNodeKind, ImportRef, ImportRefKind, RefRole, Rehome, RehomeArm,
    RehomeManifests, RehomePlanCheck, RehomeShim, RehomeTextSpellings, Rename, RenameStop, Respell,
    SymbolId, SymbolInterner, SymbolRef, SymbolSeat,
};
pub use wire::{
    file_fact, flatten, flatten_cfg, flatten_cfg_each, flatten_each, flatten_flow, flatten_jsonl,
    flatten_scip, scip_file_edges, size_skip_fact, FlatFact, SpanOut, DEFAULT_MAX_BYTES, SCHEMA,
};
