//! In-tree DSL examples shipped with the cst lib.
//!
//! Each submodule implements `Dsl` + `Compiled` + (optionally) `DslBodyLsp`.
//! These are dogfood: they validate the lib's surface against three
//! implementation strategies (TS-backed grammar, hand-rolled, borrowed engine)
//! without leaking any sprf vocabulary into the lib core.

pub mod ast;
pub mod glob;
pub mod json;
pub mod markdown;
pub mod re;
pub mod sql;
pub mod sql_where;
