//! HTTP boundary. `models/`, `ops_auto.rs`, `http_auto.rs` are generated from
//! `schema/0_wire.tsp` by `node schema/1_generate.mjs`; this file is hand-owned.
//! The runtime implements `ops_auto::Wire` and serves `http_auto::router(Arc<impl Wire>)`.

pub mod http_auto;
pub mod models;
pub mod ops_auto;
