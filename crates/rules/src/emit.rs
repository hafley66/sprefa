use std::collections::HashMap;

use regex::Regex;
use sprefa_extract::RawRef;

use crate::types::{MatchDef, ValuePattern};
use crate::walk::{CapturedValue, MatchResult};

/// Apply a value pattern (regex) to the captures, merging named groups back in.
pub fn apply_value_pattern(
    captures: &mut HashMap<String, CapturedValue>,
    pattern: &ValuePattern,
) -> bool {
    let source_text = match captures.get(&pattern.source) {
        Some(cv) => cv.text.clone(),
        None => return false, // source capture doesn't exist, skip
    };

    let re = match if pattern.full_match {
        Regex::new(&format!("^(?:{})$", &pattern.pattern))
    } else {
        Regex::new(&pattern.pattern)
    } {
        Ok(r) => r,
        Err(_) => return false, // bad regex, skip
    };

    let re_caps = match re.captures(&source_text) {
        Some(c) => c,
        None => return false, // no match, skip this node
    };

    // Merge named groups into the capture map
    for name in re.capture_names().flatten() {
        if let Some(m) = re_caps.name(name) {
            captures.insert(
                name.to_string(),
                CapturedValue {
                    text: m.as_str().to_string(),
                    span_start: 0,
                    span_end: 0,
                },
            );
        }
    }

    true
}

/// Turn a match result into RawRefs according to the create_matches list.
/// `group` tags all refs from this extraction site so they share a group_id in the DB.
pub fn create_refs(
    result: &MatchResult,
    match_defs: &[MatchDef],
    value_pattern: Option<&ValuePattern>,
    rule_name: &str,
    group: Option<u32>,
) -> Vec<RawRef> {
    let mut captures = result.captures.clone();

    // Apply value pattern if present (regex filter + capture split)
    if let Some(vp) = value_pattern {
        if !apply_value_pattern(&mut captures, vp) {
            return vec![]; // value pattern didn't match, no refs
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

fn create_one(def: &MatchDef, rule_name: &str, captures: &HashMap<String, CapturedValue>, node_path: Option<&str>, group: Option<u32>) -> Option<RawRef> {
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
pub fn expand_template(
    template: &str,
    captures: &HashMap<String, CapturedValue>,
) -> String {
    let mut result = template.to_string();
    for (name, cv) in captures {
        result = result.replace(&format!("{{{}}}", name), &cv.text);
    }
    result
}
