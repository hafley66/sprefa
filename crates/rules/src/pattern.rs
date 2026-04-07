use std::collections::HashMap;

use globset::{Glob, GlobMatcher};
use regex::Regex;

/// One segment in a segment-capture pattern.
///
/// Pattern `$ORG/$REPO` parses to `[Capture("ORG"), Literal("/"), Capture("REPO")]`.
/// Pattern `support/$MAJOR.$MINOR` parses to
///   `[Literal("support/"), Capture("MAJOR"), Literal("."), Capture("MINOR")]`.
#[derive(Debug, Clone)]
pub enum Segment {
    /// Literal text that must match exactly.
    Literal(String),
    /// `$NAME` -- captures one segment of non-separator characters.
    Capture(String),
    /// `$$$NAME` -- captures zero or more characters including separators.
    MultiCapture(String),
    /// `$_` -- matches one segment, no binding.
    Wild,
    /// `$$$_` -- matches zero or more characters including separators, no binding.
    MultiWild,
}

/// A single compiled pattern: glob, regex, or segment capture.
///
/// Detection rule for pattern strings:
/// - `re:` prefix -> Regex
/// - contains `$` -> SegmentCapture
/// - otherwise -> Glob (pipe `|` splits into multiple globs)
pub enum PatternMatcher {
    Glob(GlobMatcher),
    Regex(Regex),
    SegmentCapture(Vec<Segment>),
}

impl std::fmt::Debug for PatternMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Glob(_) => write!(f, "Glob(..)"),
            Self::Regex(r) => write!(f, "Regex({})", r.as_str()),
            Self::SegmentCapture(segs) => write!(f, "SegmentCapture({:?})", segs),
        }
    }
}

impl PatternMatcher {
    pub fn is_match(&self, value: &str) -> bool {
        match self {
            Self::Glob(g) => g.is_match(value),
            Self::Regex(r) => r.is_match(value),
            Self::SegmentCapture(segs) => match_segments(segs, value).is_some(),
        }
    }

    /// Extract named captures from the pattern against `value`.
    /// Returns None if no match or if the pattern type doesn't capture.
    pub fn captures(&self, value: &str) -> Option<HashMap<String, String>> {
        match self {
            Self::Glob(_) => None,
            Self::Regex(r) => {
                let caps = r.captures(value)?;
                let mut map = HashMap::new();
                for name in r.capture_names().flatten() {
                    if let Some(m) = caps.name(name) {
                        map.insert(name.to_string(), m.as_str().to_string());
                    }
                }
                if map.is_empty() {
                    None
                } else {
                    Some(map)
                }
            }
            Self::SegmentCapture(segs) => match_segments(segs, value),
        }
    }
}

/// Parse a pattern string containing `$` captures into a Vec<Segment>.
///
/// `$NAME` or `${NAME}` = single capture (greedy up to next literal or end).
/// `$$$NAME` or `$$${NAME}` = multi capture (greedy across separators).
/// `$_` = wild (single). `$$$_` = multi wild.
/// Everything else is literal.
///
/// Use `${NAME}` when the capture is adjacent to identifier characters,
/// e.g. `use${ENTITY}Query` captures `ENTITY` with literal `use` and `Query` around it.
pub fn parse_segment_pattern(pattern: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut chars = pattern.chars().peekable();
    let mut literal = String::new();

    while let Some(&c) = chars.peek() {
        if c == '$' {
            // flush literal
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }

            chars.next(); // consume first $
                          // check for $$$ (multi)
            let multi = chars.peek() == Some(&'$') && {
                let mut lookahead = chars.clone();
                lookahead.next();
                lookahead.peek() == Some(&'$')
            };

            if multi {
                chars.next(); // second $
                chars.next(); // third $
            }

            // read the name: ${NAME} braced or $NAME bare
            let name = if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut n = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == '}' {
                        chars.next(); // consume '}'
                        break;
                    }
                    n.push(nc);
                    chars.next();
                }
                n
            } else {
                let mut n = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc.is_ascii_alphanumeric() || nc == '_' {
                        n.push(nc);
                        chars.next();
                    } else {
                        break;
                    }
                }
                n
            };

            if name == "_" || name.is_empty() {
                if multi {
                    segments.push(Segment::MultiWild);
                } else {
                    segments.push(Segment::Wild);
                }
            } else if multi {
                segments.push(Segment::MultiCapture(name));
            } else {
                segments.push(Segment::Capture(name));
            }
        } else {
            literal.push(c);
            chars.next();
        }
    }

    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }

    segments
}

/// Public entry point for walk engine. Matches segments against a value.
pub fn match_segments_pub(segments: &[Segment], value: &str) -> Option<HashMap<String, String>> {
    match_segments(segments, value)
}

/// Compute byte offsets of each named capture within the matched value.
///
/// Given segments and the capture map from a successful `match_segments` call,
/// walks the segments sequentially to determine where each capture starts/ends
/// within the value string. Returns `(start_byte, end_byte)` pairs.
///
/// Stops computing if a Wild/MultiWild is encountered (offset becomes ambiguous).
pub fn capture_offsets_in_value(
    segments: &[Segment],
    captures: &HashMap<String, String>,
) -> HashMap<String, (u32, u32)> {
    let mut offsets = HashMap::new();
    let mut pos: u32 = 0;
    for seg in segments {
        match seg {
            Segment::Literal(s) => pos += s.len() as u32,
            Segment::Capture(name) | Segment::MultiCapture(name) => {
                if let Some(text) = captures.get(name) {
                    let start = pos;
                    pos += text.len() as u32;
                    offsets.insert(name.clone(), (start, pos));
                }
            }
            Segment::Wild | Segment::MultiWild => break,
        }
    }
    offsets
}

/// Match a segment pattern against a value, returning captured bindings.
///
/// Single captures (`$NAME`) match greedily up to the next literal or end of string,
/// but won't match `/` (path separator) unless the pattern has no more literals.
/// Multi captures (`$$$NAME`) match across `/` boundaries.
fn match_segments(segments: &[Segment], value: &str) -> Option<HashMap<String, String>> {
    let mut captures = HashMap::new();
    match_segments_inner(segments, value, &mut captures).then_some(captures)
}

fn match_segments_inner(
    segments: &[Segment],
    remaining: &str,
    captures: &mut HashMap<String, String>,
) -> bool {
    if segments.is_empty() {
        return remaining.is_empty();
    }

    match &segments[0] {
        Segment::Literal(lit) => {
            if let Some(rest) = remaining.strip_prefix(lit.as_str()) {
                match_segments_inner(&segments[1..], rest, captures)
            } else {
                false
            }
        }

        Segment::Capture(name) => {
            // Find the shortest match that allows the rest to succeed.
            // Don't cross `/` boundaries for single captures.
            let next_lit = find_next_literal(&segments[1..]);
            let limit = remaining.find('/').unwrap_or(remaining.len());

            for end in 1..=limit {
                if !remaining.is_char_boundary(end) {
                    continue;
                }
                // If there's a following literal, prefer ending right before it
                if let Some(ref lit) = next_lit {
                    if !remaining[end..].starts_with(lit.as_str()) {
                        continue;
                    }
                }
                let candidate = &remaining[..end];
                let mut trial = captures.clone();
                trial.insert(name.clone(), candidate.to_string());
                if match_segments_inner(&segments[1..], &remaining[end..], &mut trial) {
                    *captures = trial;
                    return true;
                }
            }
            // If no next literal constraint, also try matching up to limit
            if next_lit.is_some() {
                // already tried all positions
            }
            false
        }

        Segment::MultiCapture(name) => {
            // Try all lengths, shortest first
            for end in 0..=remaining.len() {
                if !remaining.is_char_boundary(end) {
                    continue;
                }
                let candidate = &remaining[..end];
                let mut trial = captures.clone();
                trial.insert(name.clone(), candidate.to_string());
                if match_segments_inner(&segments[1..], &remaining[end..], &mut trial) {
                    *captures = trial;
                    return true;
                }
            }
            false
        }

        Segment::Wild => {
            let limit = remaining.find('/').unwrap_or(remaining.len());
            for end in 1..=limit {
                if !remaining.is_char_boundary(end) {
                    continue;
                }
                if match_segments_inner(&segments[1..], &remaining[end..], captures) {
                    return true;
                }
            }
            false
        }

        Segment::MultiWild => {
            for end in 0..=remaining.len() {
                if !remaining.is_char_boundary(end) {
                    continue;
                }
                if match_segments_inner(&segments[1..], &remaining[end..], captures) {
                    return true;
                }
            }
            false
        }
    }
}

/// Find the next Literal segment's text, for anchoring capture boundaries.
fn find_next_literal(segments: &[Segment]) -> Option<String> {
    for seg in segments {
        match seg {
            Segment::Literal(s) => return Some(s.clone()),
            _ => continue,
        }
    }
    None
}

/// Compile a single pipe-delimited pattern string into matchers.
///
/// Returns a Vec because pipe `|` in glob mode produces multiple matchers.
pub fn compile_pattern(pattern: &str) -> anyhow::Result<Vec<PatternMatcher>> {
    compile_patterns(&[pattern])
}

/// Test a single pattern string against a value.
/// Compiles the pattern on each call -- use `compile_pattern` to cache.
pub fn pattern_matches(pattern: &str, value: &str) -> bool {
    compile_pattern(pattern)
        .map(|ms| ms.iter().any(|m| m.is_match(value)))
        .unwrap_or(false)
}

/// Compile a slice of pattern strings into matchers.
///
/// Each string is either:
/// - `re:pattern` -> compiled as regex
/// - contains `$` -> parsed as segment capture pattern
/// - `a|b|c` -> split on `|`, each compiled as glob
pub fn compile_patterns(patterns: &[&str]) -> anyhow::Result<Vec<PatternMatcher>> {
    let mut matchers = Vec::new();
    for p in patterns {
        if let Some(re_pattern) = p.strip_prefix("re:") {
            matchers.push(PatternMatcher::Regex(Regex::new(re_pattern)?));
        } else if p.contains('$') {
            matchers.push(PatternMatcher::SegmentCapture(parse_segment_pattern(p)));
        } else {
            for segment in p.split('|') {
                let segment = segment.trim();
                matchers.push(PatternMatcher::Glob(Glob::new(segment)?.compile_matcher()));
            }
        }
    }
    Ok(matchers)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parse tests ──────────────────────────────────

    #[test]
    fn parse_simple_capture() {
        let segs = parse_segment_pattern("$ORG/$REPO");
        assert!(matches!(&segs[0], Segment::Capture(n) if n == "ORG"));
        assert!(matches!(&segs[1], Segment::Literal(s) if s == "/"));
        assert!(matches!(&segs[2], Segment::Capture(n) if n == "REPO"));
    }

    #[test]
    fn parse_literal_prefix() {
        let segs = parse_segment_pattern("support/$MAJOR.$MINOR");
        assert!(matches!(&segs[0], Segment::Literal(s) if s == "support/"));
        assert!(matches!(&segs[1], Segment::Capture(n) if n == "MAJOR"));
        assert!(matches!(&segs[2], Segment::Literal(s) if s == "."));
        assert!(matches!(&segs[3], Segment::Capture(n) if n == "MINOR"));
    }

    #[test]
    fn parse_multi_capture() {
        let segs = parse_segment_pattern("$$$REST");
        assert!(matches!(&segs[0], Segment::MultiCapture(n) if n == "REST"));
    }

    #[test]
    fn parse_wild() {
        let segs = parse_segment_pattern("$_/$NAME");
        assert!(matches!(&segs[0], Segment::Wild));
        assert!(matches!(&segs[1], Segment::Literal(s) if s == "/"));
        assert!(matches!(&segs[2], Segment::Capture(n) if n == "NAME"));
    }

    #[test]
    fn parse_multi_wild() {
        let segs = parse_segment_pattern("$$$_/end");
        assert!(matches!(&segs[0], Segment::MultiWild));
        assert!(matches!(&segs[1], Segment::Literal(s) if s == "/end"));
    }

    #[test]
    fn parse_braced_capture() {
        let segs = parse_segment_pattern("use${ENTITY}Query");
        assert!(matches!(&segs[0], Segment::Literal(s) if s == "use"));
        assert!(matches!(&segs[1], Segment::Capture(n) if n == "ENTITY"));
        assert!(matches!(&segs[2], Segment::Literal(s) if s == "Query"));
    }

    #[test]
    fn parse_braced_multi_capture() {
        let segs = parse_segment_pattern("$$${PATH}/end");
        assert!(matches!(&segs[0], Segment::MultiCapture(n) if n == "PATH"));
        assert!(matches!(&segs[1], Segment::Literal(s) if s == "/end"));
    }

    #[test]
    fn parse_braced_wild() {
        let segs = parse_segment_pattern("${_}/$NAME");
        assert!(matches!(&segs[0], Segment::Wild));
        assert!(matches!(&segs[1], Segment::Literal(s) if s == "/"));
        assert!(matches!(&segs[2], Segment::Capture(n) if n == "NAME"));
    }

    // ── Match tests ──────────────────────────────────

    #[test]
    fn match_org_repo() {
        let segs = parse_segment_pattern("$ORG/$REPO");
        let caps = match_segments(&segs, "acme/frontend").unwrap();
        assert_eq!(caps["ORG"], "acme");
        assert_eq!(caps["REPO"], "frontend");
    }

    #[test]
    fn match_org_repo_no_match() {
        let segs = parse_segment_pattern("$ORG/$REPO");
        assert!(match_segments(&segs, "noslash").is_none());
    }

    #[test]
    fn match_version_dots() {
        let segs = parse_segment_pattern("support/$MAJOR.$MINOR.$PATCH");
        let caps = match_segments(&segs, "support/1.2.3").unwrap();
        assert_eq!(caps["MAJOR"], "1");
        assert_eq!(caps["MINOR"], "2");
        assert_eq!(caps["PATCH"], "3");
    }

    #[test]
    fn match_literal_prefix_fail() {
        let segs = parse_segment_pattern("support/$MAJOR.$MINOR");
        assert!(match_segments(&segs, "release/1.2").is_none());
    }

    #[test]
    fn match_single_capture_whole_value() {
        let segs = parse_segment_pattern("$BRANCH");
        let caps = match_segments(&segs, "main").unwrap();
        assert_eq!(caps["BRANCH"], "main");
    }

    #[test]
    fn match_single_capture_rejects_slash() {
        let segs = parse_segment_pattern("$BRANCH");
        // $BRANCH is single-segment, won't cross /
        assert!(match_segments(&segs, "feature/foo").is_none());
    }

    #[test]
    fn match_multi_capture() {
        let segs = parse_segment_pattern("$$$BRANCH");
        let caps = match_segments(&segs, "feature/foo/bar").unwrap();
        assert_eq!(caps["BRANCH"], "feature/foo/bar");
    }

    #[test]
    fn match_multi_capture_with_prefix() {
        let segs = parse_segment_pattern("release/$$$REST");
        let caps = match_segments(&segs, "release/1.2.3/hotfix").unwrap();
        assert_eq!(caps["REST"], "1.2.3/hotfix");
    }

    #[test]
    fn match_wild_skips_segment() {
        let segs = parse_segment_pattern("$_/$NAME");
        let caps = match_segments(&segs, "anything/target").unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps["NAME"], "target");
    }

    #[test]
    fn match_multi_wild() {
        let segs = parse_segment_pattern("$$$_/end");
        assert!(match_segments(&segs, "a/b/c/end").is_some());
        assert!(match_segments(&segs, "/end").is_some());
        assert!(match_segments(&segs, "a/b/c/nope").is_none());
    }

    #[test]
    fn match_braced_capture_adjacent() {
        let segs = parse_segment_pattern("use${ENTITY}Query");
        let caps = match_segments(&segs, "useUserQuery").unwrap();
        assert_eq!(caps["ENTITY"], "User");
    }

    #[test]
    fn match_braced_capture_no_match() {
        let segs = parse_segment_pattern("use${ENTITY}Query");
        assert!(match_segments(&segs, "useUserMutation").is_none());
    }

    // ── Offset computation ───────────────────────────

    #[test]
    fn capture_offsets_braced() {
        let segs = parse_segment_pattern("use${ENTITY}Query");
        let caps = match_segments(&segs, "useUserQuery").unwrap();
        let offsets = capture_offsets_in_value(&segs, &caps);
        // "use" = 3 bytes, "User" = 4 bytes, "Query" = 5 bytes
        assert_eq!(offsets["ENTITY"], (3, 7));
    }

    #[test]
    fn capture_offsets_multiple() {
        let segs = parse_segment_pattern("$ORG/$REPO");
        let caps = match_segments(&segs, "acme/frontend").unwrap();
        let offsets = capture_offsets_in_value(&segs, &caps);
        assert_eq!(offsets["ORG"], (0, 4));   // "acme"
        assert_eq!(offsets["REPO"], (5, 13)); // "frontend"
    }

    // ── PatternMatcher integration ───────────────────

    #[test]
    fn compile_detects_segment_capture() {
        let matchers = compile_patterns(&["$ORG/$REPO"]).unwrap();
        assert!(matches!(&matchers[0], PatternMatcher::SegmentCapture(_)));
        assert!(matchers[0].is_match("acme/frontend"));
        assert!(!matchers[0].is_match("noslash"));
    }

    #[test]
    fn compile_detects_glob() {
        let matchers = compile_patterns(&["release/*"]).unwrap();
        assert!(matches!(&matchers[0], PatternMatcher::Glob(_)));
    }

    #[test]
    fn compile_detects_regex() {
        let matchers = compile_patterns(&["re:^v\\d+"]).unwrap();
        assert!(matches!(&matchers[0], PatternMatcher::Regex(_)));
    }

    #[test]
    fn captures_from_segment() {
        let matchers = compile_patterns(&["$ORG/$REPO"]).unwrap();
        let caps = matchers[0].captures("acme/frontend").unwrap();
        assert_eq!(caps["ORG"], "acme");
        assert_eq!(caps["REPO"], "frontend");
    }

    #[test]
    fn captures_from_regex() {
        let matchers = compile_patterns(&["re:(?P<VER>\\d+\\.\\d+)"]).unwrap();
        let caps = matchers[0].captures("release-1.5").unwrap();
        assert_eq!(caps["VER"], "1.5");
    }

    #[test]
    fn captures_from_glob_is_none() {
        let matchers = compile_patterns(&["release/*"]).unwrap();
        assert!(matchers[0].captures("release/foo").is_none());
    }
}
