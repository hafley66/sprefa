//! Descriptors: the canonical value a typed path literal (`fs:`/`glob:`) resolves
//! to. A literal is `scheme:body`; the body is a sequence of `/`-separated
//! segments, each a concrete name (Dir/File) or a hole (`${NAME}`, `*`, `**`).
//! Zero holes = a concrete value (canonical repo-relative path); any hole = a
//! pattern (a glob). The scheme registry is a static table so a future `rs:` /
//! `dot:` / `url:` is one row, not a new parser.

use std::path::{Component, Path, PathBuf};

/// A resolved path literal, segment by segment. `text` is the canonical rendered
/// form (normalized repo-relative path for `fs:`, the glob pattern for `glob:`);
/// callers join it against the existing path columns or hand it to globset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desc {
    pub text: String,
    pub kind: DescKind,
    pub segments: Vec<Segment>,
}

/// The overall shape of a descriptor. v1 path schemes only ever produce `Dir`,
/// `File`, or `Hole`; the other variants reserve the namespace for `rs:`/`dot:`
/// schemes that are deferred (segment-materialization table, access paths).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DescKind {
    Namespace,
    Type_,
    Term,
    Method,
    Param,
    Index(u64),
    Dir,
    File,
    Hole(HoleKind),
}

/// A hole in a pattern. `Named` is a `${NAME}` interpolation; `Star`/`Globstar`
/// are `*` / `**` glob wildcards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleKind {
    Named(String),
    Star,
    Globstar,
}

/// One `/`-separated body segment after the generic lexer has classified it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// A concrete name (no holes). The string is the literal text.
    Name(String),
    /// A single hole occupying a whole segment (`${x}`, `*`, `**`).
    Hole(HoleKind),
    /// A segment mixing literal text and holes (`foo${x}.rs`, `*.rs`). Rendered
    /// verbatim into the glob pattern; only ever appears in a pattern desc.
    Mixed(String),
}

/// A row in the scheme registry. `name` is the bare scheme keyword (`fs`,
/// `glob`); adding a scheme later is a new row here plus a resolve arm.
#[derive(Clone, Copy, Debug)]
pub struct SchemeSpec {
    pub name: &'static str,
    /// true = a concrete path (`fs:`); false = a pattern (`glob:`). Concrete
    /// schemes reject holes; pattern schemes allow them.
    pub concrete: bool,
}

/// The v1 scheme registry. `re:` is intentionally absent: `/.../` regex literals
/// already exist and are not disturbed.
pub const SCHEMES: &[SchemeSpec] = &[
    SchemeSpec {
        name: "fs",
        concrete: true,
    },
    SchemeSpec {
        name: "glob",
        concrete: false,
    },
    // `q:{ $k: $v }` — a structural brace-pattern literal (the declarative
    // `json` op's carrier). Not a filesystem path; `concrete: false` keeps it
    // out of descriptor materialization. The body is opaque to the lexer
    // (balanced brackets) and parsed by datapath::parse_pattern.
    SchemeSpec {
        name: "q",
        concrete: false,
    },
];

pub fn scheme_spec(name: &str) -> Option<&'static SchemeSpec> {
    SCHEMES.iter().find(|s| s.name == name)
}

/// Is `name` a registered scheme keyword? Used by the lexer to decide whether an
/// `ident:` adjacency starts a typed literal.
pub fn is_scheme(name: &str) -> bool {
    scheme_spec(name).is_some()
}

/// Outcome of resolving a literal: a value (concrete canonical text) or a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// A concrete path: normalized repo-relative text. Holes-free.
    Value(Desc),
    /// A glob pattern: the canonical pattern text. Has at least one hole.
    Pattern(Desc),
}

impl Resolved {
    pub fn text(&self) -> &str {
        match self {
            Resolved::Value(d) | Resolved::Pattern(d) => &d.text,
        }
    }
    pub fn desc(&self) -> &Desc {
        match self {
            Resolved::Value(d) | Resolved::Pattern(d) => d,
        }
    }
}

/// A resolution failure carrying the spec's diagnostic code. The caller maps it
/// to a `TypeDiag` with the literal's span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DescError {
    /// The body referenced an anchor that is not `~` and not a declared name.
    UnknownAnchor(String),
    /// The resolved concrete path normalizes outside the scan root.
    PathEscapesRoot(String),
    /// A concrete scheme (`fs:`) body contained a glob/interp hole.
    HoleInConcrete,
}

impl DescError {
    pub fn code(&self) -> &'static str {
        match self {
            DescError::UnknownAnchor(_) => "unknown-anchor",
            DescError::PathEscapesRoot(_) => "path-escapes-root",
            // A hole in a concrete `fs:` literal is a scheme misuse; surface it as
            // an escapes-root-adjacent error. (No dedicated code in the v1 table.)
            DescError::HoleInConcrete => "unknown-scheme",
        }
    }
    pub fn msg(&self) -> String {
        match self {
            DescError::UnknownAnchor(a) => format!("unknown anchor `{a}` (v1 supports only `~`)"),
            DescError::PathEscapesRoot(p) => format!("path `{p}` escapes the scan root"),
            DescError::HoleInConcrete => {
                "a concrete `fs:` literal cannot contain a `${{}}` / `*` hole; use `glob:`".into()
            }
        }
    }
}

/// The one generic segment lexer. Splits a literal body into `/`-separated
/// segments and classifies each as a concrete name, a whole-segment hole, or a
/// mixed literal+hole segment. `\` escapes the next char (drops the backslash).
/// `${NAME}` is a named hole; `*` / `**` are star/globstar holes. This is scheme
/// agnostic; the resolver decides what to do with the segments.
pub fn lex_segments(body: &str) -> Vec<Segment> {
    let mut segs = Vec::new();
    for raw in split_unescaped_slash(body) {
        segs.push(classify_segment(&raw));
    }
    segs
}

/// Split on `/` honoring `\/` as an escaped slash inside a segment. Keeps the
/// escape backslash in the piece (classify_segment strips it).
fn split_unescaped_slash(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            cur.push('\\');
            if let Some(&n) = chars.peek() {
                cur.push(n);
                chars.next();
            }
        } else if c == '/' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn classify_segment(seg: &str) -> Segment {
    // Whole-segment holes.
    if seg == "**" {
        return Segment::Hole(HoleKind::Globstar);
    }
    if seg == "*" {
        return Segment::Hole(HoleKind::Star);
    }
    if let Some(name) = whole_named_hole(seg) {
        return Segment::Hole(HoleKind::Named(name));
    }
    // Mixed if it still contains a wildcard or interp after unescaping detection.
    let has_hole = seg_contains_hole(seg);
    let literal = unescape(seg);
    if has_hole {
        Segment::Mixed(literal)
    } else {
        Segment::Name(literal)
    }
}

/// `${NAME}` occupying the whole segment -> Some(NAME).
fn whole_named_hole(seg: &str) -> Option<String> {
    let inner = seg.strip_prefix("${")?.strip_suffix('}')?;
    if inner.contains('{') || inner.contains('}') {
        return None;
    }
    Some(inner.to_string())
}

/// Does the segment contain an unescaped `*` or `${`?
fn seg_contains_hole(seg: &str) -> bool {
    let mut chars = seg.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '*' => return true,
            '$' if chars.peek() == Some(&'{') => return true,
            _ => {}
        }
    }
    false
}

/// Drop escape backslashes: `\(` -> `(`, `a\/b` -> `a/b`.
fn unescape(seg: &str) -> String {
    let mut out = String::new();
    let mut chars = seg.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Resolve a `fs:`/`glob:` literal body to a descriptor. `root` is the scan root
/// (for the escapes-root check); `concrete` selects the scheme rule.
///
/// pseudo:
///   segs = lex_segments(strip ~/ prefix)
///   if concrete and any hole -> Err(HoleInConcrete)
///   if has hole -> Pattern(text = normalized glob, kind = Hole(first hole kind))
///   else -> normalize repo-relative; if escapes root -> Err; Value
pub fn resolve(body: &str, concrete: bool, anchored_root: bool) -> Result<Resolved, DescError> {
    // `~` anchor: a leading `~/` means "relative to the default anchor = scan
    // root". After stripping it the remaining body is already repo-relative, so
    // resolution is identical; the flag only records that the anchor was seen.
    let _ = anchored_root;
    let segs = lex_segments(body);
    let any_hole = segs
        .iter()
        .any(|s| matches!(s, Segment::Hole(_) | Segment::Mixed(_)));

    if concrete && any_hole {
        return Err(DescError::HoleInConcrete);
    }

    if any_hole {
        // Pattern: render the canonical glob (segments rejoined by `/`, holes
        // verbatim). The hole kind on the Desc is the first hole seen.
        let text = render_pattern(&segs);
        let kind = first_hole_kind(&segs).unwrap_or(HoleKind::Star);
        return Ok(Resolved::Pattern(Desc {
            text,
            kind: DescKind::Hole(kind),
            segments: segs,
        }));
    }

    // Concrete: normalize the repo-relative path and reject escapes.
    let joined = render_concrete(&segs);
    let norm = normalize_rel(&joined).ok_or_else(|| DescError::PathEscapesRoot(joined.clone()))?;
    let kind = if norm.is_empty() || joined.ends_with('/') {
        DescKind::Dir
    } else {
        DescKind::File
    };
    Ok(Resolved::Value(Desc {
        text: norm,
        kind,
        segments: segs,
    }))
}

fn first_hole_kind(segs: &[Segment]) -> Option<HoleKind> {
    for s in segs {
        if let Segment::Hole(h) = s {
            return Some(h.clone());
        }
        if let Segment::Mixed(m) = s {
            if m.contains("**") {
                return Some(HoleKind::Globstar);
            }
            return Some(HoleKind::Star);
        }
    }
    None
}

/// Rejoin segments into the canonical glob pattern. Holes render to their glob
/// form (`*`, `**`, `${name}`); names/mixed render their literal text.
fn render_pattern(segs: &[Segment]) -> String {
    let parts: Vec<String> = segs
        .iter()
        .map(|s| match s {
            Segment::Name(n) => n.clone(),
            Segment::Mixed(m) => m.clone(),
            Segment::Hole(HoleKind::Star) => "*".into(),
            Segment::Hole(HoleKind::Globstar) => "**".into(),
            Segment::Hole(HoleKind::Named(n)) => format!("${{{n}}}"),
        })
        .collect();
    parts.join("/")
}

fn render_concrete(segs: &[Segment]) -> String {
    let parts: Vec<String> = segs
        .iter()
        .map(|s| match s {
            Segment::Name(n) => n.clone(),
            // Concrete path can't have holes (checked earlier); render best-effort.
            Segment::Mixed(m) => m.clone(),
            Segment::Hole(_) => String::new(),
        })
        .collect();
    parts.join("/")
}

/// Normalize a repo-relative path: collapse `.` / `..` / empty segments. Returns
/// None if it escapes above the root (a leading `..` that pops past the top).
pub fn normalize_rel(rel: &str) -> Option<String> {
    // Reject absolute paths: a `fs:/abs` body is outside the repo-relative model.
    let p = Path::new(rel);
    let mut stack: Vec<String> = Vec::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return None;
                }
            }
            Component::Normal(s) => stack.push(s.to_string_lossy().to_string()),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(stack.join("/"))
}

/// Canonicalize a body that may carry a leading `~/` anchor. Returns
/// (anchored, remaining_body). In v1 only `~` is supported as an anchor; a
/// named-anchor prefix (`name/`) is NOT special here (deferred with `rs:`).
pub fn strip_anchor(body: &str) -> (bool, &str) {
    if let Some(rest) = body.strip_prefix("~/") {
        (true, rest)
    } else if body == "~" {
        (true, "")
    } else {
        (false, body)
    }
}

/// Unused-import guard for the reserved DescKind variants (Namespace/Type_/...);
/// keeps the enum from tripping dead-code lints before `rs:` lands.
#[allow(dead_code)]
fn _reserved_kinds() -> [DescKind; 6] {
    [
        DescKind::Namespace,
        DescKind::Type_,
        DescKind::Term,
        DescKind::Method,
        DescKind::Param,
        DescKind::Index(0),
    ]
}

#[allow(dead_code)]
fn _pathbuf_anchor(root: &Path, rel: &str) -> PathBuf {
    root.join(rel)
}
