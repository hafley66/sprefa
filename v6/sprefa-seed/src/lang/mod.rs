//! The LANGUAGE CORE. Nested modules (they share the AST) that become the
//! `sprefa-lang::*` module tree. The centerpiece is `analyze`: async/effect/purity
//! is INFERRED by painting the topo-stratified reference graph, not annotated.

pub mod syntax;   // lexer + parser -> CST (stub here)
pub mod ast;      // typed AST: Rel, Rule{Source|Derived|Extract|Effect}, Body, Term
pub mod types;    // the type system: nested records + dot-access (projection) kinds
pub mod analyze;  // effect/purity inference + stratification (ONE ref-graph pass)
pub mod resolve;  // name resolution + variable binding (stub)
pub mod lower;    // AST -> plan; NORMALIZES dot access into joins (stub)
