//! The language roster. Commit 1: one entry, `astgrep` (one Parser covers
//! rust/ts/tsx/js/go via ast-grep's grammars). Per-language modules
//! (rust.rs / ts.rs / go.rs) land with commits 2-6 as the oxc / syn /
//! tree-sitter projectors + SCIP resolvers are added.

pub mod astgrep;
pub mod oxc;

pub use astgrep::{AstGrepParser, CstProjector, SgRoot};
pub use oxc::{OxcParser, TypeProjector};
