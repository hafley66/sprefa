//! Lexer + parser -> lossless CST. Stub in the seed (no parser lib yet). The one
//! rule that keeps the surface tiny: extract/effect ops are GENERIC CALLS
//! `name(inputs...) -> (outputs...)`, ONE production; `name` binds to a registry
//! handler. No per-op keyword/grammar -> a new extractor/effect needs zero syntax.

pub struct Cst;      // lossless tree (rowan|chumsky later)
pub struct Token;    // lexer token
