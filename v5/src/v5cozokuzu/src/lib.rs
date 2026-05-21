//! Shared fixture: the same call graph the `dl` examples build, so the Cozo
//! and Kuzu demos answer the identical question and you can compare surfaces.
//!
//!   main -> run -> parse -> lex      (helper is defined elsewhere, never called)
//!
//! "what does main reach transitively?"  ==>  run, parse, lex

pub const EDGES: &[(&str, &str)] = &[
    ("main", "run"),
    ("run", "parse"),
    ("parse", "lex"),
];
