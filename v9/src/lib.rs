//! v9: a Rust-built datalog program, checked into a stratified `Plan`, run by an `Engine` over a
//! `Store`, each end picked by a `Recipe`. Design: `plans/v9/2026-10-25-v9-ends.plan.md`.

pub mod _0_id;
pub mod _1_effects;
pub mod _2_ir;
pub mod _2a_diag;
pub mod _3_plan;
pub mod _4_recipe;
pub mod _5_ends;

/// `Engine` impls.
pub mod _6_back {
    pub mod _0_rule;
    pub mod _1_dd;
}

/// `Store` impls.
pub mod _7_store {
    pub mod _0_sql;
    pub mod _1_sqlite;
}

/// `Core`: one engine, one store, the services, picked by the `Recipe`; the thread that owns it.
pub mod _8_runtime {
    pub mod _0_programs;
    pub mod _1_any;
    pub mod _2_ryi;
    pub mod _3_core;
    pub mod _4_handle;
}

pub mod _9_wire;

// The generated wire code names its siblings as `crate::models`, `crate::ops`, `crate::ops_auto`.
pub use _9_wire::{models, ops, ops_auto};
