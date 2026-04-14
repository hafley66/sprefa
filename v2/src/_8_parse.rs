//! Pure parse utilities. No I/O. No state.

use std::ops::Range;
use std::sync::Arc;
use std::path::Path;

use crate::_0_types::{ParseSeg, ParseSite};
use crate::_5_op::{BraceSlot, BracketSlot, OpInvocation, ParenSlot};

// ---------------------------------------------------------------------------
// Token classification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenClass {
    Capture(CaptureRef),
    Provenance(ProvRef),
    CrossRef(CrossRefRef),
    Literal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRef { pub name: Arc<str> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvRef {
    pub kind: ProvKind,
    pub norm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvKind { Repo, Rev, Fs }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossRefRef {
    pub rule: Arc<str>,
    pub var:  Arc<str>,
}

pub fn parse_capture(s: &str) -> Option<CaptureRef> {
    let rest = s.strip_prefix('$')?;
    if rest.starts_with('$') || rest.starts_with('{') { return None; }
    if rest.is_empty() || !is_ident_start(rest.as_bytes()[0]) { return None; }
    if !rest.bytes().all(is_ident_byte) { return None; }
    Some(CaptureRef { name: Arc::from(rest) })
}

pub fn parse_provenance(s: &str) -> Option<ProvRef> {
    let rest = s.strip_prefix("$$")?;
    let (name, norm) = match rest.strip_suffix(".norm") {
        Some(n) => (n, true),
        None    => (rest, false),
    };
    let kind = match name {
        "repo" => ProvKind::Repo,
        "rev"  => ProvKind::Rev,
        "fs"   => ProvKind::Fs,
        _      => return None,
    };
    Some(ProvRef { kind, norm })
}

pub fn parse_cross_ref(s: &str) -> Option<CrossRefRef> {
    let inner = s.strip_prefix("${")?.strip_suffix('}')?;
    let (rule, rest) = inner.split_once('.')?;
    let var = rest.strip_prefix('$')?;
    if rule.is_empty() || var.is_empty() { return None; }
    if !rule.bytes().all(is_ident_byte) || !var.bytes().all(is_ident_byte) { return None; }
    Some(CrossRefRef { rule: Arc::from(rule), var: Arc::from(var) })
}

pub fn classify_token(s: &str) -> TokenClass {
    if let Some(c) = parse_cross_ref(s)   { return TokenClass::CrossRef(c); }
    if let Some(p) = parse_provenance(s)  { return TokenClass::Provenance(p); }
    if let Some(c) = parse_capture(s)     { return TokenClass::Capture(c); }
    TokenClass::Literal
}

fn is_ident_start(b: u8) -> bool { b.is_ascii_alphabetic() || b == b'_' }
fn is_ident_byte (b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' }

// ---------------------------------------------------------------------------
// Balanced bracket scanner
// ---------------------------------------------------------------------------

/// Given `src` positioned at an opening delimiter, return the byte range of
/// the matched inner (exclusive of delimiters). Returns None on EOF or mismatch.
pub fn scan_balanced(src: &str, open: u8, close: u8) -> Option<Range<usize>> {
    let bytes = src.as_bytes();
    if bytes.first() != Some(&open) { return None; }
    let mut depth = 0u32;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == open  { depth += 1; }
        if c == close { depth -= 1; if depth == 0 { return Some(1..i); } }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Host parser
//
// Grammar (v2.1):
//   program  := stmt*
//   stmt     := chain ';'?
//   chain    := op ('>' op)*
//   op       := IDENT ('[' bracket ']')? ('(' paren ')')? ('{' brace '}')?
//
// Line comments: `#` to end of line.
// `$NAME`, `$$prov`, `${rule.$V}` are leaf-level tokens classified by ops at
// parse time; the host parser does not look at them.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: Arc<str>,
    pub offset:  usize,
}

/// One Rx-style pipe of ops chained with `>`. Lowers to `Pipeline::Seq`.
#[derive(Debug, Clone)]
pub struct Pipe {
    pub ops: Vec<OpInvocation>,
}

fn skip_trivia(bytes: &[u8], mut i: usize) -> usize {
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i < bytes.len() && bytes[i] == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            continue;
        }
        break;
    }
    i
}

/// What kind of seg the host parser appends to each op's parse path.
/// Top for top-level program, BraceChild for re-entry on a brace body, etc.
#[derive(Debug, Clone, Copy)]
pub enum ChildKind {
    Top,
    BraceChild,
    ParenChild,
}

pub fn host_parse(
    src:  &str,
    file: Arc<Path>,
) -> Result<Vec<Pipe>, Vec<ParseError>> {
    host_parse_inner(src, file, &[], ChildKind::Top, false, 0)
}

/// Re-enter the parser on a brace body. `parent_path` = enclosing op's
/// `parse_site.path` so emitted ParseSites are `<parent>/BraceChild[N]/...`.
pub fn host_parse_brace(
    src:         &str,
    file:        Arc<Path>,
    parent_path: &[ParseSeg],
) -> Result<Vec<Pipe>, Vec<ParseError>> {
    host_parse_inner(src, file, parent_path, ChildKind::BraceChild, false, 0)
}

/// Re-enter the parser on a fork-arm brace body. Each pipe must begin with `>`.
///
/// `base_offset` is the absolute byte offset in the original source where
/// `src` (the brace body substring) starts. Pass 0 for top-level.
pub fn host_parse_arm_brace(
    src:         &str,
    file:        Arc<Path>,
    parent_path: &[ParseSeg],
) -> Result<Vec<Pipe>, Vec<ParseError>> {
    host_parse_inner(src, file, parent_path, ChildKind::BraceChild, true, 0)
}

/// Same as `host_parse_arm_brace` but preserves absolute source offsets by
/// adding `base_offset` to every emitted `byte_range`.
pub fn host_parse_arm_brace_abs(
    src:         &str,
    file:        Arc<Path>,
    parent_path: &[ParseSeg],
    base_offset: usize,
) -> Result<Vec<Pipe>, Vec<ParseError>> {
    host_parse_inner(src, file, parent_path, ChildKind::BraceChild, true, base_offset)
}

fn host_parse_inner(
    src:                &str,
    file:               Arc<Path>,
    parent_path:        &[ParseSeg],
    kind:               ChildKind,
    expect_leading_arm: bool,
    base_offset:        usize,
) -> Result<Vec<Pipe>, Vec<ParseError>> {
    let bytes      = src.as_bytes();
    let mut i      = 0usize;
    let mut pipes  = Vec::new();
    let mut child_ix = 0u16;

    loop {
        i = skip_trivia(bytes, i);
        if i >= bytes.len() { break; }

        if expect_leading_arm {
            if i >= bytes.len() || bytes[i] != b'>' {
                return Err(vec![ParseError {
                    message: Arc::from("expected `>` at start of fork arm (arms must begin with `>`)"),
                    offset:  i,
                }]);
            }
            i += 1;
            i = skip_trivia(bytes, i);
        }

        let mut ops = Vec::new();
        loop {
            let inv = parse_op(src, bytes, &mut i, &file, parent_path, kind, child_ix, base_offset)?;
            child_ix += 1;
            ops.push(inv);

            i = skip_trivia(bytes, i);
            if i < bytes.len() && bytes[i] == b'>' {
                i += 1;
                i = skip_trivia(bytes, i);
                continue;
            }
            break;
        }

        i = skip_trivia(bytes, i);
        if i < bytes.len() && bytes[i] == b';' { i += 1; }

        pipes.push(Pipe { ops });
    }

    Ok(pipes)
}

fn parse_op(
    src:         &str,
    bytes:       &[u8],
    i:           &mut usize,
    file:        &Arc<Path>,
    parent_path: &[ParseSeg],
    kind:        ChildKind,
    child_ix:    u16,
    base_offset: usize,
) -> Result<OpInvocation, Vec<ParseError>> {
    let inv_start = *i;
    if *i >= bytes.len() || !is_ident_start(bytes[*i]) {
        return Err(vec![ParseError {
            message: Arc::from("expected identifier (op name)"),
            offset:  *i,
        }]);
    }
    let name_start = *i;
    while *i < bytes.len() && is_ident_byte(bytes[*i]) { *i += 1; }
    let name = Arc::<str>::from(&src[name_start..*i]);

    let mut brackets = Vec::new();
    *i = skip_trivia(bytes, *i);
    while *i < bytes.len() && bytes[*i] == b'[' {
        let inner = scan_balanced(&src[*i..], b'[', b']').ok_or_else(|| {
            vec![ParseError { message: Arc::from("unclosed `[`"), offset: *i }]
        })?;
        let abs = (*i + inner.start)..(*i + inner.end);
        brackets.push(BracketSlot {
            src:        Arc::from(&src[abs.clone()]),
            byte_range: (abs.start + base_offset)..(abs.end + base_offset),
        });
        *i = abs.end + 1;
        *i = skip_trivia(bytes, *i);
    }

    let paren_src = if *i < bytes.len() && bytes[*i] == b'(' {
        let inner = scan_balanced(&src[*i..], b'(', b')').ok_or_else(|| {
            vec![ParseError { message: Arc::from("unclosed `(`"), offset: *i }]
        })?;
        let abs = (*i + inner.start)..(*i + inner.end);
        let slot = ParenSlot {
            src:        Arc::from(&src[abs.clone()]),
            byte_range: (abs.start + base_offset)..(abs.end + base_offset),
        };
        *i = abs.end + 1;
        Some(slot)
    } else { None };

    *i = skip_trivia(bytes, *i);
    let brace_src = if *i < bytes.len() && bytes[*i] == b'{' {
        let inner = scan_balanced(&src[*i..], b'{', b'}').ok_or_else(|| {
            vec![ParseError { message: Arc::from("unclosed `{`"), offset: *i }]
        })?;
        let abs = (*i + inner.start)..(*i + inner.end);
        let slot = BraceSlot {
            src:        Arc::from(&src[abs.clone()]),
            byte_range: (abs.start + base_offset)..(abs.end + base_offset),
        };
        *i = abs.end + 1;
        Some(slot)
    } else { None };

    let leaf_seg = match kind {
        ChildKind::Top        => ParseSeg::Top        { index: child_ix },
        ChildKind::BraceChild => ParseSeg::BraceChild { index: child_ix },
        ChildKind::ParenChild => ParseSeg::ParenChild { index: child_ix },
    };
    let mut path: Vec<ParseSeg> = parent_path.to_vec();
    path.push(leaf_seg);

    let parse_site = Arc::new(ParseSite {
        file:       file.clone(),
        path:       Arc::from(path.into_boxed_slice()),
        byte_range: (inv_start + base_offset)..(*i + base_offset),
    });

    Ok(OpInvocation { name, brackets, paren_src, brace_src, parse_site })
}

// ---------------------------------------------------------------------------
// Small helpers, kept from prior pass
// ---------------------------------------------------------------------------

pub fn glob_match(pattern: &str, s: &str) -> bool {
    // Normalize: **/foo should also match foo at the root (zero dir components).
    // Strip any leading **/ and recursively try the suffix.
    if let Some(rest) = pattern.strip_prefix("**/") {
        if glob_match(rest, s) { return true; }
    }
    let pb = pattern.as_bytes();
    let sb = s.as_bytes();
    glob_rec(pb, 0, sb, 0)
}

fn glob_rec(p: &[u8], pi: usize, s: &[u8], si: usize) -> bool {
    if pi == p.len() { return si == s.len(); }
    match p[pi] {
        b'*' => (si..=s.len()).any(|k| glob_rec(p, pi + 1, s, k)),
        b'?' => si < s.len() && glob_rec(p, pi + 1, s, si + 1),
        c    => si < s.len() && s[si] == c && glob_rec(p, pi + 1, s, si + 1),
    }
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j-1] + 1).min(prev[j-1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fake_file() -> Arc<Path> {
        Arc::from(PathBuf::from("test.sprf").as_path())
    }

    // T1: empty input yields zero pipes
    #[test]
    fn arm_brace_empty() {
        let pipes = host_parse_arm_brace("", fake_file(), &[]).unwrap();
        assert!(pipes.is_empty());
    }

    // T2: whitespace-only yields zero pipes
    #[test]
    fn arm_brace_whitespace_only() {
        let pipes = host_parse_arm_brace("   \n  ", fake_file(), &[]).unwrap();
        assert!(pipes.is_empty());
    }

    // T3: single arm with one op
    #[test]
    fn arm_brace_single_arm_one_op() {
        let pipes = host_parse_arm_brace("> foo", fake_file(), &[]).unwrap();
        assert_eq!(pipes.len(), 1);
        assert_eq!(pipes[0].ops.len(), 1);
        assert_eq!(&*pipes[0].ops[0].name, "foo");
    }

    // T4: single arm with chained ops separated by `>`
    #[test]
    fn arm_brace_single_arm_chain() {
        let pipes = host_parse_arm_brace("> foo > bar > baz", fake_file(), &[]).unwrap();
        assert_eq!(pipes.len(), 1);
        assert_eq!(pipes[0].ops.len(), 3);
        assert_eq!(&*pipes[0].ops[0].name, "foo");
        assert_eq!(&*pipes[0].ops[1].name, "bar");
        assert_eq!(&*pipes[0].ops[2].name, "baz");
    }

    // T5: two arms separated by `;`
    #[test]
    fn arm_brace_two_arms() {
        let pipes = host_parse_arm_brace("> foo;\n> bar", fake_file(), &[]).unwrap();
        assert_eq!(pipes.len(), 2);
        assert_eq!(&*pipes[0].ops[0].name, "foo");
        assert_eq!(&*pipes[1].ops[0].name, "bar");
    }

    // T6: arms separated by `;`
    #[test]
    fn arm_brace_semicolon_separated() {
        let pipes = host_parse_arm_brace("> foo; > bar", fake_file(), &[]).unwrap();
        assert_eq!(pipes.len(), 2);
        assert_eq!(&*pipes[0].ops[0].name, "foo");
        assert_eq!(&*pipes[1].ops[0].name, "bar");
    }

    // T7: missing leading `>` on first arm is an error
    #[test]
    fn arm_brace_missing_leading_arrow_error() {
        let errs = host_parse_arm_brace("foo", fake_file(), &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(&*errs[0].message, "expected `>` at start of fork arm (arms must begin with `>`)");
        assert_eq!(errs[0].offset, 0);
    }

    // T8: missing leading `>` on second arm (after valid first) is an error
    #[test]
    fn arm_brace_missing_arrow_second_arm() {
        let errs = host_parse_arm_brace("> foo\nbar", fake_file(), &[]).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(&*errs[0].message, "expected `>` at start of fork arm (arms must begin with `>`)");
    }

    // T9: error offset points at the non-`>` character
    #[test]
    fn arm_brace_error_offset() {
        let src = "   foo";
        let errs = host_parse_arm_brace(src, fake_file(), &[]).unwrap_err();
        assert_eq!(errs[0].offset, 3);
    }

    // T10: host_parse and host_parse_brace are unaffected (no leading `>` required)
    #[test]
    fn plain_parse_unaffected() {
        let pipes = host_parse("foo > bar", fake_file()).unwrap();
        assert_eq!(pipes.len(), 1);
        assert_eq!(pipes[0].ops.len(), 2);

        let pipes2 = host_parse_brace("baz", fake_file(), &[]).unwrap();
        assert_eq!(pipes2.len(), 1);
        assert_eq!(&*pipes2[0].ops[0].name, "baz");
    }
}
