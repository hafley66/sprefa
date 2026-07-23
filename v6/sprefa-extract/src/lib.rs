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

pub mod dispatch;
pub mod family;
pub mod lang;
pub mod rows;
pub mod seams;
pub mod shape;
pub mod source;
pub mod wire;

pub use dispatch::dispatch;
pub use family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, DfEdgeKind, DfF, DfNodeKind,
    Family, SigSlot, TypeEdgeKind, TypeEntityKind, TypeFAux, TypeF, TypeSig,
};
pub use lang::{source_for, sources, AstgrepSource, TsSource};
pub use rows::{Edge, FamilyBundle, Node};
pub use seams::{BlobSource, ParseError, Parser, Project};
pub use shape::{BlobHash, FamilyTag, NameId, NodeRef, Span, Strings};
pub use source::{ExtractOutput, FamilyMask, Source};
pub use wire::{flatten, flatten_jsonl, FlatFact, SpanOut};
