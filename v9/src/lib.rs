//! v9 walking skeleton: ryi call edges -> differential dataflow reach -> SQLite, incremental
//! across runs. Design: `plans/v9/2026-10-21-v9-core-types.md`.

pub mod _0_id;
pub mod _1_effects;
pub mod _2_store;
pub mod _3_reach;
pub mod _4_oracle;
pub mod _9_wire;

// The generated wire code names its siblings as `crate::models`, `crate::ops`, `crate::ops_auto`.
pub use _9_wire::{models, ops, ops_auto};
