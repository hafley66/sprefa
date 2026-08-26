use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ast_grep_language::{LanguageExt, SupportLang};
use clap::Parser;
use serde_json::{Map, Value};
use tree_sitter::{
    Parser as TreeParser, Query, QueryCursor, QueryPredicate, QueryPredicateArg, StreamingIterator,
};

#[derive(Parser)]
#[command(name = "extract query")]
struct QueryCli {
    #[arg(long)]
    lang: String,
    #[arg(long)]
    query: String,
    #[arg(long)]
    digest: Option<String>,
    path: PathBuf,
}

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let cli = QueryCli::try_parse_from(args).map_err(one_line)?;
    let language = query_language(&cli.lang)?;
    let bytes = source_bytes(&cli.path, cli.digest.as_deref())?;
    let source = std::str::from_utf8(&bytes)
        .map_err(|error| format!("query input '{}': {error}", cli.path.display()))?;
    let mut parser = TreeParser::new();
    parser
        .set_language(&language)
        .map_err(|error| format!("invalid language '{}': {error:?}", cli.lang))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "query parse failed: source tree was not produced".to_string())?;
    let query_text = rewrite_predicates(&cli.query);
    let query = Query::new(&language, &query_text).map_err(|error| {
        one_line_text(format!(
            "invalid query at row {}: {error}",
            error.row.saturating_add(1)
        ))
    })?;
    validate_predicates(&query)?;
    stream_matches(&query, tree.root_node(), source.as_bytes())
}

fn source_bytes(path: &PathBuf, digest: Option<&str>) -> Result<Vec<u8>, String> {
    match digest {
        Some(oid) => cat_blob(path, oid),
        None => std::fs::read(path)
            .map_err(|error| format!("query input '{}': {error}", path.display())),
    }
}

fn cat_blob(path: &Path, oid: &str) -> Result<Vec<u8>, String> {
    let repository = soopy::discover(path.parent().unwrap_or(path))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let mut batch = soopy::GitBatch::open(&repository.root)
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    let bytes = batch
        .read(&soopy::ObjectId(oid.into()))
        .map_err(|error| one_line_text(format!("git cat-file blob {oid}: {error}")))?;
    Ok(bytes.to_vec())
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
                "sprefa-match?" | "sprefa-not-match?" => {
                    validate_match_predicate(predicate)?;
                }
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

fn stream_matches(query: &Query, root: tree_sitter::Node<'_>, source: &[u8]) -> Result<(), String> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, source);
    while let Some(found) = matches.next() {
        if found.captures.is_empty() {
            continue;
        }
        if !matches_predicates(query, found, source)? {
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
        // Sorted through the BTreeMap, never through `Map`: a dependency turning
        // `serde_json/preserve_order` on otherwise reorders every row.
        let object: Map<String, Value> = captures.into_iter().collect();
        println!("{}", Value::Object(object));
    }
    Ok(())
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

fn one_line(error: clap::Error) -> String {
    one_line_text(error.to_string())
}

fn one_line_text(text: String) -> String {
    text.lines()
        .next()
        .unwrap_or("invalid query command")
        .to_string()
}
