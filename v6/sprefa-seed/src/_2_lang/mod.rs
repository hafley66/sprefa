//! The LANGUAGE CORE. Nested modules (they share the AST) that become the
//! `sprefa-lang::*` module tree. The centerpiece is `analyze`: async/effect/purity
//! is INFERRED by painting the topo-stratified reference graph, not annotated.

pub mod _0_types;    // the type system: nested records + dot-access (projection) kinds
pub mod _1_syntax;   // lexer + parser -> CST (stub here)
pub mod _2_ast;      // typed AST: Rel, Rule{Source|Derived|Extract|Effect}, Body, Term
pub mod _3_resolve;  // name resolution + variable binding (stub)
pub mod _4_analyze;  // effect/purity inference + stratification (ONE ref-graph pass)
pub mod _5_lower;    // AST + analysis -> plan; NORMALIZES dot access into joins (stub)
