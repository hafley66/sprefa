//! HTTP boundary. `models/`, `ops_auto.rs`, `http_auto.rs` are generated from
//! `schema/0_wire.tsp` by `node schema/1_generate.mjs`; this file is hand-owned.
//! The runtime implements `ops_auto::Wire` and serves `http_auto::router(Arc<impl Wire>)`.

// Generated code is the effect edge of the wire: each handler runs its op on tokio's blocking
// pool (`tokio::task::spawn_blocking`), the one effect `clippy.toml` allows outside
// `src/_1_effects.rs`.
#[allow(clippy::disallowed_methods)]
pub mod http_auto;
pub mod models;
pub mod ops_auto;
