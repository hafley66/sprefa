#![allow(dead_code)]
//! sprefa v6 — SEED. All types in one crate; each top-level module BECOMES a crate.
//! Purpose: see the whole TYPE GRAPH in one build and iron out the rough parts
//! before splitting. No impls of substance — types + trait signatures.
//!
//! module (future crate)   owns
//!   key                   dense identity: normalized ids, no coordinate ever stored on a fact
//!   feldera               the calculus: Z-set / weight / retraction / fixpoint (denotational)
//!   lang                  the language core (nested): syntax, ast, types, ANALYZE, resolve, lower
//!   store                 the Store trait: table vs view; sqlite hides behind this later
//!   extract               ONE Extractor seam; json/regex/ast/yaml are registered impls
//!   runtime               the scheduler/reconciler: inputs -> tick -> commit deltas
//!
//! The reactivity thesis lives in `lang::analyze`: async/effect/purity is INFERRED
//! by painting the topo-stratified reference graph, not annotated by hand.

pub mod key;
pub mod feldera;
pub mod lang;
pub mod store;
pub mod extract;
pub mod runtime;
