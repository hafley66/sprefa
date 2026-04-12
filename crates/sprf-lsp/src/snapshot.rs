//! Document Snapshot: Pre-computed document state for fast LSP operations.
//!
//! This module provides a snapshot of a document's structure that can be
//! computed once and used for fast hover, completion, and goto-definition
//! without re-parsing during request handlers.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use sprefa_sprf::_0_ast::{RuleBody, Statement, Tag};
use sprefa_sprf::_1_parse::parse_program;

/// A pre-computed snapshot of a document's structure.
/// Computed once on document change, used for fast read-only operations.
#[derive(Clone, Debug)]
pub struct DocumentSnapshot {
    /// Document URI
    pub uri: String,
    /// Full document text
    pub text: String,
    /// Document version
    pub version: i32,

    /// All capture variables with their positions
    pub captures: Vec<CaptureInfo>,
    /// All rules with their spans and captures
    pub rules: Vec<RuleSnapshot>,
    /// Cross-references (derived_rule -> base_rule)
    pub cross_refs: Vec<CrossRefInfo>,
    /// All capture names (for completion)
    pub all_capture_names: HashSet<String>,
    /// All rule names (for completion)
    pub all_rule_names: Vec<String>,
    /// Check names
    pub check_names: Vec<String>,
}

/// Information about a capture variable at a specific position.
#[derive(Clone, Debug)]
pub struct CaptureInfo {
    /// Capture name (without $ prefix)
    pub name: String,
    /// Rule that defines this capture
    pub rule_name: String,
    /// Byte offset in source where capture appears
    pub offset: usize,
    /// Byte range of the capture ($NAME)
    pub range: Range<usize>,
    /// Type of capture occurrence
    pub kind: CaptureKind,
}

/// Type of capture occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    /// Capture in a pattern (e.g., json({ name: $NAME }))
    Pattern,
    /// Capture in a cross-ref binding (e.g., base_rule(name: $NAME))
    CrossRefBinding,
    /// Definition-style capture (left side of binding)
    BindingTarget,
}

/// Snapshot of a rule's structure.
#[derive(Clone, Debug)]
pub struct RuleSnapshot {
    /// Rule name
    pub name: String,
    /// Byte range of the entire rule statement
    pub span: Range<usize>,
    /// Captures defined within this rule
    pub captures: Vec<String>,
    /// Cross-references made by this rule
    pub cross_refs: Vec<CrossRefInfo>,
}

/// Information about a cross-reference.
#[derive(Clone, Debug)]
pub struct CrossRefInfo {
    /// Target rule name (e.g., "base_rule" in `base_rule(repo: $REPO)`)
    pub target_rule: String,
    /// Parameter bindings: [(param_name, capture_var)]
    /// e.g., [("repo", "REPO"), ("tag", "TAG")]
    pub bindings: Vec<(String, String)>,
    /// Byte range of the cross-reference
    pub span: Range<usize>,
    /// Rule that contains this cross-reference
    pub containing_rule: String,
}

impl DocumentSnapshot {
    /// Compute a snapshot from document text.
    pub fn compute(uri: &str, text: &str, version: i32) -> Self {
        // Try to parse the document
        let program = match parse_program(text) {
            Ok(p) => p,
            Err(_) => {
                // Parse failed - return empty snapshot
                // We'll still try to extract captures via regex as fallback
                return Self::compute_fallback(uri, text, version);
            }
        };

        // Build statement spans
        let stmt_spans = find_statement_spans(text);

        let mut captures = Vec::new();
        let mut rules = Vec::new();
        let mut cross_refs = Vec::new();
        let mut all_capture_names = HashSet::new();
        let mut all_rule_names = Vec::new();
        let mut check_names = Vec::new();

        for (idx, stmt) in program.iter().enumerate() {
            let span = stmt_spans
                .get(idx)
                .cloned()
                .unwrap_or(0..text.len());

            match stmt {
                Statement::Rule(decl) => {
                    all_rule_names.push(decl.name.clone());

                    let mut rule_captures = Vec::new();
                    let mut rule_cross_refs = Vec::new();

                    // Process each body element
                    for body in &decl.body {
                        collect_body_info(
                            body,
                            text,
                            &decl.name,
                            span.start,
                            &mut captures,
                            &mut rule_captures,
                            &mut rule_cross_refs,
                            &mut all_capture_names,
                        );
                    }

                    // Add cross_refs to global list
                    cross_refs.extend(rule_cross_refs.clone());

                    rules.push(RuleSnapshot {
                        name: decl.name.clone(),
                        span,
                        captures: rule_captures,
                        cross_refs: rule_cross_refs,
                    });
                }
                Statement::Check(decl) => {
                    check_names.push(decl.name.clone());
                }
            }
        }

        Self {
            uri: uri.to_string(),
            text: text.to_string(),
            version,
            captures,
            rules,
            cross_refs,
            all_capture_names,
            all_rule_names,
            check_names,
        }
    }

    /// Fallback snapshot computation when parsing fails.
    /// Uses regex to extract captures and rules.
    fn compute_fallback(uri: &str, text: &str, version: i32) -> Self {
        // Simple regex-based extraction
        let mut captures = Vec::new();
        let mut rules = Vec::new();
        let mut cross_refs = Vec::new();
        let mut all_capture_names = HashSet::new();
        let mut all_rule_names = Vec::new();
        let mut check_names = Vec::new();

        // Extract rule declarations: rule(name) { ... }
        for cap in regex::Regex::new(r"rule\s*\(\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\)")
            .unwrap()
            .captures_iter(text)
        {
            let name = cap[1].to_string();
            let start = cap.get(0).unwrap().start();
            let end = find_rule_end(text, start);
            all_rule_names.push(name.clone());
            rules.push(RuleSnapshot {
                name,
                span: start..end,
                captures: Vec::new(),
                cross_refs: Vec::new(),
            });
        }

        // Extract captures: $NAME
        for cap in regex::Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)")
            .unwrap()
            .captures_iter(text)
        {
            let name = cap[1].to_string();
            let range = cap.get(0).unwrap().range();
            all_capture_names.insert(name.clone());

            // Find which rule this capture is in
            let rule_name = rules
                .iter()
                .find(|r| r.span.start <= range.start && range.end <= r.span.end)
                .map(|r| r.name.clone())
                .unwrap_or_default();

            captures.push(CaptureInfo {
                name,
                rule_name,
                offset: range.start,
                range,
                kind: CaptureKind::Pattern, // Default, can't determine precisely without parse
            });
        }

        // Extract check declarations
        for cap in regex::Regex::new(r"check\s*\(\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\)")
            .unwrap()
            .captures_iter(text)
        {
            check_names.push(cap[1].to_string());
        }

        Self {
            uri: uri.to_string(),
            text: text.to_string(),
            version,
            captures,
            rules,
            cross_refs,
            all_capture_names,
            all_rule_names,
            check_names,
        }
    }

    /// Find a capture at the given byte offset.
    /// Returns the capture if the offset is within or adjacent to the capture range.
    pub fn capture_at(&self, offset: usize) -> Option<&CaptureInfo> {
        // First try exact containment
        let exact = self.captures.iter()
            .find(|c| c.range.start <= offset && offset <= c.range.end);
        
        if exact.is_some() {
            return exact;
        }
        
        // Try finding closest capture within a small window (e.g., cursor near but not exactly on)
        self.captures.iter()
            .min_by_key(|c| {
                if offset < c.range.start {
                    c.range.start - offset
                } else if offset > c.range.end {
                    offset - c.range.end
                } else {
                    0
                }
            })
            .filter(|c| {
                let dist = if offset < c.range.start {
                    c.range.start - offset
                } else if offset > c.range.end {
                    offset - c.range.end
                } else {
                    0
                };
                dist <= 2 // Within 2 bytes
            })
    }

    /// Find captures in a specific rule.
    pub fn captures_in_rule(&self, rule_name: &str) -> Vec<&CaptureInfo> {
        self.captures
            .iter()
            .filter(|c| c.rule_name == rule_name)
            .collect()
    }

    /// Find the rule that contains the given offset.
    pub fn rule_at(&self, offset: usize) -> Option<&RuleSnapshot> {
        self.rules.iter().find(|r| r.span.start <= offset && offset <= r.span.end)
    }

    /// Find rule by name.
    pub fn find_rule(&self, name: &str) -> Option<&RuleSnapshot> {
        self.rules.iter().find(|r| r.name == name)
    }

    /// Get captures available at a position (scoped to enclosing rule).
    pub fn available_captures_at(&self, offset: usize) -> Vec<String> {
        if let Some(rule) = self.rule_at(offset) {
            // Return captures from this rule plus inherited captures
            let mut caps: Vec<String> = rule.captures.clone();
            
            // Also add captures from cross-ref bindings that are in scope
            for cross_ref in &rule.cross_refs {
                for (_, var) in &cross_ref.bindings {
                    if !caps.contains(var) {
                        caps.push(var.clone());
                    }
                }
            }
            
            caps
        } else {
            // Not in a rule, return all captures
            self.all_capture_names.iter().cloned().collect()
        }
    }

    /// Find cross-refs that target a specific rule.
    pub fn cross_refs_to(&self, target_rule: &str) -> Vec<&CrossRefInfo> {
        self.cross_refs
            .iter()
            .filter(|cr| cr.target_rule == target_rule)
            .collect()
    }

    /// Find cross-ref at a given offset.
    pub fn cross_ref_at(&self, offset: usize) -> Option<&CrossRefInfo> {
        self.cross_refs
            .iter()
            .find(|cr| cr.span.start <= offset && offset <= cr.span.end)
    }
}

/// Collect information from a rule body.
fn collect_body_info(
    body: &RuleBody,
    text: &str,
    rule_name: &str,
    rule_offset: usize,
    captures: &mut Vec<CaptureInfo>,
    rule_captures: &mut Vec<String>,
    rule_cross_refs: &mut Vec<CrossRefInfo>,
    all_capture_names: &mut HashSet<String>,
) {
    match body {
        RuleBody::Step(slot) => {
            for cap in slot.captures() {
                // Find the range of this capture in the text
                if let Some(range) = find_capture_range(text, rule_offset, &cap) {
                    all_capture_names.insert(cap.clone());
                    rule_captures.push(cap.clone());
                    captures.push(CaptureInfo {
                        name: cap,
                        rule_name: rule_name.to_string(),
                        offset: range.start,
                        range,
                        kind: CaptureKind::Pattern,
                    });
                }
            }
        }
        RuleBody::Block {
            slot, children, ..
        } => {
            for cap in slot.captures() {
                if let Some(range) = find_capture_range(text, rule_offset, &cap) {
                    all_capture_names.insert(cap.clone());
                    rule_captures.push(cap.clone());
                    captures.push(CaptureInfo {
                        name: cap,
                        rule_name: rule_name.to_string(),
                        offset: range.start,
                        range,
                        kind: CaptureKind::Pattern,
                    });
                }
            }
            for child in children {
                collect_body_info(
                    child,
                    text,
                    rule_name,
                    rule_offset,
                    captures,
                    rule_captures,
                    rule_cross_refs,
                    all_capture_names,
                );
            }
        }
        RuleBody::Ref {
            cross_ref,
            children,
        } => {
            // Record cross-reference
            let mut bindings = Vec::new();
            for binding in &cross_ref.bindings {
                all_capture_names.insert(binding.var.clone());
                bindings.push((binding.column.clone(), binding.var.clone()));
                
                // Find range of this binding's capture
                if let Some(range) = find_capture_range(text, rule_offset, &binding.var) {
                    captures.push(CaptureInfo {
                        name: binding.var.clone(),
                        rule_name: rule_name.to_string(),
                        offset: range.start,
                        range,
                        kind: CaptureKind::CrossRefBinding,
                    });
                }
            }

            // Find span of the cross-ref
            let span = find_cross_ref_span(text, rule_offset, &cross_ref.rule_name);
            
            let cross_ref_info = CrossRefInfo {
                target_rule: cross_ref.rule_name.clone(),
                bindings,
                span,
                containing_rule: rule_name.to_string(),
            };
            
            rule_cross_refs.push(cross_ref_info.clone());

            for child in children {
                collect_body_info(
                    child,
                    text,
                    rule_name,
                    rule_offset,
                    captures,
                    rule_captures,
                    rule_cross_refs,
                    all_capture_names,
                );
            }
        }
    }
}

/// Find the byte range of a capture in the text.
/// Returns (start, end) where start is the position of `$` and end is after the name.
fn find_capture_range(text: &str, start_offset: usize, cap_name: &str) -> Option<Range<usize>> {
    let search_text = &text[start_offset..];
    let pattern = format!("${}", cap_name);
    search_text.find(&pattern).map(|pos| {
        let start = start_offset + pos;
        let end = start + pattern.len();
        start..end
    })
}

/// Find the byte position of a capture in the text (legacy, returns start position).
fn find_capture_position(text: &str, start_offset: usize, cap_name: &str) -> Option<usize> {
    find_capture_range(text, start_offset, cap_name).map(|r| r.start)
}

/// Find the byte span of a cross-reference.
fn find_cross_ref_span(text: &str, start_offset: usize, rule_name: &str) -> Range<usize> {
    let search_text = &text[start_offset..];
    // Look for rule_name followed by (
    let pattern = format!("{}", rule_name);
    if let Some(pos) = search_text.find(&pattern) {
        let start = start_offset + pos;
        // Find matching )
        let after = &search_text[pos..];
        let mut depth = 0;
        for (i, ch) in after.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return start..start_offset + pos + i + 1;
                    }
                }
                _ => {}
            }
        }
        start..start + pattern.len()
    } else {
        start_offset..start_offset
    }
}

/// Find the end of a rule (closing brace or semicolon).
fn find_rule_end(text: &str, start: usize) -> usize {
    let rest = &text[start..];
    let mut depth = 0;
    for (i, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return start + i + 1;
                }
            }
            ';' if depth == 0 => return start + i + 1,
            _ => {}
        }
    }
    text.len()
}

/// Find byte ranges for each statement.
fn find_statement_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = vec![];
    let mut start = 0;
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;

    for (i, ch) in text.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ';' if brace_depth <= 0 && paren_depth <= 0 => {
                spans.push(start..i);
                start = i + 1;
            }
            _ => {}
        }
    }
    // Trailing statement without semicolon
    let trailing = text[start..].trim();
    if !trailing.is_empty() && !trailing.starts_with('#') {
        spans.push(start..text.len());
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_basic() {
        let text = r#"
rule(test) {
    fs(**/*.json) > json({ name: $NAME })
}
"#;
        let snapshot = DocumentSnapshot::compute("file://test.sprf", text, 1);

        assert_eq!(snapshot.all_rule_names, vec!["test"]);
        assert!(snapshot.all_capture_names.contains("NAME"));
        
        let caps = snapshot.captures_in_rule("test");
        assert!(!caps.is_empty());
        assert_eq!(caps[0].name, "NAME");
    }

    #[test]
    fn test_cross_ref_detection() {
        let text = r#"rule(base) {
    fs(**/*.yaml) > json({ image: $IMG })
};
rule(derived) {
    base(img: $IMG) {
        fs(**/*.json)
    }
};
"#;
        let snapshot = DocumentSnapshot::compute("file://test.sprf", text, 1);

        let cross_refs = snapshot.cross_refs_to("base");
        assert_eq!(cross_refs.len(), 1);
        assert_eq!(cross_refs[0].target_rule, "base");
        assert_eq!(cross_refs[0].bindings, vec![("img".to_string(), "IMG".to_string())]);
    }

    #[test]
    fn test_capture_at_position() {
        let text = r#"rule(test) { json({ name: $NAME }) }"#;
        let snapshot = DocumentSnapshot::compute("file://test.sprf", text, 1);

        // Find the $NAME position
        let name_pos = text.find("$NAME").unwrap();
        let cap = snapshot.capture_at(name_pos);
        assert!(cap.is_some());
        assert_eq!(cap.unwrap().name, "NAME");
    }
}
