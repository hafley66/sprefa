/// Markdown structural matching for md() tag.
///
/// Two modes:
/// - Heading scoping: produces byte ranges from heading to next same-or-higher level heading.
/// - Element matching: matches list items, links, code blocks, table rows, blockquotes.
///
/// Line-based parser. No external markdown crate dependency.
use std::collections::HashMap;

use regex::Regex;

use crate::types::MdPattern;
use crate::walk::CapturedValue;

/// A byte range produced by heading scoping, with optional captures.
pub struct MdRegion {
    pub start: usize,
    pub end: usize,
    pub captures: HashMap<String, CapturedValue>,
}

/// A match produced by element matching.
pub struct MdMatch {
    pub captures: HashMap<String, CapturedValue>,
}

// ── Heading scoping ──────────────────────────────

/// Find byte ranges for heading-scoped regions.
///
/// Each matching heading opens a region that extends to the next heading
/// of the same or lower level (fewer #'s = higher rank), or EOF.
pub fn find_heading_regions(source: &[u8], pattern: &MdPattern) -> Vec<MdRegion> {
    let src = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let (level, text_filter, capture_name) = match pattern {
        MdPattern::Heading {
            level,
            text,
            capture,
        } => (*level, text.as_deref(), capture.as_deref()),
        _ => return vec![],
    };

    let text_re = text_filter.and_then(|t| {
        // Build regex from the text filter, replacing $VAR with named groups
        let pat = capture_pattern_to_regex(t);
        Regex::new(&pat).ok()
    });

    let mut regions: Vec<MdRegion> = vec![];
    let mut current_region: Option<(usize, HashMap<String, CapturedValue>)> = None;

    let mut offset = 0;
    for line in src.split('\n') {
        let line_start = offset;
        let line_end = offset + line.len();

        if let Some((h_level, h_text)) = parse_heading_line(line) {
            // If we have an open region and this heading closes it
            // (same or higher rank = lower or equal level number)
            if let Some((start, caps)) = current_region.take() {
                if h_level <= level {
                    regions.push(MdRegion {
                        start,
                        end: line_start,
                        captures: caps,
                    });
                } else {
                    // Deeper heading, region continues
                    current_region = Some((start, caps));
                }
            }

            // Check if this heading matches our pattern
            if h_level == level {
                let matched = match &text_re {
                    Some(re) => re.is_match(h_text),
                    None => text_filter.map_or(true, |t| {
                        // No $VAR captures, literal match
                        !t.contains('$') && h_text.trim() == t.trim()
                    }),
                };

                if matched {
                    let mut caps = HashMap::new();

                    // Extract regex captures if pattern has $VAR
                    if let Some(re) = &text_re {
                        if let Some(m) = re.captures(h_text) {
                            for name in re.capture_names().flatten() {
                                if let Some(val) = m.name(name) {
                                    caps.insert(
                                        name.to_string(),
                                        CapturedValue {
                                            text: val.as_str().to_string(),
                                            span_start: line_start as u32,
                                            span_end: line_end as u32,
                                        },
                                    );
                                }
                            }
                        }
                    }

                    // Capture the heading text itself if requested
                    if let Some(cap) = capture_name {
                        if !caps.contains_key(cap) {
                            caps.insert(
                                cap.to_string(),
                                CapturedValue {
                                    text: h_text.trim().to_string(),
                                    span_start: line_start as u32,
                                    span_end: line_end as u32,
                                },
                            );
                        }
                    }

                    current_region = Some((line_start, caps));
                }
            }
        }

        offset = line_end + 1; // +1 for newline
    }

    // Close any open region at EOF
    if let Some((start, caps)) = current_region {
        regions.push(MdRegion {
            start,
            end: source.len(),
            captures: caps,
        });
    }

    regions
}

// ── Element matching ─────────────────────────────

/// Match markdown elements within a source slice, producing captures.
pub fn match_elements(source: &[u8], pattern: &MdPattern) -> Vec<MdMatch> {
    let src = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    match pattern {
        MdPattern::Heading {
            level,
            text,
            capture,
        } => match_headings(src, *level, text.as_deref(), capture.as_deref()),
        MdPattern::ListItem { capture } => match_list_items(src, capture.as_deref()),
        MdPattern::Link {
            text_capture,
            url_capture,
        } => match_links(src, text_capture.as_deref(), url_capture.as_deref()),
        MdPattern::CodeBlock {
            lang_capture,
            body_capture,
        } => match_code_blocks(src, lang_capture.as_deref(), body_capture.as_deref()),
        MdPattern::TableRow { capture } => match_table_rows(src, capture.as_deref()),
        MdPattern::Blockquote { capture } => match_blockquotes(src, capture.as_deref()),
    }
}

fn match_headings(
    src: &str,
    level: u8,
    text_filter: Option<&str>,
    capture_name: Option<&str>,
) -> Vec<MdMatch> {
    let text_re = text_filter.and_then(|t| {
        let pat = capture_pattern_to_regex(t);
        Regex::new(&pat).ok()
    });

    let mut matches = vec![];
    for line in src.lines() {
        if let Some((h_level, h_text)) = parse_heading_line(line) {
            if h_level != level {
                continue;
            }

            let matched = match &text_re {
                Some(re) => re.is_match(h_text),
                None => text_filter.map_or(true, |t| !t.contains('$') && h_text.trim() == t.trim()),
            };

            if matched {
                let mut caps = HashMap::new();
                if let Some(re) = &text_re {
                    if let Some(m) = re.captures(h_text) {
                        for name in re.capture_names().flatten() {
                            if let Some(val) = m.name(name) {
                                caps.insert(
                                    name.to_string(),
                                    CapturedValue {
                                        text: val.as_str().to_string(),
                                        span_start: 0,
                                        span_end: 0,
                                    },
                                );
                            }
                        }
                    }
                }
                if let Some(cap) = capture_name {
                    if !caps.contains_key(cap) {
                        caps.insert(
                            cap.to_string(),
                            CapturedValue {
                                text: h_text.trim().to_string(),
                                span_start: 0,
                                span_end: 0,
                            },
                        );
                    }
                }
                matches.push(MdMatch { captures: caps });
            }
        }
    }
    matches
}

fn match_list_items(src: &str, capture_name: Option<&str>) -> Vec<MdMatch> {
    let re = Regex::new(r"^[ \t]*(?:[-*+]|\d+[.)]) (.+)").unwrap();
    let mut matches = vec![];
    for line in src.lines() {
        if let Some(m) = re.captures(line) {
            let text = m.get(1).unwrap().as_str();
            let mut caps = HashMap::new();
            if let Some(cap) = capture_name {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: text.to_string(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            matches.push(MdMatch { captures: caps });
        }
    }
    matches
}

fn match_links(
    src: &str,
    text_capture: Option<&str>,
    url_capture: Option<&str>,
) -> Vec<MdMatch> {
    let re = Regex::new(r"\[([^\]]*)\]\(([^)]*)\)").unwrap();
    let mut matches = vec![];
    for line in src.lines() {
        for m in re.captures_iter(line) {
            let text = m.get(1).unwrap().as_str();
            let url = m.get(2).unwrap().as_str();
            let mut caps = HashMap::new();
            if let Some(cap) = text_capture {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: text.to_string(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            if let Some(cap) = url_capture {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: url.to_string(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            matches.push(MdMatch { captures: caps });
        }
    }
    matches
}

fn match_code_blocks(
    src: &str,
    lang_capture: Option<&str>,
    body_capture: Option<&str>,
) -> Vec<MdMatch> {
    let mut matches = vec![];
    let mut in_block = false;
    let mut block_lang = String::new();
    let mut block_body = String::new();

    for line in src.lines() {
        if !in_block {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_block = true;
                block_lang = trimmed[3..].trim().to_string();
                block_body.clear();
            }
        } else if line.trim_start().starts_with("```") {
            // End of block
            let mut caps = HashMap::new();
            if let Some(cap) = lang_capture {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: block_lang.clone(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            if let Some(cap) = body_capture {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: block_body.trim_end().to_string(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            matches.push(MdMatch { captures: caps });
            in_block = false;
        } else {
            if !block_body.is_empty() {
                block_body.push('\n');
            }
            block_body.push_str(line);
        }
    }
    matches
}

fn match_table_rows(src: &str, capture_name: Option<&str>) -> Vec<MdMatch> {
    let mut matches = vec![];
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            // Skip separator rows (| --- | --- |)
            let inner = &trimmed[1..trimmed.len() - 1];
            if inner.chars().all(|c| c == '-' || c == '|' || c == ' ' || c == ':') {
                continue;
            }
            let mut caps = HashMap::new();
            if let Some(cap) = capture_name {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: trimmed.to_string(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            matches.push(MdMatch { captures: caps });
        }
    }
    matches
}

fn match_blockquotes(src: &str, capture_name: Option<&str>) -> Vec<MdMatch> {
    let mut matches = vec![];
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("> ") || trimmed == ">" {
            let text = if trimmed.len() > 2 { &trimmed[2..] } else { "" };
            let mut caps = HashMap::new();
            if let Some(cap) = capture_name {
                caps.insert(
                    cap.to_string(),
                    CapturedValue {
                        text: text.to_string(),
                        span_start: 0,
                        span_end: 0,
                    },
                );
            }
            matches.push(MdMatch { captures: caps });
        }
    }
    matches
}

// ── Helpers ──────────────────────────────────────

/// Parse a line as a markdown heading. Returns (level, text after hashes).
fn parse_heading_line(line: &str) -> Option<(u8, &str)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    // Must have space after hashes (or be hashes-only for empty heading)
    let rest = &trimmed[hashes..];
    if rest.is_empty() {
        return Some((hashes as u8, ""));
    }
    if !rest.starts_with(' ') {
        return None;
    }
    Some((hashes as u8, &rest[1..]))
}

/// Convert a capture pattern like `$NAME` or `$ORG/$REPO` to a regex.
///
/// `$NAME` -> `(?P<NAME>[^\s]+)`
/// `$$$NAME` -> `(?P<NAME>.+)`
/// Literal segments are escaped.
fn capture_pattern_to_regex(pattern: &str) -> String {
    let mut out = String::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;

    // If the pattern has no $ captures, just return it as a literal regex
    if !pattern.contains('$') {
        return regex::escape(pattern.trim());
    }

    out.push('^');
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            // $$$ = multi-capture
            let multi = i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'$';
            if multi {
                i += 2;
            }
            // Skip $_ wildcard
            if i < bytes.len() && bytes[i] == b'_' {
                if multi {
                    out.push_str(".+");
                } else {
                    out.push_str("\\S+");
                }
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let name = &pattern[start..i];
                if multi {
                    out.push_str(&format!("(?P<{}>.+)", name));
                } else {
                    out.push_str(&format!("(?P<{}>\\S+)", name));
                }
            }
        } else {
            out.push_str(&regex::escape(&pattern[i..i + 1]));
            i += 1;
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_scoping_basic() {
        let src = b"# Top\n## Section A\nContent A\n## Section B\nContent B\n";
        let pattern = MdPattern::Heading {
            level: 2,
            text: None,
            capture: Some("SECTION".to_string()),
        };
        let regions = find_heading_regions(src, &pattern);
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions[0].captures.get("SECTION").map(|c| c.text.as_str()),
            Some("Section A")
        );
        assert_eq!(
            regions[1].captures.get("SECTION").map(|c| c.text.as_str()),
            Some("Section B")
        );
        // Section A region contains "Content A" but not "Content B"
        let a_text = std::str::from_utf8(&src[regions[0].start..regions[0].end]).unwrap();
        assert!(a_text.contains("Content A"));
        assert!(!a_text.contains("Content B"));
    }

    #[test]
    fn heading_scoping_with_text_filter() {
        let src = b"## Install\nstep 1\n## Usage\nstep 2\n## Contributing\nstep 3\n";
        let pattern = MdPattern::Heading {
            level: 2,
            text: Some("Usage".to_string()),
            capture: None,
        };
        let regions = find_heading_regions(src, &pattern);
        assert_eq!(regions.len(), 1);
        let text = std::str::from_utf8(&src[regions[0].start..regions[0].end]).unwrap();
        assert!(text.contains("step 2"));
        assert!(!text.contains("step 1"));
        assert!(!text.contains("step 3"));
    }

    #[test]
    fn heading_scoping_with_capture_var() {
        let src = b"## foo-service\ndocs\n## bar-service\ndocs\n";
        let pattern = MdPattern::Heading {
            level: 2,
            text: Some("$NAME".to_string()),
            capture: None,
        };
        let regions = find_heading_regions(src, &pattern);
        assert_eq!(regions.len(), 2);
        assert_eq!(
            regions[0].captures.get("NAME").map(|c| c.text.as_str()),
            Some("foo-service")
        );
        assert_eq!(
            regions[1].captures.get("NAME").map(|c| c.text.as_str()),
            Some("bar-service")
        );
    }

    #[test]
    fn heading_higher_rank_closes_region() {
        let src = b"## A\ncontent\n# Top\nmore\n";
        let pattern = MdPattern::Heading {
            level: 2,
            text: None,
            capture: Some("S".to_string()),
        };
        let regions = find_heading_regions(src, &pattern);
        assert_eq!(regions.len(), 1);
        let text = std::str::from_utf8(&src[regions[0].start..regions[0].end]).unwrap();
        assert!(text.contains("content"));
        assert!(!text.contains("Top"));
    }

    #[test]
    fn match_list_items_basic() {
        let src = b"## Deps\n- express\n- lodash\n* chalk\n1. first\n";
        let pattern = MdPattern::ListItem {
            capture: Some("ITEM".to_string()),
        };
        let matches = match_elements(src, &pattern);
        let items: Vec<&str> = matches
            .iter()
            .filter_map(|m| m.captures.get("ITEM").map(|c| c.text.as_str()))
            .collect();
        assert_eq!(items, vec!["express", "lodash", "chalk", "first"]);
    }

    #[test]
    fn match_links_basic() {
        let src = b"See [docs](https://example.com) and [repo](https://github.com/x)\n";
        let pattern = MdPattern::Link {
            text_capture: Some("TEXT".to_string()),
            url_capture: Some("URL".to_string()),
        };
        let matches = match_elements(src, &pattern);
        assert_eq!(matches.len(), 2);
        assert_eq!(
            matches[0].captures.get("TEXT").map(|c| c.text.as_str()),
            Some("docs")
        );
        assert_eq!(
            matches[0].captures.get("URL").map(|c| c.text.as_str()),
            Some("https://example.com")
        );
    }

    #[test]
    fn match_code_blocks_basic() {
        let src = b"text\n```rust\nfn main() {}\n```\nmore\n```js\nconsole.log(1)\n```\n";
        let pattern = MdPattern::CodeBlock {
            lang_capture: Some("LANG".to_string()),
            body_capture: Some("BODY".to_string()),
        };
        let matches = match_elements(src, &pattern);
        assert_eq!(matches.len(), 2);
        assert_eq!(
            matches[0].captures.get("LANG").map(|c| c.text.as_str()),
            Some("rust")
        );
        assert_eq!(
            matches[0].captures.get("BODY").map(|c| c.text.as_str()),
            Some("fn main() {}")
        );
        assert_eq!(
            matches[1].captures.get("LANG").map(|c| c.text.as_str()),
            Some("js")
        );
    }

    #[test]
    fn match_table_rows_skips_separator() {
        let src = b"| Name | Version |\n| --- | --- |\n| express | 4.18 |\n| lodash | 4.17 |\n";
        let pattern = MdPattern::TableRow {
            capture: Some("ROW".to_string()),
        };
        let matches = match_elements(src, &pattern);
        assert_eq!(matches.len(), 3); // header + 2 data rows, separator skipped
    }

    #[test]
    fn match_blockquotes_basic() {
        let src = b"> Note: this is important\n> second line\nregular text\n";
        let pattern = MdPattern::Blockquote {
            capture: Some("TEXT".to_string()),
        };
        let matches = match_elements(src, &pattern);
        assert_eq!(matches.len(), 2);
        assert_eq!(
            matches[0].captures.get("TEXT").map(|c| c.text.as_str()),
            Some("Note: this is important")
        );
    }

    #[test]
    fn capture_pattern_to_regex_basic() {
        let re_str = capture_pattern_to_regex("$NAME");
        let re = Regex::new(&re_str).unwrap();
        let caps = re.captures("foo-service").unwrap();
        assert_eq!(caps.name("NAME").unwrap().as_str(), "foo-service");
    }

    #[test]
    fn capture_pattern_to_regex_composite() {
        let re_str = capture_pattern_to_regex("$ORG/$REPO");
        let re = Regex::new(&re_str).unwrap();
        let caps = re.captures("myorg/myrepo").unwrap();
        assert_eq!(caps.name("ORG").unwrap().as_str(), "myorg");
        assert_eq!(caps.name("REPO").unwrap().as_str(), "myrepo");
    }
}
