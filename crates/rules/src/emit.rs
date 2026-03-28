use std::collections::HashMap;

use regex::Regex;
use sprefa_extract::RawRef;

use crate::types::{Action, EmitRef, ValuePattern};
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

/// Turn a match result into RawRefs according to the action's emit list.
pub fn emit_refs(
    result: &MatchResult,
    action: &Action,
    value_pattern: Option<&ValuePattern>,
    rule_name: &str,
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

    for emit in &action.emit {
        if let Some(raw) = emit_one(emit, rule_name, &captures, node_path.as_deref()) {
            refs.push(raw);
        }
    }

    refs
}

fn emit_one(emit: &EmitRef, rule_name: &str, captures: &HashMap<String, CapturedValue>, node_path: Option<&str>) -> Option<RawRef> {
    let cv = captures.get(&emit.capture)?;

    let parent_key = emit
        .parent
        .as_ref()
        .and_then(|p| captures.get(p))
        .map(|pv| pv.text.clone());

    Some(RawRef {
        value: cv.text.clone(),
        span_start: cv.span_start,
        span_end: cv.span_end,
        kind: emit.kind.to_kind_str().to_string(),
        rule_name: rule_name.to_string(),
        is_path: false,
        parent_key,
        node_path: node_path.map(String::from),
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
