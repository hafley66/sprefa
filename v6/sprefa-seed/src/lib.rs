#![allow(dead_code)]
//! sprefa v6 — SEED. All types in one crate; each top-level module BECOMES a crate.
//! Purpose: see the whole TYPE GRAPH in one build and iron out the rough parts
//! before splitting. No impls of substance — types + trait signatures.
//!
//! Files are numbered `_N_name.rs` in TOPO ORDER — purest/most-foundational at the
//! top (lowest N), impurity at the bottom. The `_` prefix keeps each a valid Rust
//! identifier so `mod _0_key;` works with NO `#[path]` attribute.
//!
//! module (future crate)     owns                                                   pure?
//!   _0_key                  dense identity: normalized ids, no coordinate on a fact   base
//!   _1_feldera              calculus: Z-set / weight / retraction / fixpoint          pure
//!   _2_lang                 language core (nested, topo inside): types..lower         pure types
//!   _3_extract              ONE Extractor seam; json/regex/ast/yaml = registered impls pure trait
//!   _4_store                Store trait: table vs view; sqlite hides behind this       pure trait
//!   _5_runtime              scheduler/reconciler + EFFECTS + MUTATION (--move)        least pure
//!
//! The reactivity thesis lives in `_2_lang::_4_analyze`: async/effect/purity is
//! INFERRED by painting the topo-stratified reference graph, not annotated by hand.

pub mod _0_key;
pub mod _1_feldera;
pub mod _2_lang;
pub mod _3_extract;
pub mod _4_store;
pub mod _5_runtime;
