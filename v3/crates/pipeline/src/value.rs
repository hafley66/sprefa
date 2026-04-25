//! First-class Value category for the op arg surface (post-§14.5m).
//!
//! One shape composable everywhere: cursor[] -> cursor[]. An argument
//! is either a scalar leaf (Atom/Str/Int/Float), a lexical capture
//! reference (`$NAME` / Term), or an op. Patterns and sub-pipelines
//! both land here as `Value::Op(Arc<dyn Op>)` — the thing that matters
//! at the outer op's inspection point is the trait surface, not the
//! enum discriminant.
//!
//! The outer op consults [`Op::try_raw_regex`], [`Op::materialize_with`],
//! and [`Op::bound_captures`] to bulk-dispatch without a concrete-type
//! downcast. Concrete pattern structs (the prior `GlobPattern`,
//! `RegexPattern`) are fused into their ops' state.
//!
//! # Seg and the shared materialization helpers
//!
//! `Seg` remains the per-pattern template element: `Fragment` is
//! already-escaped regex bytes; `Term` is a `$NAME` hole with a per-op
//! unbound substitution (`glob` → `[^/]+`, `re` → `.*?`).
//! [`materialize_template`] rebuilds a regex from a template + bindings.
//! [`apply_identity_pattern`] is the shared drive-loop for identity-
//! projection pattern ops (both `glob` and `re` reuse it).

use std::collections::HashMap;
use std::sync::Arc;

use regex::bytes::Regex;

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;

// ---------------------------------------------------------------------------
// Value
// ---------------------------------------------------------------------------

/// Whatever an op can receive in an argument slot.
#[derive(Clone)]
pub enum Value {
    /// `:name` — interned symbol.
    Atom(Arc<str>),

    /// `"bytes"` — byte string literal.
    Str(Arc<str>),

    /// `42`, `-1`, `0x10` — integer literal.
    Int(i64),

    /// `1.5` — float literal.
    Float(f64),

    /// `${NAME}` (read) or `${NAME?}` (unbound introducer) — capture
    /// reference. Resolved via the binding graph at drive time. The
    /// resolver looks up the current binding (read mode) or introduces
    /// a fresh binding (unbound mode) on the cursor's captures.
    Term { name: Arc<str>, mode: TermMode },

    /// Any op instance in arg position: pattern ops (`re(...)`,
    /// `glob(...)`, ...), nested pipelines, and arbitrary sub-pipes.
    /// Outer ops inspect via [`Op::try_raw_regex`], consume as
    /// sub-pipelines via `op.pipe(...)`, or read declared captures via
    /// [`Op::bound_captures`].
    Op(Arc<dyn Op>),
}

/// Syntactic mode for a term reference. Locked at lower time from the
/// `${NAME}` vs `${NAME?}` surface (sprefa-9lt). No runtime-inferred
/// path any more: BindingGraph checks the mode statically.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TermMode {
    /// `${NAME}` — must already be bound at this point. Static error
    /// if no upstream writer in this arm.
    Read,
    /// `${NAME?}` — introducer. Must NOT be bound at this point; the
    /// hole binds from the match (pattern ops) or writes from the cursor
    /// (tag/capture_write). Static error if the name is already in scope.
    Unbound,
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Atom(a) => write!(f, "Atom({a:?})"),
            Value::Str(s) => write!(f, "Str({s:?})"),
            Value::Int(n) => write!(f, "Int({n})"),
            Value::Float(n) => write!(f, "Float({n})"),
            Value::Term { name, mode } => write!(f, "Term({name:?}, {mode:?})"),
            Value::Op(op) => write!(f, "Op({})", op.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Template segment + shared materialization
// ---------------------------------------------------------------------------

/// Pattern compile lowers each grammar child into one of these. The
/// template is a sequence of segments that together reconstruct the
/// regex source given a binding map.
#[derive(Clone, Debug)]
pub enum Seg {
    /// Verbatim regex source. Already escaped at compile time.
    Fragment(Arc<str>),
    /// `$NAME` hole. Write mode: becomes `(?P<NAME><unbound_re>)`.
    /// Read mode (upstream binding present): substituted with the
    /// escaped bound value.
    Term {
        name: Arc<str>,
        unbound_re: Arc<str>,
        mode: TermMode,
    },
}

/// Rebuild a regex from a template + anchor pair with the given bindings.
/// Unbound holes expand to `(?P<NAME><unbound_re>)`. Bound holes expand
/// to the regex-escaped UTF-8 string.
pub fn materialize_template(
    template: &[Seg],
    anchors: (&str, &str),
    bindings: &HashMap<Arc<str>, Vec<u8>>,
) -> Result<Regex, regex::Error> {
    let (prefix, suffix) = anchors;
    let mut out = String::from(prefix);
    for seg in template {
        match seg {
            Seg::Fragment(s) => out.push_str(s),
            Seg::Term { name, unbound_re, .. } => match bindings.get(name) {
                Some(bytes) => {
                    let s = std::str::from_utf8(bytes).map_err(|_| {
                        regex::Error::Syntax("bound term is not valid UTF-8".into())
                    })?;
                    out.push_str(&regex::escape(s));
                }
                None => {
                    out.push_str("(?P<");
                    out.push_str(name);
                    out.push('>');
                    out.push_str(unbound_re);
                    out.push(')');
                }
            },
        }
    }
    out.push_str(suffix);
    Regex::new(&out)
}

/// Shared apply logic for identity-projection pattern ops (Regex, Glob).
///
/// Term-binding dispatch:
///   - Unbound hole → write mode (cached regex, named group match →
///     new Capture on output cursor).
///   - Bound hole → read mode (rebuild regex with substituted literal,
///     no new Capture written for that hole).
pub fn apply_identity_pattern(
    cached_regex: &Regex,
    bound_captures: &[Arc<str>],
    template: &[Seg],
    anchors: (&str, &str),
    c: &Cursor,
) -> Vec<Cursor> {
    let bindings = collect_bindings(bound_captures, c);
    if bindings.is_empty() {
        return apply_raw_regex(cached_regex, bound_captures, c);
    }
    let unbound: Vec<Arc<str>> = bound_captures
        .iter()
        .filter(|n| !bindings.contains_key(*n))
        .cloned()
        .collect();
    match materialize_template(template, anchors, &bindings) {
        Ok(r) => apply_raw_regex(&r, &unbound, c),
        Err(_) => Vec::new(),
    }
}

/// Collect the subset of `holes` that are already bound upstream.
fn collect_bindings(holes: &[Arc<str>], c: &Cursor) -> HashMap<Arc<str>, Vec<u8>> {
    let mut out = HashMap::new();
    for name in holes {
        if let Some(cap) = c.capture(name) {
            out.insert(name.clone(), cap.bytes(&c.content).to_vec());
        }
    }
    out
}

fn apply_raw_regex(re: &Regex, bound: &[Arc<str>], c: &Cursor) -> Vec<Cursor> {
    let active = c.active();
    let base = c.byte_range.start;
    let mut out = Vec::new();
    for caps in re.captures_iter(active) {
        let whole = match caps.get(0) {
            Some(m) => m,
            None => continue,
        };
        let mut next = c.narrow((base + whole.start())..(base + whole.end()));
        for name in bound {
            if let Some(m) = caps.name(name) {
                next.captures.push(Capture::span_backed(
                    name.clone(),
                    (base + m.start())..(base + m.end()),
                ));
            }
        }
        out.push(next);
    }
    out
}

// ---------------------------------------------------------------------------
// ArgSpec — what an op expects in each arg slot.
// ---------------------------------------------------------------------------

/// Per-slot expectation. The lowerer consults this to reject obvious
/// misuses (atom into an op slot, etc.). Ops declare via [`Op::arg_spec`].
#[derive(Clone, Copy, Debug)]
pub enum ArgSpec {
    /// `:name`. Lowerer rejects anything else in this slot.
    Atom,
    /// `"..."` byte string.
    Str,
    /// Integer literal.
    Int,
    /// Float literal.
    Float,
    /// `$NAME` capture reference.
    Term,
    /// Any op invocation. Covers both pattern ops and sub-pipelines;
    /// consumers distinguish via the trait methods [`Op::try_raw_regex`],
    /// [`Op::bound_captures`], etc.
    Op,
    /// Consumer inspects. Lowerer accepts any shape; outer op matches
    /// on the resulting [`Value`].
    Any,
}
