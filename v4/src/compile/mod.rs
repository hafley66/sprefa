//! `compile/` — language adaptor: source text → `Pipe<Cursor>`.
//!
//! Layer shape:
//!   ast.rs     — value-typed AST (OpCall, PipeAst, SlotText, DslText) shared
//!                by parse and walk; no tree-sitter handles, no lifetimes.
//!   parse.rs   — host_parse(src) → (Vec<PipeAst>, Vec<Diag>) via tree-sitter-sprefa.
//!   walk.rs    — walk_program(&[PipeAst], &Registry, &mut LowerCtx)
//!                → (Vec<Pipe<Cursor>>, Vec<Diag>). Resolves slots into Value,
//!                calls Registry::lower per op, threads diags.
//!   lower/     — OperatorDef trait + Registry + validate_call + per-op wrappers.

pub mod ast;
pub mod lower;
// pub mod parse;   // dispatched: tree-sitter-sprefa + parse.rs port
pub mod walk;
