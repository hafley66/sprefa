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

/// The walking skeleton (#820): ryi call edges -> differential dataflow reach -> SQLite.
pub mod skeleton {
    pub mod _0_store;
    pub mod _1_reach;
    pub mod _2_oracle;
}
