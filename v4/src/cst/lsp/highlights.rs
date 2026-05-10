//! Auto semantic-tokens for TS-backed DSLs via `highlights.scm` queries.
//!
//! Convention: every tree-sitter grammar in the wild ships a `highlights.scm`
//! whose captures use the standard `nvim-treesitter`/`helix` capture-name set
//! (`@function`, `@string`, `@keyword`, `@type`, `@number`, `@comment`, ...).
//! Each name maps deterministically to an `lsp_types::SemanticTokenType`.
//!
//! A TS-backed DSL compiles its `highlights.scm` into a `tree_sitter::Query`
//! once at construction. Inside `DslBodyLsp::semantic_tokens` it calls
//! [`highlights_to_semantic_tokens`] with that query, the cached parse tree,
//! and the body bytes; the helper returns body-relative `SemanticToken`s.
//! Hand-rolled / borrowed-engine DSLs walk their own AST and emit tokens
//! manually.
//!
//! The capture-name table is opinionated but standard. DSL authors override
//! by passing their own `&HashMap<&'static str, ...>` if they need to.
//!
//! Modifier suffixes: a capture like `@function.builtin` looks up `function`
//! as the base type, then OR-s in the `defaultLibrary` modifier. Recognized
//! suffixes: `builtin` → `defaultLibrary`, `readonly` → `readonly`,
//! `defaultLibrary` → `defaultLibrary`.

use std::collections::HashMap;

use lsp_types::{SemanticTokenModifier, SemanticTokenType};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor, Tree};

use crate::cst::lsp::providers::SemanticToken;

/// Registered tables for type/modifier indices. Mirrors the
/// `SemanticTokensLegend` the LSP server announces at initialization.
#[derive(Clone, Debug)]
pub struct Legend {
    pub types: Vec<SemanticTokenType>,
    pub modifiers: Vec<SemanticTokenModifier>,
}

impl Legend {
    /// Standard legend covering the types/modifiers our default capture table
    /// emits. Use this when you don't already have a project-wide legend.
    pub fn standard() -> Self {
        Self {
            types: vec![
                SemanticTokenType::FUNCTION,
                SemanticTokenType::VARIABLE,
                SemanticTokenType::PARAMETER,
                SemanticTokenType::KEYWORD,
                SemanticTokenType::STRING,
                SemanticTokenType::NUMBER,
                SemanticTokenType::COMMENT,
                SemanticTokenType::OPERATOR,
                SemanticTokenType::TYPE,
                SemanticTokenType::PROPERTY,
                SemanticTokenType::NAMESPACE,
                SemanticTokenType::REGEXP,
                SemanticTokenType::MACRO,
                SemanticTokenType::CLASS,
                SemanticTokenType::ENUM_MEMBER,
            ],
            modifiers: vec![
                SemanticTokenModifier::DEFAULT_LIBRARY,
                SemanticTokenModifier::READONLY,
            ],
        }
    }

    pub fn type_index(&self, t: &SemanticTokenType) -> Option<u32> {
        self.types.iter().position(|x| x == t).map(|i| i as u32)
    }

    pub fn modifier_bits(&self, mods: &[SemanticTokenModifier]) -> u32 {
        let mut bits: u32 = 0;
        for m in mods {
            if let Some(i) = self.modifiers.iter().position(|x| x == m) {
                bits |= 1u32 << i;
            }
        }
        bits
    }
}

/// Mapping entry: `(SemanticTokenType, base modifiers always applied)`.
pub type CaptureTable = HashMap<&'static str, (SemanticTokenType, Vec<SemanticTokenModifier>)>;

/// Standard nvim-treesitter / helix capture names → LSP types. Modifier
/// suffixes (`.builtin`, `.readonly`, ...) are resolved at lookup time and
/// don't need their own entries.
pub fn default_capture_table() -> CaptureTable {
    use SemanticTokenType as T;
    let mut m: CaptureTable = HashMap::new();
    m.insert("function", (T::FUNCTION, vec![]));
    m.insert("function.call", (T::FUNCTION, vec![]));
    m.insert("function.macro", (T::MACRO, vec![]));
    m.insert("method", (T::METHOD, vec![]));
    m.insert("constructor", (T::FUNCTION, vec![]));
    m.insert("variable", (T::VARIABLE, vec![]));
    m.insert("parameter", (T::PARAMETER, vec![]));
    m.insert("variable.parameter", (T::PARAMETER, vec![]));
    m.insert(
        "variable.builtin",
        (T::VARIABLE, vec![SemanticTokenModifier::DEFAULT_LIBRARY]),
    );
    m.insert(
        "constant",
        (T::VARIABLE, vec![SemanticTokenModifier::READONLY]),
    );
    m.insert(
        "constant.builtin",
        (
            T::VARIABLE,
            vec![
                SemanticTokenModifier::READONLY,
                SemanticTokenModifier::DEFAULT_LIBRARY,
            ],
        ),
    );
    m.insert("keyword", (T::KEYWORD, vec![]));
    m.insert("keyword.operator", (T::KEYWORD, vec![]));
    m.insert("string", (T::STRING, vec![]));
    m.insert("string.escape", (T::STRING, vec![]));
    m.insert("string.special", (T::REGEXP, vec![]));
    m.insert("number", (T::NUMBER, vec![]));
    m.insert("comment", (T::COMMENT, vec![]));
    m.insert("operator", (T::OPERATOR, vec![]));
    m.insert("type", (T::TYPE, vec![]));
    m.insert(
        "type.builtin",
        (T::TYPE, vec![SemanticTokenModifier::DEFAULT_LIBRARY]),
    );
    m.insert("property", (T::PROPERTY, vec![]));
    m.insert("namespace", (T::NAMESPACE, vec![]));
    m.insert("class", (T::CLASS, vec![]));
    m.insert("enum_member", (T::ENUM_MEMBER, vec![]));
    m.insert("punctuation", (T::OPERATOR, vec![]));
    m
}

/// Resolve a possibly-dotted capture name (e.g. `function.builtin`) against
/// the table. Falls back to progressively-shorter prefixes; modifier suffixes
/// merge into the result's modifier list.
fn lookup<'a>(
    table: &'a CaptureTable,
    name: &str,
) -> Option<(SemanticTokenType, Vec<SemanticTokenModifier>)> {
    if let Some((tt, mods)) = table.get(name) {
        return Some((tt.clone(), mods.clone()));
    }
    let mut cur = name;
    let mut extra_mods: Vec<SemanticTokenModifier> = Vec::new();
    while let Some(dot) = cur.rfind('.') {
        let suffix = &cur[dot + 1..];
        match suffix {
            "builtin" | "defaultLibrary" => extra_mods.push(SemanticTokenModifier::DEFAULT_LIBRARY),
            "readonly" => extra_mods.push(SemanticTokenModifier::READONLY),
            _ => {}
        }
        cur = &cur[..dot];
        if let Some((tt, mods)) = table.get(cur) {
            let mut merged = mods.clone();
            merged.extend(extra_mods);
            return Some((tt.clone(), merged));
        }
    }
    None
}

/// Run `query` against `tree.root_node()` over `body` and return one
/// body-relative `SemanticToken` per matched capture. The consumer shifts
/// these into host-document ranges with `crate::cst::lsp::shift::shift_to_host`
/// before sending them to the LSP client.
pub fn highlights_to_semantic_tokens(
    query: &Query,
    tree: &Tree,
    body: &[u8],
    legend: &Legend,
    table: &CaptureTable,
) -> Vec<SemanticToken> {
    let mut cursor = QueryCursor::new();
    let capture_names = query.capture_names();
    let mut tokens: Vec<SemanticToken> = Vec::new();
    let mut matches = cursor.matches(query, tree.root_node(), body);
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let name = capture_names[cap.index as usize];
            let Some((tt, mods)) = lookup(table, name) else {
                continue;
            };
            let Some(type_idx) = legend.type_index(&tt) else {
                continue;
            };
            let mod_bits = legend.modifier_bits(&mods);
            let r = cap.node.byte_range();
            tokens.push(SemanticToken {
                byte_range: r,
                token_type: type_idx,
                token_modifiers: mod_bits,
            });
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_capture_resolves_to_base_with_modifier() {
        let table = default_capture_table();
        let (tt, mods) = lookup(&table, "function.builtin").unwrap();
        assert_eq!(tt, SemanticTokenType::FUNCTION);
        assert!(mods.contains(&SemanticTokenModifier::DEFAULT_LIBRARY));
    }

    #[test]
    fn unknown_capture_returns_none() {
        let table = default_capture_table();
        assert!(lookup(&table, "made.up.capture").is_none());
    }

    #[test]
    fn legend_modifier_bits_are_or_packed() {
        let legend = Legend::standard();
        let bits = legend.modifier_bits(&[
            SemanticTokenModifier::DEFAULT_LIBRARY,
            SemanticTokenModifier::READONLY,
        ]);
        assert_eq!(bits, 0b11);
    }

    #[test]
    fn legend_type_index_lookup() {
        let legend = Legend::standard();
        assert_eq!(legend.type_index(&SemanticTokenType::FUNCTION), Some(0));
        assert_eq!(legend.type_index(&SemanticTokenType::COMMENT), Some(6));
        assert!(legend.type_index(&SemanticTokenType::EVENT).is_none());
    }
}
