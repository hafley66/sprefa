/// Comment-bounded region extraction for marker() scoping.
///
/// Finds comment nodes in source code via tree-sitter (or line-based fallback),
/// matches them against open/close regex patterns, and produces byte ranges
/// that downstream matchers are constrained to.
use std::collections::HashMap;
use std::path::Path;

use ast_grep_core::Language;
use ast_grep_language::{LanguageExt, SupportLang};
use regex::Regex;

use crate::types::MarkerScope;
use crate::walk::CapturedValue;

/// A matched region produced by marker scoping.
pub struct MarkerRegion {
    pub start: usize,
    pub end: usize,
    /// Captures extracted from the marker comment (e.g. region name).
    pub captures: HashMap<String, CapturedValue>,
}

/// A comment node extracted from source.
struct CommentNode {
    text: String,
    byte_start: usize,
    byte_end: usize,
}

/// Find marker-bounded regions in source code.
///
/// Returns a list of byte ranges (and any captures from marker text).
/// If no markers match, returns empty vec (caller should treat as "whole file").
pub fn find_marker_regions(
    source: &[u8],
    path: &str,
    scope: &MarkerScope,
) -> Vec<MarkerRegion> {
    let src = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let open_re = match Regex::new(&scope.open) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let close_re = scope.close.as_ref().and_then(|p| Regex::new(p).ok());

    let comments = extract_comments(source, path, src);
    if comments.is_empty() {
        return vec![];
    }

    match &close_re {
        Some(close) => paired_regions(&comments, &open_re, close, &scope.capture, source.len()),
        None => sequential_regions(&comments, &open_re, &scope.capture, source.len()),
    }
}

/// Extract comment nodes using tree-sitter when available, line-prefix fallback otherwise.
fn extract_comments(_source: &[u8], path: &str, src: &str) -> Vec<CommentNode> {
    if let Some(lang) = SupportLang::from_path(Path::new(path)) {
        extract_comments_treesitter(src, lang)
    } else {
        extract_comments_line_prefix(src)
    }
}

/// Use tree-sitter to find all comment nodes.
fn extract_comments_treesitter(src: &str, lang: SupportLang) -> Vec<CommentNode> {
    let root = lang.ast_grep(src);
    let node = root.root();
    let mut comments = vec![];
    collect_comment_nodes(&node, src, &mut comments);
    comments
}

/// Recursively collect nodes whose kind contains "comment".
fn collect_comment_nodes<D: ast_grep_core::Doc>(
    node: &ast_grep_core::Node<D>,
    src: &str,
    out: &mut Vec<CommentNode>,
) {
    let kind = node.kind();
    if kind.contains("comment") {
        let range = node.range();
        let start = range.start;
        let end = range.end;
        if start < end && end <= src.len() {
            out.push(CommentNode {
                text: src[start..end].to_string(),
                byte_start: start,
                byte_end: end,
            });
        }
    }
    // Recurse into children
    for child in node.children() {
        collect_comment_nodes(&child, src, out);
    }
}

/// Line-prefix fallback: detect lines starting with common comment prefixes.
fn extract_comments_line_prefix(src: &str) -> Vec<CommentNode> {
    let mut comments = vec![];
    let mut offset = 0;
    for line in src.split('\n') {
        let trimmed = line.trim_start();
        let is_comment = trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("--")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("<!--");
        if is_comment {
            comments.push(CommentNode {
                text: line.to_string(),
                byte_start: offset,
                byte_end: offset + line.len(),
            });
        }
        offset += line.len() + 1; // +1 for newline
    }
    comments
}

/// Build regions from paired open/close markers.
/// Supports nesting: each open pushes a stack frame, each close pops.
/// Unpaired opens produce point matches.
fn paired_regions(
    comments: &[CommentNode],
    open_re: &Regex,
    close_re: &Regex,
    capture_name: &Option<String>,
    _source_len: usize,
) -> Vec<MarkerRegion> {
    let mut regions = vec![];
    // Stack of (open_byte_start, open_captures)
    let mut stack: Vec<(usize, HashMap<String, CapturedValue>)> = vec![];

    for comment in comments {
        if let Some(m) = open_re.find(&comment.text) {
            let caps = extract_label_capture(
                &comment.text,
                m.end(),
                capture_name,
                comment.byte_start,
                comment.byte_end,
            );
            stack.push((comment.byte_start, caps));
        } else if close_re.is_match(&comment.text) {
            if let Some((start, caps)) = stack.pop() {
                regions.push(MarkerRegion {
                    start,
                    end: comment.byte_end,
                    captures: caps,
                });
            }
        }
    }

    // Unpaired opens become point matches
    for (start, caps) in stack {
        // Find the end of the comment node that opened this
        let point_end = comments
            .iter()
            .find(|c| c.byte_start == start)
            .map(|c| c.byte_end)
            .unwrap_or(start);
        regions.push(MarkerRegion {
            start,
            end: point_end,
            captures: caps,
        });
    }

    regions.sort_by_key(|r| r.start);
    regions
}

/// Build regions from sequential (flat) markers.
/// Each marker opens a region that ends at the next marker or EOF.
/// A lone marker (last or only) extends to EOF as a region.
fn sequential_regions(
    comments: &[CommentNode],
    open_re: &Regex,
    capture_name: &Option<String>,
    source_len: usize,
) -> Vec<MarkerRegion> {
    let mut opens: Vec<(usize, usize, HashMap<String, CapturedValue>)> = vec![];

    for comment in comments {
        if let Some(m) = open_re.find(&comment.text) {
            let caps = extract_label_capture(
                &comment.text,
                m.end(),
                capture_name,
                comment.byte_start,
                comment.byte_end,
            );
            opens.push((comment.byte_start, comment.byte_end, caps));
        }
    }

    if opens.is_empty() {
        return vec![];
    }

    let mut regions = vec![];
    for i in 0..opens.len() {
        let (start, _, ref caps) = opens[i];
        let end = if i + 1 < opens.len() {
            opens[i + 1].0 // next marker's start
        } else {
            source_len // EOF
        };
        regions.push(MarkerRegion {
            start,
            end,
            captures: caps.clone(),
        });
    }

    regions
}

/// Extract a label capture from comment text after the regex match.
/// The label is the trimmed remainder of the comment text after the matched prefix.
fn extract_label_capture(
    comment_text: &str,
    match_end: usize,
    capture_name: &Option<String>,
    byte_start: usize,
    byte_end: usize,
) -> HashMap<String, CapturedValue> {
    let mut caps = HashMap::new();
    if let Some(name) = capture_name {
        let label = comment_text[match_end..].trim();
        if !label.is_empty() {
            caps.insert(
                name.clone(),
                CapturedValue {
                    text: label.to_string(),
                    span_start: byte_start as u32,
                    span_end: byte_end as u32,
                },
            );
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_flat_markers() {
        let src = b"// SECTION: imports\nuse std::fs;\nuse std::path::Path;\n// SECTION: helpers\nfn cleanup() {}\n";
        let scope = MarkerScope {
            open: "SECTION:".to_string(),
            close: None,
            capture: Some("LABEL".to_string()),
        };
        let regions = find_marker_regions(src, "test.rs", &scope);
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions[0].captures.get("LABEL").map(|c| c.text.as_str()),
            Some("imports")
        );
        assert_eq!(
            regions[1].captures.get("LABEL").map(|c| c.text.as_str()),
            Some("helpers")
        );
        // First region ends where second starts
        assert_eq!(regions[0].end, regions[1].start);
    }

    #[test]
    fn paired_open_close() {
        let src = b"// BEGIN: auth\nfn login() {}\n// END: auth\nfn other() {}\n";
        let scope = MarkerScope {
            open: "BEGIN:".to_string(),
            close: Some("END:".to_string()),
            capture: Some("LABEL".to_string()),
        };
        let regions = find_marker_regions(src, "test.rs", &scope);
        assert_eq!(regions.len(), 1);
        assert_eq!(
            regions[0].captures.get("LABEL").map(|c| c.text.as_str()),
            Some("auth")
        );
        // Region includes the auth section but not fn other()
        let region_text = std::str::from_utf8(&src[regions[0].start..regions[0].end]).unwrap();
        assert!(region_text.contains("login"));
        assert!(!region_text.contains("other"));
    }

    #[test]
    fn unpaired_open_is_point_match() {
        let src = b"fn before() {}\n// TODO: fix this\nfn broken() {}\n";
        let scope = MarkerScope {
            open: "TODO:".to_string(),
            close: None,
            capture: Some("LABEL".to_string()),
        };
        let regions = find_marker_regions(src, "test.rs", &scope);
        // Single marker with no next marker -> extends to EOF
        assert_eq!(regions.len(), 1);
        assert_eq!(
            regions[0].captures.get("LABEL").map(|c| c.text.as_str()),
            Some("fix this")
        );
    }

    #[test]
    fn unpaired_open_close_is_point_match() {
        let src = b"// BEGIN: orphan\nfn code() {}\n";
        let scope = MarkerScope {
            open: "BEGIN:".to_string(),
            close: Some("END:".to_string()),
            capture: None,
        };
        let regions = find_marker_regions(src, "test.rs", &scope);
        // No matching END, so the open becomes a point match on the comment node
        assert_eq!(regions.len(), 1);
        let region_text = std::str::from_utf8(&src[regions[0].start..regions[0].end]).unwrap();
        assert!(region_text.contains("BEGIN: orphan"));
        assert!(!region_text.contains("fn code"));
    }

    #[test]
    fn fallback_line_prefix_for_unknown_ext() {
        let src = b"# SECTION: config\nfoo=bar\n# SECTION: env\nbaz=qux\n";
        let scope = MarkerScope {
            open: "SECTION:".to_string(),
            close: None,
            capture: Some("LABEL".to_string()),
        };
        let regions = find_marker_regions(src, "unknown.conf", &scope);
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions[0].captures.get("LABEL").map(|c| c.text.as_str()),
            Some("config")
        );
    }

    #[test]
    fn no_matching_comments_returns_empty() {
        let src = b"fn no_markers() {}\n";
        let scope = MarkerScope {
            open: "SECTION:".to_string(),
            close: None,
            capture: None,
        };
        let regions = find_marker_regions(src, "test.rs", &scope);
        assert!(regions.is_empty());
    }
}
