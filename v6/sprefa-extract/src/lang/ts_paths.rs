//! The relative path constants a TS/JS file hands to a path builder: the
//! literals a rehomed file re-aims the way it re-aims its imports.
//! @comment-ok: module header, the seam list every lang file opens with
//!
//! WHY AN AST WALK AND NOT A REGEX. `"../fixtures"` is a path constant inside
//! `new URL(...)` and a plain argument inside `source("grapht", ...)`; the text
//! is identical and only the callee separates them. A regex over the source
//! rewrites both, and the second rewrite silently breaks a passing test.

use oxc_ast::ast as ts;
use oxc_ast_visit::Visit as OxcVisit;

use super::ts::OxcParser;
use crate::seams::{ParseError, Parser};
use crate::types::Span;

/// The callees whose string-literal arguments name a file path. A member form
/// (`path.resolve`) matches on the property, the way node's own API reads.
const PATH_CALLEES: [&str; 4] = ["URL", "resolve", "join", "fileURLToPath"];

/// A relative path constant as written: the literal's byte span including its
/// quotes, the path itself, and the quote character a re-aim preserves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsPathLiteral {
    pub span: Span,
    pub text: String,
    pub quote: char,
}

/// Whether a literal spells a path relative to the writing file. A name that
/// merely opens with a dot (`.env`) spells no directory step and is left alone.
pub fn is_relative_path(text: &str) -> bool {
    text.starts_with("./") || text.starts_with("../") || text == "." || text == ".."
}

/// Every relative path constant one TS/JS file writes, in source order, off its
/// own oxc parse. Only a file this run moves is worth parsing twice.
pub fn ts_path_literals(path: &str, content: &str) -> Result<Vec<TsPathLiteral>, ParseError> {
    let parser = OxcParser;
    let arena = parser.make_arena();
    let program = parser.parse(&arena, path, content.as_bytes())?;
    let mut walker = PathLiteralWalker { out: Vec::new() };
    walker.visit_program(&program);
    let mut rows: Vec<TsPathLiteral> = walker
        .out
        .into_iter()
        .filter(|(_, text)| is_relative_path(text))
        .map(|(span, text)| TsPathLiteral {
            span: Span {
                start: span.start,
                len: span.end - span.start,
            },
            text: text.to_string(),
            quote: quote_at(content, span.start as usize),
        })
        .collect();
    rows.sort_by_key(|row| row.span.start);
    rows.dedup_by_key(|row| row.span.start);
    Ok(rows)
}

/// A literal's opening quote. Anything else at the span start means the caller
/// paired a parse with different text; `"` keeps the replacement well formed.
fn quote_at(content: &str, start: usize) -> char {
    match content.as_bytes().get(start) {
        Some(b'\'') => '\'',
        Some(b'`') => '`',
        _ => '"',
    }
}

/// Every string literal handed straight to a path builder. Nesting needs no arm:
/// in `fileURLToPath(new URL(lit, base))` the literal is the inner call's own.
struct PathLiteralWalker<'a> {
    out: Vec<(oxc_span::Span, &'a str)>,
}

impl<'a> PathLiteralWalker<'a> {
    fn take_arguments(&mut self, callee: &ts::Expression<'a>, arguments: &[ts::Argument<'a>]) {
        let Some(name) = path_callee(callee) else {
            return;
        };
        // `Array.prototype.join` takes a separator and node's `path.join` of one
        // segment is a no-op, so a lone argument is never a path segment.
        if name == "join" && arguments.len() < 2 {
            return;
        }
        for argument in arguments {
            if let Some(ts::Expression::StringLiteral(literal)) = argument.as_expression() {
                self.out.push((literal.span, literal.value.as_str()));
            }
        }
    }
}

impl<'a> OxcVisit<'a> for PathLiteralWalker<'a> {
    fn visit_call_expression(&mut self, it: &ts::CallExpression<'a>) {
        self.take_arguments(&it.callee, &it.arguments);
        oxc_ast_visit::walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &ts::NewExpression<'a>) {
        self.take_arguments(&it.callee, &it.arguments);
        oxc_ast_visit::walk::walk_new_expression(self, it);
    }
}

/// The path builder a callee names. A member form is read only through a plain
/// object identifier (`path.resolve`); `issue.path.join` is an array's own method.
fn path_callee<'a>(expr: &ts::Expression<'a>) -> Option<&'a str> {
    let name = match expr {
        ts::Expression::Identifier(id) => id.name.as_str(),
        ts::Expression::StaticMemberExpression(member) => {
            matches!(member.object, ts::Expression::Identifier(_))
                .then(|| member.property.name.as_str())?
        }
        _ => return None,
    };
    PATH_CALLEES.contains(&name).then_some(name)
}
