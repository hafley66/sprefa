//! Comment-marker regions: `comment(p, rev, /open/[, /close/], l0, l1, label)`.
//! Port of v3 `pipeline/src/ops/comment.rs`, byte ranges traded for 1-based
//! line spans (v5 source ops bind lines; bytes ride the ref spine separately).
//!
//! Two modes:
//!   * open only — **sequential**: every comment matching `open` starts a
//!     region; it runs to the line before the next match (or the last line).
//!     Think bash `# SECTION: foo` dividers.
//!   * open + close — **paired**: markers nest LIFO; the region spans the open
//!     marker line through the close marker line. An unpaired open collapses
//!     to the marker line itself.
//!
//! Comment lines are detected by line prefix after leading whitespace:
//! `<!--`, `//`, `/*`, `--`, `#`, `*`. The regex matches against the trimmed
//! text after the marker.
//!
//! `label` per region: the open regex's first named-group capture when it
//! matched non-empty (a trailing `$NAME` hole desugars to a lazy group that
//! matches empty — see `desugar_regex_holes`), else the trimmed post-match
//! tail, else "".

use regex::Regex;

pub struct Region {
    /// Open-marker line, 1-based.
    pub l0: i64,
    /// Last line of the region, 1-based: the close-marker line (paired) or
    /// the line before the next marker / the file's last line (sequential).
    pub l1: i64,
    pub label: String,
    /// Byte range of the label in the file content, for the ref spine.
    pub label_span: Option<(usize, usize)>,
}

struct CommentLine<'a> {
    /// 0-based line index.
    line: usize,
    /// Byte offset of `text` in the file content.
    text_off: usize,
    /// Text after the comment marker and following whitespace.
    text: &'a str,
}

pub fn run_comment(content: &str, open: &Regex, close: Option<&Regex>) -> Vec<Region> {
    let comments = comment_lines(content);
    let mut regions = match close {
        Some(c) => paired(&comments, open, c),
        None => sequential(&comments, open, content.lines().count()),
    };
    regions.sort_by_key(|r| (r.l0, r.l1));
    regions
}

fn comment_lines(content: &str) -> Vec<CommentLine<'_>> {
    let base = content.as_ptr() as usize;
    let mut out = Vec::new();
    for (i, ln) in content.lines().enumerate() {
        let trimmed = ln.trim_start_matches([' ', '\t']);
        let m = marker_len(trimmed);
        if m == 0 {
            continue;
        }
        let text = trimmed[m..].trim_start_matches([' ', '\t']);
        out.push(CommentLine {
            line: i,
            text_off: text.as_ptr() as usize - base,
            text,
        });
    }
    out
}

/// Byte length of the comment marker opening `s`, 0 if none.
fn marker_len(s: &str) -> usize {
    for p in ["<!--", "//", "/*", "--"] {
        if s.starts_with(p) {
            return p.len();
        }
    }
    if s.starts_with('#') || s.starts_with('*') {
        1
    } else {
        0
    }
}

fn sequential(comments: &[CommentLine<'_>], open: &Regex, total_lines: usize) -> Vec<Region> {
    let opens: Vec<&CommentLine> = comments.iter().filter(|c| open.is_match(c.text)).collect();
    opens
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let l1 = opens
                .get(i + 1)
                .map(|n| n.line as i64)
                .unwrap_or(total_lines as i64);
            let (label, label_span) = label_of(c, open);
            Region {
                l0: c.line as i64 + 1,
                l1,
                label,
                label_span,
            }
        })
        .collect()
}

fn paired(comments: &[CommentLine<'_>], open: &Regex, close: &Regex) -> Vec<Region> {
    let mut regions = Vec::new();
    let mut stack: Vec<&CommentLine> = Vec::new();
    for c in comments {
        if open.is_match(c.text) {
            stack.push(c);
        } else if close.is_match(c.text) {
            if let Some(o) = stack.pop() {
                let (label, label_span) = label_of(o, open);
                regions.push(Region {
                    l0: o.line as i64 + 1,
                    l1: c.line as i64 + 1,
                    label,
                    label_span,
                });
            }
        }
    }
    // Unpaired opens collapse to the marker line itself.
    for o in stack {
        let (label, label_span) = label_of(o, open);
        regions.push(Region {
            l0: o.line as i64 + 1,
            l1: o.line as i64 + 1,
            label,
            label_span,
        });
    }
    regions
}

fn label_of(c: &CommentLine<'_>, open: &Regex) -> (String, Option<(usize, usize)>) {
    let Some(caps) = open.captures(c.text) else {
        return (String::new(), None);
    };
    if let Some(name) = open.capture_names().flatten().next() {
        if let Some(g) = caps.name(name) {
            if !g.as_str().is_empty() {
                return (
                    g.as_str().to_string(),
                    Some((c.text_off + g.start(), c.text_off + g.end())),
                );
            }
        }
    }
    let m = caps.get(0).unwrap();
    let tail = c.text[m.end()..].trim();
    if tail.is_empty() {
        return (String::new(), None);
    }
    let start = m.end() + c.text[m.end()..].len() - c.text[m.end()..].trim_start().len();
    (
        tail.to_string(),
        Some((c.text_off + start, c.text_off + start + tail.len())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_regions_run_to_next_marker_or_eof() {
        let src = "# SECTION: alpha\na\nb\n# SECTION: beta\nc\n";
        let re = Regex::new("SECTION:").unwrap();
        let rs = run_comment(src, &re, None);
        assert_eq!(rs.len(), 2);
        assert_eq!((rs[0].l0, rs[0].l1, rs[0].label.as_str()), (1, 3, "alpha"));
        assert_eq!((rs[1].l0, rs[1].l1, rs[1].label.as_str()), (4, 5, "beta"));
    }

    #[test]
    fn paired_regions_nest_lifo_and_unpaired_collapses() {
        let src =
            "// BEGIN: outer\nx\n// BEGIN: inner\ny\n// END:\nz\n// END:\n// BEGIN: leak\nw\n";
        let open = Regex::new("BEGIN:").unwrap();
        let close = Regex::new("END:").unwrap();
        let rs = run_comment(src, &open, Some(&close));
        assert_eq!(rs.len(), 3);
        assert_eq!((rs[0].l0, rs[0].l1, rs[0].label.as_str()), (1, 7, "outer"));
        assert_eq!((rs[1].l0, rs[1].l1, rs[1].label.as_str()), (3, 5, "inner"));
        assert_eq!((rs[2].l0, rs[2].l1, rs[2].label.as_str()), (8, 8, "leak"));
    }

    #[test]
    fn label_prefers_nonempty_named_group_else_tail() {
        let src = "// MARK: hello world\n";
        // Named group matches non-empty.
        let re = Regex::new(r"MARK: (?P<t>\w+)").unwrap();
        let rs = run_comment(src, &re, None);
        assert_eq!(rs[0].label, "hello");
        let (lo, hi) = rs[0].label_span.unwrap();
        assert_eq!(&src[lo..hi], "hello");
        // Trailing lazy group matches empty -> tail fallback.
        let re = Regex::new(r"MARK:(?P<t>.*?)").unwrap();
        let rs = run_comment(src, &re, None);
        assert_eq!(rs[0].label, "hello world");
    }

    #[test]
    fn marker_prefixes_cover_the_common_shapes() {
        let src = "<!-- S: a -->\n-- S: b\n* S: c\n/* S: d */\nplain S: e\n";
        let re = Regex::new("S: (?P<x>[a-z])").unwrap();
        let rs = run_comment(src, &re, None);
        let labels: Vec<&str> = rs.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["a", "b", "c", "d"],
            "non-comment line must not match"
        );
    }
}
