//! Pattern matchers used by the walker for key-globs and leaf-segment
//! patterns. Ported from v2/src/_16_pattern.rs. Three matcher kinds:
//! `re:` regex (with `$NAME` sugar → named groups), segment capture
//! (`$ORG/$REPO`), and globset glob (with `|` alternation).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use globset::{Glob, GlobMatcher};
use regex::Regex;

#[derive(Debug, Clone)]
pub enum Segment {
    Literal(String),
    Capture(String),
    MultiCapture(String),
    Wild,
    MultiWild,
}

#[derive(Clone)]
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

#[derive(Clone)]
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
    pub fn compile(src: &str) -> anyhow_lite::Result<Self> {
        Ok(Self {
            src:      Arc::from(src),
            matchers: compile_pattern(src)?,
        })
    }
    pub fn is_match(&self, s: &str) -> bool {
        self.matchers.iter().any(|m| m.is_match(s))
    }
}

pub fn parse_segment_pattern(pattern: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut chars = pattern.chars().peekable();
    let mut literal = String::new();

    while let Some(&c) = chars.peek() {
        if c == '$' {
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }

            chars.next();
            let multi = chars.peek() == Some(&'$') && {
                let mut lookahead = chars.clone();
                lookahead.next();
                lookahead.peek() == Some(&'$')
            };

            if multi { chars.next(); chars.next(); }

            let name = if chars.peek() == Some(&'{') {
                chars.next();
                let mut n = String::new();
                while let Some(&nc) = chars.peek() {
                    if nc == '}' { chars.next(); break; }
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
                    } else { break; }
                }
                n
            };

            if name == "_" || name.is_empty() {
                if multi { segments.push(Segment::MultiWild); }
                else { segments.push(Segment::Wild); }
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

    if !literal.is_empty() { segments.push(Segment::Literal(literal)); }
    segments
}

pub fn match_segments_with_bindings(
    segments: &[Segment],
    value:    &str,
    pre_bound: HashMap<String, String>,
) -> Option<HashMap<String, String>> {
    let mut captures = pre_bound;
    match_segments_inner(segments, value, &mut captures).then_some(captures)
}

pub fn compile_pattern(pattern: &str) -> anyhow_lite::Result<Vec<PatternMatcher>> {
    compile_patterns(&[pattern])
}

pub fn compile_patterns(patterns: &[&str]) -> anyhow_lite::Result<Vec<PatternMatcher>> {
    let mut matchers = Vec::new();
    for p in patterns {
        if let Some(re_pattern) = p.strip_prefix("re:") {
            let src = if has_dollar_capture(re_pattern) {
                Cow::Owned(rewrite_re_dollar_captures(re_pattern))
            } else {
                Cow::Borrowed(re_pattern)
            };
            matchers.push(PatternMatcher::Regex(
                Regex::new(&src).map_err(|e| anyhow_lite::msg(format!("regex: {e}")))?,
            ));
        } else if p.contains('$') {
            matchers.push(PatternMatcher::SegmentCapture(parse_segment_pattern(p)));
        } else {
            for segment in p.split('|') {
                let segment = segment.trim();
                let normalized = normalize_folder_glob(segment);
                matchers.push(PatternMatcher::Glob(
                    Glob::new(&normalized)
                        .map_err(|e| anyhow_lite::msg(format!("glob: {e}")))?
                        .compile_matcher(),
                ));
            }
        }
    }
    Ok(matchers)
}

pub fn normalize_folder_glob(segment: &str) -> Cow<'_, str> {
    if segment == "/" || !segment.ends_with('/') {
        return Cow::Borrowed(segment);
    }
    let stripped = &segment[..segment.len() - 1];
    if stripped.starts_with("**") || stripped.starts_with('/') || stripped.contains('$') {
        return Cow::Owned(format!("{stripped}/**"));
    }
    Cow::Owned(format!("**/{stripped}/**"))
}

fn match_segments(segments: &[Segment], value: &str) -> Option<HashMap<String, String>> {
    let mut captures = HashMap::new();
    match_segments_inner(segments, value, &mut captures).then_some(captures)
}

fn match_segments_inner(
    segments:  &[Segment],
    remaining: &str,
    captures:  &mut HashMap<String, String>,
) -> bool {
    if segments.is_empty() { return remaining.is_empty(); }

    match &segments[0] {
        Segment::Literal(lit) => {
            if let Some(rest) = remaining.strip_prefix(lit.as_str()) {
                match_segments_inner(&segments[1..], rest, captures)
            } else { false }
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
                if !remaining.is_char_boundary(end) { continue; }
                if let Some(ref lit) = next_lit {
                    if !remaining[end..].starts_with(lit.as_str()) { continue; }
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
                if !remaining.is_char_boundary(end) { continue; }
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
                if !remaining.is_char_boundary(end) { continue; }
                if match_segments_inner(&segments[1..], &remaining[end..], captures) { return true; }
            }
            false
        }
        Segment::MultiWild => {
            for end in 0..=remaining.len() {
                if !remaining.is_char_boundary(end) { continue; }
                if match_segments_inner(&segments[1..], &remaining[end..], captures) { return true; }
            }
            false
        }
    }
}

fn find_next_literal(segments: &[Segment]) -> Option<String> {
    for seg in segments {
        if let Segment::Literal(s) = seg { return Some(s.clone()); }
    }
    None
}

fn has_dollar_capture(s: &str) -> bool {
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] != b'$' { continue; }
        if i > 0 && bytes[i - 1] == b'\\' { continue; }
        let Some(&c) = bytes.get(i + 1) else { continue; };
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'{' || c == b'$' { return true; }
    }
    false
}

pub fn rewrite_re_dollar_captures(pattern: &str) -> String {
    let segments = parse_segment_pattern(pattern);
    let mut out = String::new();
    for seg in &segments {
        match seg {
            Segment::Literal(s)         => out.push_str(s),
            Segment::Capture(name)      => out.push_str(&format!("(?P<{}>[a-zA-Z0-9._/-]+)", name)),
            Segment::MultiCapture(name) => out.push_str(&format!("(?P<{}>.+)", name)),
            Segment::Wild               => out.push_str("\\S+"),
            Segment::MultiWild          => out.push_str(".+"),
        }
    }
    out
}

/// Lightweight error wrapper. Avoids pulling in anyhow as a workspace dep.
pub mod anyhow_lite {
    use std::fmt;

    #[derive(Debug)]
    pub struct Error(pub String);

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0) }
    }
    impl std::error::Error for Error {}

    pub type Result<T> = std::result::Result<T, Error>;
    pub fn msg<S: Into<String>>(s: S) -> Error { Error(s.into()) }
}

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
    fn match_org_repo_via_compile() {
        let ms = compile_patterns(&["$ORG/$REPO"]).unwrap();
        assert!(ms[0].is_match("acme/frontend"));
        let caps = ms[0].captures("acme/frontend").unwrap();
        assert_eq!(caps["ORG"], "acme");
        assert_eq!(caps["REPO"], "frontend");
    }

    #[test]
    fn glob_with_alt() {
        let ms = compile_patterns(&["*.json|*.yaml"]).unwrap();
        assert_eq!(ms.len(), 2);
        assert!(ms[0].is_match("a.json"));
        assert!(ms[1].is_match("a.yaml"));
    }

    #[test]
    fn re_dollar_sugar() {
        let ms = compile_patterns(&["re:$VER-$SHA"]).unwrap();
        let caps = ms[0].captures("v1.2.3-abc").unwrap();
        assert_eq!(caps["VER"], "v1.2.3");
        assert_eq!(caps["SHA"], "abc");
    }

    #[test]
    fn prebound_constrains() {
        let segs = parse_segment_pattern("$NAME:$TAG");
        let mut pre = HashMap::new();
        pre.insert("NAME".to_string(), "nginx".to_string());
        let r = match_segments_with_bindings(&segs, "nginx:latest", pre).unwrap();
        assert_eq!(r["TAG"], "latest");

        let mut bad = HashMap::new();
        bad.insert("NAME".to_string(), "apache".to_string());
        assert!(match_segments_with_bindings(&segs, "nginx:latest", bad).is_none());
    }
}
