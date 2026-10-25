//! HTTP boundary. `models/`, `ops_auto.rs`, `http_auto.rs` are generated from
//! `schema/0_wire.tsp` by `node schema/1_generate.mjs`; `ops.rs` is hand-owned.

// The generator imports extractors for every op shape, used or not.
#[allow(unused_imports)]
pub mod http_auto;
pub mod models;
pub mod ops;
pub mod ops_auto;
