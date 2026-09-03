//! One library entrypoint for source queries backed by tree-sitter or ast-grep.
//!
//! This facade preserves each engine's current result shape. Canonical source
//! occurrence, match, and capture facts belong to the later normalization
//! boundary and are intentionally absent here.

use std::collections::BTreeMap;
use std::str::FromStr;

use ast_grep_language::{LanguageExt, SupportLang};
use serde_json::Value;
use tree_sitter::{
    Parser as TreeParser, Query, QueryCursor, QueryPredicate, QueryPredicateArg, StreamingIterator,
};

use super::{
    query_ast_rule, query_patterns, AstCaptureFact, AstPatternQuery, AstRuleError, AstRuleMatch,
    AstRuleRequest,
};
use crate::seams::ParseError;

/// A tree-sitter query keeps the native S-expression and explicit grammar name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeSitterQuery {
    pub language: String,
    pub query: String,
}

/// The source query algebras currently hosted by sprefa-extract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceQuery {
    TreeSitter(TreeSitterQuery),
    AstPatterns(Vec<AstPatternQuery>),
    AstRule(AstRuleRequest),
}

/// The existing tree-sitter CLI row: capture names map to captured text, with
/// one-based `line` and `end_line` fields in the same top-level object.
pub type TreeSitterQueryMatch = BTreeMap<String, Value>;

/// Results remain engine-shaped until the common match-fact schema is reviewed.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceQueryOutput {
    TreeSitter(Vec<TreeSitterQueryMatch>),
    AstPatterns(Vec<AstCaptureFact>),
    AstRule(Vec<AstRuleMatch>),
}

#[derive(Debug)]
pub enum SourceQueryError {
    TreeSitter(String),
    AstPatterns(ParseError),
    AstRule(AstRuleError),
}

impl std::fmt::Display for SourceQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TreeSitter(error) => formatter.write_str(error),
            Self::AstPatterns(error) => std::fmt::Display::fmt(error, formatter),
            Self::AstRule(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for SourceQueryError {}

/// Dispatch one query against caller-owned source bytes.
pub fn query_source(
    path: &str,
    content: &[u8],
    query: &SourceQuery,
) -> Result<SourceQueryOutput, SourceQueryError> {
    match query {
        SourceQuery::TreeSitter(query) => query_tree_sitter(content, query)
            .map(SourceQueryOutput::TreeSitter)
            .map_err(SourceQueryError::TreeSitter),
        SourceQuery::AstPatterns(queries) => query_patterns(path, content, queries)
            .map(SourceQueryOutput::AstPatterns)
            .map_err(SourceQueryError::AstPatterns),
        SourceQuery::AstRule(request) => query_ast_rule(path, content, request)
            .map(SourceQueryOutput::AstRule)
            .map_err(SourceQueryError::AstRule),
    }
}

pub fn query_tree_sitter(
    content: &[u8],
    request: &TreeSitterQuery,
) -> Result<Vec<TreeSitterQueryMatch>, String> {
    let language = query_language(&request.language)?;
    let source = std::str::from_utf8(content)
        .map_err(|error| format!("query input is not valid UTF-8: {error}"))?;
    let mut parser = TreeParser::new();
    parser
        .set_language(&language)
        .map_err(|error| format!("invalid language '{}': {error:?}", request.language))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "query parse failed: source tree was not produced".to_string())?;
    let query_text = rewrite_predicates(&request.query);
    let query = Query::new(&language, &query_text).map_err(|error| {
        one_line_text(format!(
            "invalid query at row {}: {error}",
            error.row.saturating_add(1)
        ))
    })?;
    validate_predicates(&query)?;
    collect_matches(&query, tree.root_node(), source.as_bytes())
}

fn query_language(name: &str) -> Result<tree_sitter::Language, String> {
    let language = match name {
        "md" => return Ok(tree_sitter::Language::new(tree_sitter_md::LANGUAGE)),
        "md_inline" => return Ok(tree_sitter::Language::new(tree_sitter_md::INLINE_LANGUAGE)),
        "html" => return Ok(tree_sitter::Language::new(tree_sitter_html::LANGUAGE)),
        "dl6" => return Ok(tree_sitter::Language::new(tree_sitter_dl6::LANGUAGE)),
        "rust" => SupportLang::from_str("rust"),
        "ts" => SupportLang::from_str("ts"),
        "tsx" => SupportLang::from_str("tsx"),
        "js" => SupportLang::from_str("js"),
        "go" => SupportLang::from_str("go"),
        "kotlin" => SupportLang::from_str("kotlin"),
        _ => return Err(format!("unknown lang '{name}'")),
    }
    .map_err(|_| format!("unknown lang '{name}'"))?;
    Ok(language.get_ts_language())
}

fn validate_predicates(query: &Query) -> Result<(), String> {
    for pattern in 0..query.pattern_count() {
        for predicate in query.general_predicates(pattern) {
            match predicate.operator.as_ref() {
                "sprefa-match?" | "sprefa-not-match?" => validate_match_predicate(predicate)?,
                "sprefa-eq?" => validate_eq_predicate(predicate)?,
                operator => {
                    return Err(format!(
                        "invalid query: predicate #{operator} is not allowed"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_match_predicate(predicate: &QueryPredicate) -> Result<(), String> {
    let [QueryPredicateArg::Capture(_), QueryPredicateArg::String(pattern)] = &*predicate.args
    else {
        return Err(format!(
            "invalid query: predicate #{} expects a capture and a string",
            predicate.operator
        ));
    };
    regex::bytes::Regex::new(pattern)
        .map(|_| ())
        .map_err(|error| format!("invalid query: regex: {error}"))
}

fn validate_eq_predicate(predicate: &QueryPredicate) -> Result<(), String> {
    match &*predicate.args {
        [QueryPredicateArg::Capture(_), QueryPredicateArg::Capture(_)]
        | [QueryPredicateArg::Capture(_), QueryPredicateArg::String(_)] => Ok(()),
        _ => Err(format!(
            "invalid query: predicate #{} expects two arguments",
            predicate.operator
        )),
    }
}

fn collect_matches(
    query: &Query,
    root: tree_sitter::Node<'_>,
    source: &[u8],
) -> Result<Vec<TreeSitterQueryMatch>, String> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source);
    let mut rows = Vec::new();
    while let Some(found) = matches.next() {
        if found.captures.is_empty() || !matches_predicates(query, found, source)? {
            continue;
        }
        let mut captures = BTreeMap::new();
        let mut line = i64::MAX;
        let mut end_line = 1;
        for capture in found.captures {
            let node = capture.node;
            let name = names[capture.index as usize];
            let text = node
                .utf8_text(source)
                .map_err(|error| format!("query capture text: {error}"))?;
            captures.insert(name.to_string(), Value::String(text.to_string()));
            line = line.min(node.start_position().row as i64 + 1);
            end_line = end_line.max(node.end_position().row as i64 + 1);
        }
        captures.insert(
            "line".to_string(),
            Value::from(if line == i64::MAX { 1 } else { line }),
        );
        captures.insert("end_line".to_string(), Value::from(end_line));
        rows.push(captures);
    }
    Ok(rows)
}

fn matches_predicates(
    query: &Query,
    found: &tree_sitter::QueryMatch<'_, '_>,
    source: &[u8],
) -> Result<bool, String> {
    let direct = query.general_predicates(found.pattern_index);
    if !direct.is_empty() {
        return direct.iter().try_fold(true, |matched, predicate| {
            Ok(matched && predicate_matches(predicate, found, source)?)
        });
    }
    for pattern in 0..query.pattern_count() {
        if pattern != found.pattern_index {
            for predicate in query.general_predicates(pattern) {
                if !predicate_matches(predicate, found, source)? {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

fn predicate_matches(
    predicate: &QueryPredicate,
    found: &tree_sitter::QueryMatch<'_, '_>,
    source: &[u8],
) -> Result<bool, String> {
    let capture_texts = |index: u32| {
        found
            .captures
            .iter()
            .filter(|capture| capture.index == index)
            .map(|capture| capture.node.utf8_text(source).unwrap_or("").as_bytes())
            .collect::<Vec<_>>()
    };
    match predicate.operator.as_ref() {
        "sprefa-match?" | "sprefa-not-match?" => {
            let [QueryPredicateArg::Capture(index), QueryPredicateArg::String(pattern)] =
                &*predicate.args
            else {
                return Ok(false);
            };
            let regex = regex::bytes::Regex::new(pattern)
                .map_err(|error| format!("invalid query: regex: {error}"))?;
            let matches = capture_texts(*index)
                .iter()
                .all(|text| regex.is_match(text));
            Ok(if predicate.operator.as_ref() == "sprefa-match?" {
                matches
            } else {
                !matches
            })
        }
        "sprefa-eq?" => {
            let [QueryPredicateArg::Capture(left), right] = &*predicate.args else {
                return Ok(false);
            };
            let left = capture_texts(*left);
            let right = match right {
                QueryPredicateArg::Capture(index) => capture_texts(*index),
                QueryPredicateArg::String(value) => vec![value.as_bytes()],
            };
            Ok(left.len() == right.len() && left.iter().zip(right).all(|(a, b)| *a == b))
        }
        _ => Ok(false),
    }
}

fn rewrite_predicates(query: &str) -> String {
    let mut output = String::with_capacity(query.len());
    let mut index = 0;
    let mut quoted = false;
    let mut escaped = false;
    while index < query.len() {
        let rest = &query[index..];
        if !quoted {
            if let Some((from, to)) = [
                ("#not-match?", "#sprefa-not-match?"),
                ("#match?", "#sprefa-match?"),
                ("#eq?", "#sprefa-eq?"),
            ]
            .into_iter()
            .find(|(from, _)| rest.starts_with(from))
            {
                output.push_str(to);
                index += from.len();
                continue;
            }
        }
        let character = rest.chars().next().unwrap();
        output.push(character);
        index += character.len_utf8();
        if quoted && character == '"' && !escaped {
            quoted = false;
        } else if !quoted && character == '"' {
            quoted = true;
        }
        escaped = quoted && character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    output
}

fn one_line_text(text: String) -> String {
    text.lines()
        .next()
        .unwrap_or("invalid query command")
        .to_string()
}
