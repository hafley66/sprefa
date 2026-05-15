//! `sql_where` — narrow SQL predicate DSL.
//!
//! The body inside a `where\`...\`` op call is a SQLite predicate
//! expression (no `SELECT`/`FROM`/`JOIN`/etc.). This DSL provides the
//! vocabulary, completions, and hover for that body in isolation.
//!
//! Phase 2 of "where strictly via SQL where-clause DSL". Phase 3
//! refactors `cst::dsls::sql::SqlDsl` to delegate its WHERE/ON/HAVING
//! sub-regions to this module. The architectural invariant after that:
//! `SqlWhereDsl` is the leaf predicate grammar; any SQL surface that
//! needs predicates composes it.

use lsp_types::{
    CompletionItem, CompletionItemKind, Hover, HoverContents, MarkedString, SemanticTokenType,
};

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureSink, Compiled, Dsl};
use crate::cst::lsp::highlights::Legend;
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};
use crate::cst::path::PathBuilder;

/// LSP context for a `where\`...\`` body. Carries the captures
/// available in the rule's binding scope. No table list — predicates
/// cannot `FROM`; references resolve against captures only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlWhereLspCtx {
    pub captures: Vec<String>,
}

pub struct SqlWhereDsl {
    legend: Legend,
}

impl Default for SqlWhereDsl {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlWhereDsl {
    pub fn new() -> Self {
        Self {
            legend: Legend::standard(),
        }
    }

    pub fn with_legend(legend: Legend) -> Self {
        Self { legend }
    }

    pub fn completions_with_ctx(
        &self,
        body: &[u8],
        byte: usize,
        ctx: &SqlWhereLspCtx,
    ) -> Vec<CompletionItem> {
        let mut out = Vec::new();
        let _ = byte;
        let _ = body;
        for capture in &ctx.captures {
            out.push(CompletionItem {
                label: format!("${{{capture}}}"),
                kind: Some(CompletionItemKind::VARIABLE),
                detail: Some("rule capture (host hole)".to_string()),
                ..Default::default()
            });
        }
        for kw in PREDICATE_KEYWORDS {
            out.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("predicate keyword".to_string()),
                ..Default::default()
            });
        }
        for func in PREDICATE_FUNCTIONS {
            out.push(CompletionItem {
                label: format!("{func}("),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("SQLite predicate function".to_string()),
                ..Default::default()
            });
        }
        out
    }

    pub fn diagnostics_with_ctx(&self, body: &[u8], ctx: &SqlWhereLspCtx) -> Vec<Diag> {
        let mut out = Vec::new();
        for token in scan_predicate(body) {
            if token.kind != PredicateTokenKind::HostHole {
                continue;
            }
            let raw = std::str::from_utf8(&body[token.range.clone()]).unwrap_or("");
            let name = raw.trim_start_matches("${").trim_end_matches('}');
            if !ctx.captures.iter().any(|c| c == name) {
                out.push(Diag::error(
                    "sql_where/unbound-capture",
                    format!("predicate references unbound capture `{name}`"),
                    token.range.clone(),
                ));
            }
        }
        out
    }

    pub fn hover_with_ctx(
        &self,
        body: &[u8],
        byte: usize,
        ctx: &SqlWhereLspCtx,
    ) -> Option<Hover> {
        for token in scan_predicate(body) {
            if byte < token.range.start || byte >= token.range.end {
                continue;
            }
            let text = std::str::from_utf8(&body[token.range.clone()]).ok()?;
            let msg = match token.kind {
                PredicateTokenKind::Keyword => predicate_keyword_hover(text),
                PredicateTokenKind::Function => Some("SQLite predicate function"),
                PredicateTokenKind::HostHole => {
                    let name = text.trim_start_matches("${").trim_end_matches('}');
                    if ctx.captures.iter().any(|c| c == name) {
                        Some("rule capture (resolved to alias.col by fuser)")
                    } else {
                        Some("rule capture (unbound — predicate will error at compile time)")
                    }
                }
                _ => None,
            }?;
            return Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(msg.to_string())),
                range: None,
            });
        }
        None
    }
}

impl Dsl for SqlWhereDsl {
    fn id(&self) -> &'static str {
        "sql_where"
    }

    fn compile(&self, _body: &[u8], _diags: &dyn DiagSink) -> Result<Box<dyn Compiled>, Diag> {
        // Predicate text is emitted as raw SQL by the fuser; nothing to
        // compile at this layer.
        Ok(Box::new(SqlWhereCompiled))
    }

    fn lsp(&self) -> Option<&dyn DslBodyLsp> {
        Some(self)
    }
}

pub struct SqlWhereCompiled;

impl Compiled for SqlWhereCompiled {
    fn match_into(&self, _target: &[u8], _target_off: usize, _sink: &mut dyn CaptureSink) {}
    fn emit_path_items(&self, _body_byte: usize, _builder: &mut PathBuilder) {}
}

impl DslBodyLsp for SqlWhereDsl {
    fn semantic_tokens(&self, body: &[u8]) -> Vec<SemanticToken> {
        let Some(types) = PredicateTokenTypes::from_legend(&self.legend) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for token in scan_predicate(body) {
            let token_type = match token.kind {
                PredicateTokenKind::Keyword => types.keyword,
                PredicateTokenKind::Function => types.function,
                PredicateTokenKind::Identifier => types.property,
                PredicateTokenKind::HostHole => types.variable,
                PredicateTokenKind::String => types.string,
                PredicateTokenKind::Number => types.number,
                PredicateTokenKind::Comment => types.comment,
                PredicateTokenKind::Operator => types.operator,
            };
            out.push(SemanticToken {
                byte_range: token.range,
                token_type,
                token_modifiers: 0,
            });
        }
        out
    }

    fn completions(&self, _body: &[u8], _byte: usize) -> Vec<CompletionItem> {
        let mut out = Vec::new();
        for kw in PREDICATE_KEYWORDS {
            out.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("predicate keyword".to_string()),
                ..Default::default()
            });
        }
        for func in PREDICATE_FUNCTIONS {
            out.push(CompletionItem {
                label: format!("{func}("),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("SQLite predicate function".to_string()),
                ..Default::default()
            });
        }
        out
    }

    fn hover(&self, body: &[u8], byte: usize) -> Option<Hover> {
        for token in scan_predicate(body) {
            if byte < token.range.start || byte >= token.range.end {
                continue;
            }
            let text = std::str::from_utf8(&body[token.range.clone()]).ok()?;
            let msg = match token.kind {
                PredicateTokenKind::Keyword => predicate_keyword_hover(text),
                PredicateTokenKind::Function => Some("SQLite predicate function"),
                PredicateTokenKind::HostHole => Some("rule capture (host hole)"),
                _ => None,
            }?;
            return Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(msg.to_string())),
                range: None,
            });
        }
        None
    }
}

// ── lexer + vocab ───────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct PredicateTokenTypes {
    keyword: u32,
    function: u32,
    property: u32,
    variable: u32,
    string: u32,
    number: u32,
    comment: u32,
    operator: u32,
}

impl PredicateTokenTypes {
    fn from_legend(legend: &Legend) -> Option<Self> {
        Some(Self {
            keyword: legend.type_index(&SemanticTokenType::KEYWORD)?,
            function: legend.type_index(&SemanticTokenType::FUNCTION)?,
            property: legend.type_index(&SemanticTokenType::PROPERTY)?,
            variable: legend.type_index(&SemanticTokenType::VARIABLE)?,
            string: legend.type_index(&SemanticTokenType::STRING)?,
            number: legend.type_index(&SemanticTokenType::NUMBER)?,
            comment: legend.type_index(&SemanticTokenType::COMMENT)?,
            operator: legend.type_index(&SemanticTokenType::OPERATOR)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PredicateTokenKind {
    Keyword,
    Function,
    Identifier,
    HostHole,
    String,
    Number,
    Comment,
    Operator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PredicateToken {
    pub range: std::ops::Range<usize>,
    pub kind: PredicateTokenKind,
}

/// Predicate-only keyword vocabulary. No `SELECT`/`FROM`/`JOIN`/`GROUP`
/// etc. Logical connectives, comparison helpers, and value tests only.
pub const PREDICATE_KEYWORDS: &[&str] = &[
    "AND", "BETWEEN", "CASE", "ELSE", "END", "EXISTS", "GLOB", "IN", "IS", "LIKE", "NOT", "NULL",
    "OR", "REGEXP", "THEN", "WHEN",
];

/// SQLite functions usable in a predicate. Aggregates (COUNT/SUM/AVG)
/// are excluded — they belong with a future `having\`...\`` op.
pub const PREDICATE_FUNCTIONS: &[&str] = &[
    "abs", "coalesce", "glob", "ifnull", "instr", "length", "like", "lower", "ltrim", "nullif",
    "rtrim", "substr", "trim", "upper",
];

pub(crate) fn scan_predicate(body: &[u8]) -> Vec<PredicateToken> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < body.len() {
        match body[i] {
            b'$' if i + 1 < body.len() && body[i + 1] == b'{' => {
                let lo = i;
                i += 2;
                while i < body.len() && body[i] != b'}' {
                    i += 1;
                }
                i = (i + 1).min(body.len());
                out.push(PredicateToken {
                    range: lo..i,
                    kind: PredicateTokenKind::HostHole,
                });
            }
            b'\'' | b'"' => {
                let quote = body[i];
                let lo = i;
                i += 1;
                while i < body.len() {
                    if body[i] == quote {
                        i += 1;
                        if i < body.len() && body[i] == quote {
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                out.push(PredicateToken {
                    range: lo..i,
                    kind: PredicateTokenKind::String,
                });
            }
            b'-' if i + 1 < body.len() && body[i + 1] == b'-' => {
                let lo = i;
                i += 2;
                while i < body.len() && body[i] != b'\n' {
                    i += 1;
                }
                out.push(PredicateToken {
                    range: lo..i,
                    kind: PredicateTokenKind::Comment,
                });
            }
            b if b.is_ascii_digit() => {
                let lo = i;
                i += 1;
                while i < body.len() && (body[i].is_ascii_digit() || body[i] == b'.') {
                    i += 1;
                }
                out.push(PredicateToken {
                    range: lo..i,
                    kind: PredicateTokenKind::Number,
                });
            }
            b if is_ident_start(b) => {
                let lo = i;
                i += 1;
                while i < body.len() && is_ident_continue(body[i]) {
                    i += 1;
                }
                let text = std::str::from_utf8(&body[lo..i]).unwrap_or("");
                let kind = if is_predicate_keyword(text) {
                    PredicateTokenKind::Keyword
                } else if is_predicate_function(text) {
                    PredicateTokenKind::Function
                } else {
                    PredicateTokenKind::Identifier
                };
                out.push(PredicateToken { range: lo..i, kind });
            }
            b if matches!(
                b,
                b'(' | b')' | b',' | b'.' | b'=' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%'
                    | b'!'
            ) =>
            {
                out.push(PredicateToken {
                    range: i..i + 1,
                    kind: PredicateTokenKind::Operator,
                });
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

pub(crate) fn is_predicate_keyword(text: &str) -> bool {
    PREDICATE_KEYWORDS.iter().any(|kw| text.eq_ignore_ascii_case(kw))
}

pub(crate) fn is_predicate_function(text: &str) -> bool {
    PREDICATE_FUNCTIONS.iter().any(|f| text.eq_ignore_ascii_case(f))
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(crate) fn predicate_keyword_hover(keyword: &str) -> Option<&'static str> {
    match keyword.to_ascii_uppercase().as_str() {
        "AND" => Some("logical conjunction — both operands must be true"),
        "OR" => Some("logical disjunction — at least one operand must be true"),
        "NOT" => Some("logical negation"),
        "IS" => Some("identity test — `IS NULL`, `IS NOT NULL`, or value equality"),
        "NULL" => Some("absent value"),
        "IN" => Some("set membership — `x IN (...)` or `x IN (SELECT ...)`"),
        "EXISTS" => Some("subquery non-emptiness test — typically as `NOT EXISTS` for anti-joins"),
        "LIKE" => Some("string pattern match using `%` and `_`"),
        "GLOB" => Some("shell-style pattern match using `*` and `?`"),
        "REGEXP" => Some("regex match (requires regex extension)"),
        "BETWEEN" => Some("range test — `x BETWEEN lo AND hi`"),
        "CASE" | "WHEN" | "THEN" | "ELSE" | "END" => Some("conditional expression"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_lexer_classifies_canonical_shapes() {
        let body = b"${LO} != ${OTHER_LO} AND length(${MATCH}) > 5";
        let tokens = scan_predicate(body);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&PredicateTokenKind::HostHole));
        assert!(kinds.contains(&PredicateTokenKind::Keyword));
        assert!(kinds.contains(&PredicateTokenKind::Function));
        assert!(kinds.contains(&PredicateTokenKind::Operator));
        assert!(kinds.contains(&PredicateTokenKind::Number));
    }

    #[test]
    fn completions_with_ctx_include_captures() {
        let dsl = SqlWhereDsl::new();
        let ctx = SqlWhereLspCtx {
            captures: vec!["LO".to_string(), "OTHER_LO".to_string()],
        };
        let items = dsl.completions_with_ctx(b"", 0, &ctx);
        assert!(items.iter().any(|i| i.label == "${LO}"));
        assert!(items.iter().any(|i| i.label == "${OTHER_LO}"));
        assert!(items.iter().any(|i| i.label == "AND"));
        assert!(items.iter().any(|i| i.label == "length("));
    }

    #[test]
    fn diagnostics_flag_unbound_capture() {
        let dsl = SqlWhereDsl::new();
        let ctx = SqlWhereLspCtx {
            captures: vec!["LO".to_string()],
        };
        let diags = dsl.diagnostics_with_ctx(b"${LO} != ${MISSING}", &ctx);
        assert_eq!(diags.len(), 1);
        assert_eq!(&*diags[0].code, "sql_where/unbound-capture");
        assert!(diags[0].message.contains("MISSING"));
    }

    #[test]
    fn predicate_keywords_exclude_full_sql_statement_words() {
        for excluded in &[
            "SELECT", "FROM", "JOIN", "WHERE", "GROUP", "ORDER", "BY", "LIMIT", "HAVING", "WITH",
            "RECURSIVE", "UNION", "AS", "INSERT", "UPDATE", "DELETE",
        ] {
            assert!(
                !is_predicate_keyword(excluded),
                "{excluded} must not be a predicate keyword — it belongs to full SQL"
            );
        }
    }

    #[test]
    fn predicate_functions_exclude_aggregates() {
        for excluded in &["count", "sum", "avg", "min", "max", "group_concat"] {
            assert!(
                !is_predicate_function(excluded),
                "{excluded} is an aggregate and must not be in the predicate function set"
            );
        }
    }
}
