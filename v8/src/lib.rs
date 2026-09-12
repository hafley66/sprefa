//! dl8: the DL7 language, compiled in Rust.
//!
//! The filesystem is the pipe. Each `_<n>_name` folder under `src/` is one
//! operator; `_6_eval` is the fixpoint kernel that both `_1_macrotime` and
//! `_4_comptime` call.

pub mod _0_read;
pub mod _1_macrotime;
pub mod _2_lower;
pub mod _3_check;
pub mod _4_comptime;
pub mod _5_reify;
pub mod _6_eval;
