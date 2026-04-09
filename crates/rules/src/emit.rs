use std::collections::HashMap;

use regex::Regex;
use sprefa_extract::RawRef;

use crate::pattern::{match_segments_pub, parse_segment_pattern};
use crate::types::{LineMatcher, MatchDef};
use crate::walk::{CapturedValue, MatchResult};

/// Apply a line matcher (segment capture or regex) to the captures,
/// merging extracted groups back in.
pub fn apply_line_matcher(
    captures: &mut HashMap<String, CapturedValue>,
    matcher: &LineMatcher,
) -> bool {
    let (source, result) = match matcher {
        LineMatcher::Segments { source, pattern } => {
            let source_text = match captures.get(source) {
                Some(cv) => cv.text.clone(),
                None => return false,
            };
            let segments = parse_segment_pattern(pattern);
            (source, match_segments_pub(&segments, &source_text))
        }
        LineMatcher::Regex {
            source,
            pattern,
            full_match,
        } => {
            let source_text = match captures.get(source) {
                Some(cv) => cv.text.clone(),
                None => return false,
            };
            let re = match if *full_match {
                Regex::new(&format!("^(?:{})$", pattern))
            } else {
                Regex::new(pattern)
            } {
                Ok(r) => r,
                Err(_) => return false,
            };
            let re_caps = match re.captures(&source_text) {
                Some(c) => c,
                None => return false,
            };
            let mut map = HashMap::new();
            for name in re.capture_names().flatten() {
                if let Some(m) = re_caps.name(name) {
                    map.insert(name.to_string(), m.as_str().to_string());
                }
            }
            (source, if map.is_empty() { None } else { Some(map) })
        }
    };

    let _ = source; // used above in both arms

    match result {
        Some(extracted) => {
            for (name, text) in extracted {
                captures.insert(
                    name,
                    CapturedValue {
                        text,
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            true
        }
        None => false,
    }
}

/// Turn a match result into RawRefs according to the create_matches list.
/// `group` tags all refs from this extraction site so they form one row in per-rule tables.
pub fn create_refs(
    result: &MatchResult,
    match_defs: &[MatchDef],
    line_matcher: Option<&LineMatcher>,
    rule_name: &str,
    group: Option<u32>,
) -> Vec<RawRef> {
    let mut captures = result.captures.clone();

    // Apply line matcher if present (segment capture or regex filter + capture split)
    if let Some(lm) = line_matcher {
        if !apply_line_matcher(&mut captures, lm) {
            return vec![]; // line matcher didn't match, no refs
        }
    }

    let node_path = if result.path.is_empty() {
        None
    } else {
        Some(result.path.join("/"))
    };

    let mut refs = vec![];

    for def in match_defs {
        if let Some(raw) = create_one(def, rule_name, &captures, node_path.as_deref(), group) {
            refs.push(raw);
        }
    }

    refs
}

fn create_one(
    def: &MatchDef,
    rule_name: &str,
    captures: &HashMap<String, CapturedValue>,
    node_path: Option<&str>,
    group: Option<u32>,
) -> Option<RawRef> {
    let cv = captures.get(&def.capture)?;

    let parent_key = def
        .parent
        .as_ref()
        .and_then(|p| captures.get(p))
        .map(|pv| pv.text.clone());

    Some(RawRef {
        value: cv.text.clone(),
        span_start: cv.span_start,
        span_end: cv.span_end,
        kind: def.kind.clone(),
        rule_name: rule_name.to_string(),
        is_path: false,
        parent_key,
        node_path: node_path.map(String::from),
        scan: def.scan.clone(),
        group,
    })
}

/// Expand a template string like `"{repo}"` using captures.
pub fn expand_template(template: &str, captures: &HashMap<String, CapturedValue>) -> String {
    let mut result = template.to_string();
    for (name, cv) in captures {
        result = result.replace(&format!("{{{}}}", name), &cv.text);
    }
    result
}
