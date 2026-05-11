//! `sql` body LSP support.
//!
//! Runtime execution for the host `sql`` op lives in `crate::sql`.
//! This module keeps body-local editor affordances in the CST DSL layer.

use std::collections::BTreeMap;

use lsp_types::{
    CompletionItem, CompletionItemKind, Hover, HoverContents, MarkedString, SemanticTokenType,
};

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureSink, Compiled, Dsl};
use crate::cst::lsp::highlights::Legend;
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};
use crate::cst::path::PathBuilder;

pub struct SqlDsl {
    legend: Legend,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlLspCtx {
    pub input_cols: Vec<SqlCol>,
    pub rule_tables: Vec<SqlTable>,
    pub core_tables: Vec<SqlTable>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlTable {
    pub name: String,
    pub cols: Vec<SqlCol>,
}

impl SqlTable {
    pub fn new<I, S>(name: impl Into<String>, cols: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into(),
            cols: cols.into_iter().map(SqlCol::text).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlCol {
    pub name: String,
    pub ty: SqlType,
}

impl SqlCol {
    pub fn text(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ty: SqlType::Text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SqlType {
    Text,
}

impl Default for SqlDsl {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlDsl {
    pub fn new() -> Self {
        Self {
            legend: Legend::standard(),
        }
    }

    pub fn completions_with_ctx(
        &self,
        body: &[u8],
        byte: usize,
        ctx: &SqlLspCtx,
    ) -> Vec<CompletionItem> {
        let clipped = byte.min(body.len());
        if let Some(qualifier) = qualifier_before_dot(&body[..clipped]) {
            return self.qualified_column_completions(body, ctx, &qualifier);
        }

        if is_table_position(&body[..clipped]) {
            return ctx
                .rule_tables
                .iter()
                .chain(ctx.core_tables.iter())
                .map(|table| CompletionItem {
                    label: table.name.clone(),
                    kind: Some(CompletionItemKind::CLASS),
                    detail: Some("SQL relation".to_string()),
                    ..Default::default()
                })
                .collect();
        }

        self.completions(body, byte)
    }

    pub fn diagnostics_with_ctx(&self, body: &[u8], ctx: &SqlLspCtx) -> Vec<Diag> {
        let tables = visible_tables(ctx);
        let tokens = scan_sql(body);
        let mut out = Vec::new();
        let mut i = 0usize;
        while i + 1 < tokens.len() {
            let text = token_text(body, &tokens[i]);
            if keyword_eq(text, "FROM") || keyword_eq(text, "JOIN") {
                let table = &tokens[i + 1];
                if table.kind == SqlTokenKind::Identifier || table.kind == SqlTokenKind::InputName {
                    let table_name = token_text(body, table);
                    if !tables.contains_key(table_name) {
                        out.push(Diag::error(
                            "sql/unknown-table",
                            format!("unknown SQL table `{table_name}`"),
                            table.range.clone(),
                        ));
                    }
                }
            }
            i += 1;
        }
        out
    }

    fn qualified_column_completions(
        &self,
        body: &[u8],
        ctx: &SqlLspCtx,
        qualifier: &str,
    ) -> Vec<CompletionItem> {
        let aliases = visible_aliases(body, ctx);
        let Some(table) = aliases.get(qualifier) else {
            return Vec::new();
        };
        table
            .cols
            .iter()
            .map(|col| CompletionItem {
                label: format!("{qualifier}.{}", col.name),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(format!("{}.{}", table.name, col.name)),
                ..Default::default()
            })
            .collect()
    }
}

impl Dsl for SqlDsl {
    fn id(&self) -> &'static str {
        "sql"
    }

    fn compile(&self, _body: &[u8], _diags: &dyn DiagSink) -> Result<Box<dyn Compiled>, Diag> {
        Ok(Box::new(SqlCompiled))
    }

    fn lsp(&self) -> Option<&dyn DslBodyLsp> {
        Some(self)
    }
}

pub struct SqlCompiled;

impl Compiled for SqlCompiled {
    fn match_into(&self, _target: &[u8], _target_off: usize, _sink: &mut dyn CaptureSink) {}

    fn emit_path_items(&self, _body_byte: usize, _builder: &mut PathBuilder) {}
}

impl DslBodyLsp for SqlDsl {
    fn semantic_tokens(&self, body: &[u8]) -> Vec<SemanticToken> {
        let Some(types) = SqlTokenTypes::from_legend(&self.legend) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for token in scan_sql(body) {
            let token_type = match token.kind {
                SqlTokenKind::Keyword => types.keyword,
                SqlTokenKind::Identifier => types.property,
                SqlTokenKind::InputName => types.namespace,
                SqlTokenKind::HostHole => types.variable,
                SqlTokenKind::String => types.string,
                SqlTokenKind::Number => types.number,
                SqlTokenKind::Comment => types.comment,
                SqlTokenKind::Operator => types.operator,
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
        for label in SQL_COMPLETION_KEYWORDS {
            out.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("SQLite keyword".to_string()),
                ..Default::default()
            });
        }
        for (label, detail) in [
            ("input.__cursor_idx", "source cursor row index"),
            ("input.value", "current cursor value"),
            ("NOT EXISTS", "anti-join predicate"),
            ("WITH RECURSIVE", "recursive CTE"),
        ] {
            out.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(detail.to_string()),
                ..Default::default()
            });
        }
        out
    }

    fn hover(&self, body: &[u8], byte: usize) -> Option<Hover> {
        for token in scan_sql(body) {
            if byte < token.range.start || byte >= token.range.end {
                continue;
            }
            let text = std::str::from_utf8(&body[token.range.clone()]).ok()?;
            let msg = match token.kind {
                SqlTokenKind::Keyword => keyword_hover(text),
                SqlTokenKind::InputName if text.eq_ignore_ascii_case("input") => {
                    Some("current upstream cursor batch as a temp relation")
                }
                SqlTokenKind::Identifier if text == "__cursor_idx" => {
                    Some("source cursor index used to clone the input cursor")
                }
                SqlTokenKind::Identifier if text == "value" => {
                    Some("selected value replaces cursor.value")
                }
                SqlTokenKind::HostHole => {
                    Some("host interpolation lowered to an input column reference")
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

#[derive(Clone, Copy)]
struct SqlTokenTypes {
    keyword: u32,
    property: u32,
    namespace: u32,
    variable: u32,
    string: u32,
    number: u32,
    comment: u32,
    operator: u32,
}

impl SqlTokenTypes {
    fn from_legend(legend: &Legend) -> Option<Self> {
        Some(Self {
            keyword: legend.type_index(&SemanticTokenType::KEYWORD)?,
            property: legend.type_index(&SemanticTokenType::PROPERTY)?,
            namespace: legend.type_index(&SemanticTokenType::NAMESPACE)?,
            variable: legend.type_index(&SemanticTokenType::VARIABLE)?,
            string: legend.type_index(&SemanticTokenType::STRING)?,
            number: legend.type_index(&SemanticTokenType::NUMBER)?,
            comment: legend.type_index(&SemanticTokenType::COMMENT)?,
            operator: legend.type_index(&SemanticTokenType::OPERATOR)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SqlTokenKind {
    Keyword,
    Identifier,
    InputName,
    HostHole,
    String,
    Number,
    Comment,
    Operator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SqlToken {
    range: std::ops::Range<usize>,
    kind: SqlTokenKind,
}

const SQL_KEYWORDS: &[&str] = &[
    "AS",
    "ASC",
    "BY",
    "CASE",
    "DESC",
    "DISTINCT",
    "EXISTS",
    "FROM",
    "GROUP",
    "HAVING",
    "IN",
    "INNER",
    "JOIN",
    "LEFT",
    "LIMIT",
    "NOT",
    "NULL",
    "ON",
    "OR",
    "ORDER",
    "RECURSIVE",
    "SELECT",
    "UNION",
    "WHERE",
    "WITH",
];

const SQL_COMPLETION_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "JOIN",
    "WHERE",
    "NOT EXISTS",
    "WITH RECURSIVE",
    "GROUP BY",
    "ORDER BY",
    "LIMIT",
];

fn scan_sql(body: &[u8]) -> Vec<SqlToken> {
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
                out.push(SqlToken {
                    range: lo..i,
                    kind: SqlTokenKind::HostHole,
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
                out.push(SqlToken {
                    range: lo..i,
                    kind: SqlTokenKind::String,
                });
            }
            b'-' if i + 1 < body.len() && body[i + 1] == b'-' => {
                let lo = i;
                i += 2;
                while i < body.len() && body[i] != b'\n' {
                    i += 1;
                }
                out.push(SqlToken {
                    range: lo..i,
                    kind: SqlTokenKind::Comment,
                });
            }
            b'/' if i + 1 < body.len() && body[i + 1] == b'*' => {
                let lo = i;
                i += 2;
                while i + 1 < body.len() && !(body[i] == b'*' && body[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(body.len());
                out.push(SqlToken {
                    range: lo..i,
                    kind: SqlTokenKind::Comment,
                });
            }
            b if b.is_ascii_digit() => {
                let lo = i;
                i += 1;
                while i < body.len() && (body[i].is_ascii_digit() || body[i] == b'.') {
                    i += 1;
                }
                out.push(SqlToken {
                    range: lo..i,
                    kind: SqlTokenKind::Number,
                });
            }
            b if is_ident_start(b) => {
                let lo = i;
                i += 1;
                while i < body.len() && is_ident_continue(body[i]) {
                    i += 1;
                }
                let text = std::str::from_utf8(&body[lo..i]).unwrap_or("");
                let kind = if is_keyword(text) {
                    SqlTokenKind::Keyword
                } else if text.eq_ignore_ascii_case("input") {
                    SqlTokenKind::InputName
                } else {
                    SqlTokenKind::Identifier
                };
                out.push(SqlToken { range: lo..i, kind });
            }
            b if matches!(
                b,
                b'(' | b')' | b',' | b'.' | b'=' | b'<' | b'>' | b'+' | b'-' | b'*'
            ) =>
            {
                out.push(SqlToken {
                    range: i..i + 1,
                    kind: SqlTokenKind::Operator,
                });
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn is_keyword(s: &str) -> bool {
    SQL_KEYWORDS.iter().any(|kw| s.eq_ignore_ascii_case(kw))
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn keyword_hover(keyword: &str) -> Option<&'static str> {
    match keyword.to_ascii_uppercase().as_str() {
        "SELECT" => Some("project SQL result columns into output cursors"),
        "FROM" => Some("read from input, rule tables, or future core tables"),
        "JOIN" | "INNER" | "LEFT" => Some("combine rows by SQL join predicates"),
        "WHERE" => Some("filter rows by predicate"),
        "NOT" | "EXISTS" => Some("use NOT EXISTS for anti-join / missing rows"),
        "WITH" | "RECURSIVE" => Some("CTE support for graph traversal queries"),
        "GROUP" | "ORDER" | "LIMIT" => Some("SQLite result shaping clause"),
        _ => None,
    }
}

fn token_text<'a>(body: &'a [u8], token: &SqlToken) -> &'a str {
    std::str::from_utf8(&body[token.range.clone()]).unwrap_or("")
}

fn keyword_eq(text: &str, keyword: &str) -> bool {
    text.eq_ignore_ascii_case(keyword)
}

fn visible_tables(ctx: &SqlLspCtx) -> BTreeMap<String, SqlTable> {
    let mut out = BTreeMap::new();
    out.insert(
        "input".to_string(),
        SqlTable {
            name: "input".to_string(),
            cols: ctx.input_cols.clone(),
        },
    );
    for table in ctx.rule_tables.iter().chain(ctx.core_tables.iter()) {
        out.insert(table.name.clone(), table.clone());
    }
    out
}

fn visible_aliases(body: &[u8], ctx: &SqlLspCtx) -> BTreeMap<String, SqlTable> {
    let tables = visible_tables(ctx);
    let mut out = tables.clone();
    let tokens = scan_sql(body);
    let mut i = 0usize;
    while i + 1 < tokens.len() {
        let text = token_text(body, &tokens[i]);
        if keyword_eq(text, "FROM") || keyword_eq(text, "JOIN") {
            let table_name = token_text(body, &tokens[i + 1]);
            if let Some(table) = tables.get(table_name) {
                if i + 3 < tokens.len() && keyword_eq(token_text(body, &tokens[i + 2]), "AS") {
                    out.insert(token_text(body, &tokens[i + 3]).to_string(), table.clone());
                    i += 3;
                } else if i + 2 < tokens.len() {
                    let alias = &tokens[i + 2];
                    let alias_text = token_text(body, alias);
                    if alias.kind == SqlTokenKind::Identifier && !is_keyword(alias_text) {
                        out.insert(alias_text.to_string(), table.clone());
                        i += 2;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

fn qualifier_before_dot(prefix: &[u8]) -> Option<String> {
    let mut i = prefix.len();
    while i > 0 && prefix[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || prefix[i - 1] != b'.' {
        return None;
    }
    i -= 1;
    let hi = i;
    while i > 0 && is_ident_continue(prefix[i - 1]) {
        i -= 1;
    }
    if hi == i {
        return None;
    }
    std::str::from_utf8(&prefix[i..hi]).ok().map(str::to_string)
}

fn is_table_position(prefix: &[u8]) -> bool {
    let text = std::str::from_utf8(prefix).unwrap_or("");
    let mut words = text
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '(' | ')' | ',' | ';'))
        .filter(|word| !word.is_empty());
    let Some(last) = words.next_back() else {
        return false;
    };
    keyword_eq(last, "FROM") || keyword_eq(last, "JOIN")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::dsl::Dsl;

    #[test]
    fn sql_lsp_semantic_tokens_classify_core_shapes() {
        let dsl = SqlDsl::new();
        let body = b"SELECT input.__cursor_idx, input.OP FROM input WHERE NOT EXISTS (SELECT 1 FROM frontend_hooks WHERE frontend_hooks.OP = ${OP})";
        let tokens = dsl.semantic_tokens(body);
        assert!(!tokens.is_empty(), "expected semantic tokens");

        let select = tokens
            .iter()
            .find(|t| &body[t.byte_range.clone()] == b"SELECT")
            .unwrap();
        let input = tokens
            .iter()
            .find(|t| &body[t.byte_range.clone()] == b"input")
            .unwrap();
        let hole = tokens
            .iter()
            .find(|t| &body[t.byte_range.clone()] == b"${OP}")
            .unwrap();

        let legend = Legend::standard();
        assert_eq!(
            select.token_type,
            legend.type_index(&SemanticTokenType::KEYWORD).unwrap()
        );
        assert_eq!(
            input.token_type,
            legend.type_index(&SemanticTokenType::NAMESPACE).unwrap()
        );
        assert_eq!(
            hole.token_type,
            legend.type_index(&SemanticTokenType::VARIABLE).unwrap()
        );
    }

    #[test]
    fn sql_lsp_completions_include_sqlite_words_and_input_columns() {
        let dsl = SqlDsl::new();
        let labels: Vec<String> = dsl
            .completions(b"SELECT ", 7)
            .into_iter()
            .map(|item| item.label)
            .collect();
        assert!(labels.contains(&"SELECT".to_string()));
        assert!(labels.contains(&"NOT EXISTS".to_string()));
        assert!(labels.contains(&"input.__cursor_idx".to_string()));
        assert!(labels.contains(&"input.value".to_string()));
    }

    #[test]
    fn sql_lsp_hover_classifies_input_and_host_hole() {
        let dsl = SqlDsl::new();
        let input = dsl.hover(b"SELECT input.OP", 8).expect("hover input");
        match input.contents {
            HoverContents::Scalar(MarkedString::String(s)) => {
                assert!(s.contains("upstream cursor batch"), "got {s:?}");
            }
            _ => panic!("unexpected hover shape"),
        }

        let hole = dsl
            .hover(b"WHERE frontend_hooks.OP = ${OP}", 27)
            .expect("hover host hole");
        match hole.contents {
            HoverContents::Scalar(MarkedString::String(s)) => {
                assert!(s.contains("input column"), "got {s:?}");
            }
            _ => panic!("unexpected hover shape"),
        }
    }

    #[test]
    fn sql_dsl_is_dyn_compilable_noop_matcher() {
        let dsl: Box<dyn Dsl> = Box::new(SqlDsl::new());
        let compiled = dsl
            .compile(b"SELECT 1", &crate::cst::diag::SilentSink)
            .unwrap();
        let mut sink = crate::cst::dsl::VecCaptureSink::new();
        compiled.match_into(b"anything", 0, &mut sink);
        assert!(sink.rows.is_empty());
    }

    #[test]
    fn sql_lsp_ctx_completes_tables_after_from() {
        let dsl = SqlDsl::new();
        let ctx = SqlLspCtx {
            input_cols: vec![SqlCol::text("__cursor_idx"), SqlCol::text("value")],
            rule_tables: vec![SqlTable::new("frontend_hooks", ["OP", "FILE"])],
            core_tables: vec![SqlTable::new("_strings", ["id", "content", "norm"])],
        };

        let labels: Vec<String> = dsl
            .completions_with_ctx(b"SELECT * FROM ", 14, &ctx)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert_eq!(
            labels,
            vec!["frontend_hooks".to_string(), "_strings".to_string(),]
        );
    }

    #[test]
    fn sql_lsp_ctx_completes_alias_columns_after_dot() {
        let dsl = SqlDsl::new();
        let ctx = SqlLspCtx {
            input_cols: vec![
                SqlCol::text("__cursor_idx"),
                SqlCol::text("value"),
                SqlCol::text("OP"),
            ],
            rule_tables: vec![SqlTable::new("frontend_hooks", ["OP", "FILE"])],
            core_tables: vec![],
        };

        let labels: Vec<String> = dsl
            .completions_with_ctx(
                b"SELECT hooks. FROM input JOIN frontend_hooks AS hooks ON hooks.OP = input.OP",
                13,
                &ctx,
            )
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert_eq!(
            labels,
            vec!["hooks.OP".to_string(), "hooks.FILE".to_string()]
        );
    }

    #[test]
    fn sql_lsp_ctx_diagnoses_unknown_table() {
        let dsl = SqlDsl::new();
        let ctx = SqlLspCtx {
            input_cols: vec![SqlCol::text("__cursor_idx"), SqlCol::text("value")],
            rule_tables: vec![SqlTable::new("frontend_hooks", ["OP", "FILE"])],
            core_tables: vec![],
        };

        let diags = dsl.diagnostics_with_ctx(b"SELECT * FROM missing_hooks", &ctx);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "sql/unknown-table");
        assert_eq!(diags[0].message, "unknown SQL table `missing_hooks`");
    }
}
