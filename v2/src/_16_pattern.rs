//! Pattern segment types and segment-level matching.
//!
//! Ported from v1 `crates/rules/src/pattern.rs`.
//! Logic is identical to v1. Internal `HashMap` uses std here because
//! these are short-lived scratch maps inside the recursive matcher.
//!
//! Shared by the walker (`walk::`) and top-level ops (`repo`, `rev`, `fs`)
//! so every pattern slot in the language uses one matcher: `re:` regex,
//! `$NAME` segment capture, glob with `|` alternation, `**` recursive,
//! `?`, `[abc]`, `{a,b}` (all via `globset`).

use std::collections::HashMap;
use std::sync::Arc;

use globset::{Glob, GlobMatcher};
use regex::Regex;

/// One segment in a segment-capture pattern.
///
/// Pattern `$ORG/$REPO` parses to `[Capture("ORG"), Literal("/"), Capture("REPO")]`.
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
                if map.is_empty() { None } else { Some(map) }
            }
            Self::SegmentCapture(segs) => match_segments(segs, value),
        }
    }
}

/// Parse a pattern string containing `$` captures into a Vec<Segment>.
pub fn parse_segment_pattern(pattern: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut chars = pattern.chars().peekable();
    let mut literal = String::new();

    while let Some(&c) = chars.peek() {
        if c == '$' {
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }

            chars.next(); // consume first $
            let multi = chars.peek() == Some(&'$') && {
                let mut lookahead = chars.clone();
                lookahead.next();
                lookahead.peek() == Some(&'$')
            };

            if multi {
                chars.next(); // second $
                chars.next(); // third $
            }

            let name = if chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut n = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == '}' {
                        chars.next();
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

/// Match segments with pre-bound captures that act as constraints.
/// Pre-bound names constrain instead of capture: the value must equal the bound text.
pub fn match_segments_with_bindings(
    segments: &[Segment],
    value: &str,
    pre_bound: HashMap<String, String>,
) -> Option<HashMap<String, String>> {
    let mut captures = pre_bound;
    match_segments_inner(segments, value, &mut captures).then_some(captures)
}

/// Compile a single pipe-delimited pattern string into matchers.
pub fn compile_pattern(pattern: &str) -> anyhow::Result<Vec<PatternMatcher>> {
    compile_patterns(&[pattern])
}

/// A parsed pattern plus its source text. Source is kept so ops can
/// render the original string in hovers / diagnostics without losing it.
///
/// `is_match` succeeds when *any* compiled matcher matches — this is how
/// `|` alternation falls out of `compile_pattern`.
pub struct CompiledPattern {
    pub src:      Arc<str>,
    pub matchers: Vec<PatternMatcher>,
}

impl std::fmt::Debug for CompiledPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledPattern")
            .field("src", &self.src)
            .field("matchers", &self.matchers)
            .finish()
    }
}

impl CompiledPattern {
    pub fn compile(src: &str) -> anyhow::Result<Self> {
        Ok(Self {
            src:      Arc::from(src),
            matchers: compile_pattern(src)?,
        })
    }
    pub fn is_match(&self, s: &str) -> bool {
        self.matchers.iter().any(|m| m.is_match(s))
    }
}

/// Normalize a glob segment so trailing `/` behaves like v1's `folder(...)` step:
/// `v2/` → `**/v2/**`, `**/foo/` → `**/foo/**`, `/abs/` → `/abs/**`.
/// Patterns with `**`-prefix, absolute `/`-prefix, or `$` segment captures
/// skip the `**/` anchoring. No trailing slash → passthrough.
pub fn normalize_folder_glob(segment: &str) -> std::borrow::Cow<'_, str> {
    if segment == "/" || !segment.ends_with('/') {
        return std::borrow::Cow::Borrowed(segment);
    }
    let stripped = &segment[..segment.len() - 1];
    let anchored_owned;
    let anchored: &str = if stripped.starts_with("**")
        || stripped.starts_with('/')
        || stripped.contains('$')
    {
        stripped
    } else {
        anchored_owned = format!("**/{stripped}");
        // Return a new String so the borrow lives through the final format.
        return std::borrow::Cow::Owned(format!("{anchored_owned}/**"));
    };
    std::borrow::Cow::Owned(format!("{anchored}/**"))
}

/// Compile a slice of pattern strings into matchers.
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
                let normalized = normalize_folder_glob(segment);
                matchers.push(PatternMatcher::Glob(Glob::new(&normalized)?.compile_matcher()));
            }
        }
    }
    Ok(matchers)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

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
            if let Some(bound) = captures.get(name).cloned() {
                if let Some(rest_str) = remaining.strip_prefix(bound.as_str()) {
                    return match_segments_inner(&segments[1..], rest_str, captures);
                }
                return false;
            }

            let next_lit = find_next_literal(&segments[1..]);
            let limit = remaining.find('/').unwrap_or(remaining.len());

            for end in 1..=limit {
                if !remaining.is_char_boundary(end) {
                    continue;
                }
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
            false
        }

        Segment::MultiCapture(name) => {
            if let Some(bound) = captures.get(name).cloned() {
                if let Some(rest_str) = remaining.strip_prefix(bound.as_str()) {
                    return match_segments_inner(&segments[1..], rest_str, captures);
                }
                return false;
            }

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

fn find_next_literal(segments: &[Segment]) -> Option<String> {
    for seg in segments {
        match seg {
            Segment::Literal(s) => return Some(s.clone()),
            _ => continue,
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn prebound_capture_constrains() {
        let segs = parse_segment_pattern("$NAME:$TAG");

        let mut pre = HashMap::new();
        pre.insert("NAME".to_string(), "nginx".to_string());
        let result = match_segments_with_bindings(&segs, "nginx:latest", pre);
        assert!(result.is_some());
        assert_eq!(result.as_ref().unwrap()["TAG"], "latest");

        let mut pre2 = HashMap::new();
        pre2.insert("NAME".to_string(), "apache".to_string());
        let result2 = match_segments_with_bindings(&segs, "nginx:latest", pre2);
        assert!(result2.is_none());
    }

    #[test]
    fn prebound_multi_capture_constrains() {
        let segs = parse_segment_pattern("$$$PATH/file.txt");

        let mut pre = HashMap::new();
        pre.insert("PATH".to_string(), "src/lib".to_string());
        let result = match_segments_with_bindings(&segs, "src/lib/file.txt", pre);
        assert!(result.is_some());

        let mut pre2 = HashMap::new();
        pre2.insert("PATH".to_string(), "other".to_string());
        let result2 = match_segments_with_bindings(&segs, "src/lib/file.txt", pre2);
        assert!(result2.is_none());
    }

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
