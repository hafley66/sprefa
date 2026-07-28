//! Fixture crate root: makes every fixture .rs crate-reachable so
//! rust-analyzer indexes the whole corpus (its scip documents cover only
//! crate-graph-reachable files — scip-typescript has no such reachability
//! rule). Scip-ratchet wiring only; NOT a parity Case (no v5 oracle).
pub mod docs;
pub mod sample;
pub mod scip;
