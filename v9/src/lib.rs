//! v9 walking skeleton: ryi call edges -> differential dataflow reach -> SQLite, incremental
//! across runs. Design: `plans/v9/2026-10-21-v9-core-types.md`.

pub mod _0_id;
pub mod _1_effects;
pub mod _2_store;
pub mod _3_reach;
pub mod _4_oracle;
