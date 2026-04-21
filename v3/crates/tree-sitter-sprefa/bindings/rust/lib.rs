//! tree-sitter-sprefa — Rust binding.
//!
//! `LANGUAGE.into()` returns a `tree_sitter::Language` suitable for
//! `Parser::set_language`. The parser is compiled once by `build.rs`
//! from the generated `src/parser.c`.
//!
//! Tree shape is documented in `src/node-types.json`. Host node names
//! mirror the `grammar.js` rule names: `source_file`, `pipe`,
//! `op_invocation`, `bracket_slot`, `paren_slot`, `brace_slot`,
//! `carveout_expr`, `shell_literal`, `term_ref`, `ans_ref`,
//! `deprecated_scan_sigil`, `xref`, `cursor_ref`, `atom_literal`,
//! `string_literal`, `raw_string_literal`, `number_literal`,
//! `identifier`, `line_comment`.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_sprefa() -> *const ();
}

/// The tree-sitter `Language` for .sprf files. Call `.into()` to get a
/// `tree_sitter::Language` or pass directly to `Parser::set_language`.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_sprefa) };

/// Content of `node-types.json`. Useful for syntax-highlighter crates
/// that want to inspect the node grammar at runtime.
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

#[cfg(test)]
mod tests {
    #[test]
    fn can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading sprefa grammar");
    }
}
