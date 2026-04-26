//! `repo(...)` — filter or bind on `cursor.repo` (parse.md §14.5c).
//!
//! Two modes, v2-native:
//!   * `repo(myorg/*)` — filter: drop cursors whose `repo` fails the
//!     glob. Glob source compiles to a [`GlobOp`] via
//!     [`super::glob::compile_str`].
//!   * `repo($R)` — bind: leave the cursor otherwise untouched, append
//!     a [`Capture`] named `R` whose value is `cursor.repo`.
//!
//! Not a [`PatternOp`]. The paren body is classified at construction
//! time; the result is one of two pre-compiled modes.
//!
//! [`PatternOp`]: crate::pattern_op::PatternOp

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{ArgSpec, Value};

const REPO_ARG_SPEC: &[ArgSpec] = &[ArgSpec::Any];

#[derive(Debug)]
pub struct RepoOp {
    mode: RepoMode,
}

#[derive(Debug)]
pub enum RepoMode {
    /// Glob filter on `cursor.repo`. Holds the compiled pattern op;
    /// match via its `try_raw_regex`.
    Filter(Arc<dyn Op>),
    /// Bind `cursor.repo` into a new capture by this name.
    Bind(Arc<str>),
}

impl RepoOp {
    pub fn mode(&self) -> &RepoMode { &self.mode }

    pub fn from_source(src: &str) -> Result<Self, Vec<PatternDiagnostic>> {
        let arg = src.trim();
        if arg.is_empty() {
            return Err(vec![PatternDiagnostic {
                code: "repo/missing-arg",
                message: "repo requires a glob pattern or capture (e.g. repo(myorg/*) or repo($R))".into(),
                byte_range: 0..src.len(),
            }]);
        }
        if let Some(name) = parse_capture(arg) {
            return Ok(RepoOp { mode: RepoMode::Bind(name) });
        }
        let pat = super::glob::compile_str(arg)?;
        Ok(RepoOp { mode: RepoMode::Filter(Arc::new(pat)) })
    }

    pub fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        match values.as_slice() {
            [Value::Term { name, .. }] => Ok(RepoOp::bind(name.clone())),
            [Value::Atom(s)] | [Value::Str(s)] => {
                let pat = super::glob::compile_str(s)?;
                Ok(RepoOp { mode: RepoMode::Filter(Arc::new(pat)) })
            }
            [Value::Op(op)] if op.try_raw_regex().is_some() => {
                Ok(RepoOp { mode: RepoMode::Filter(op.clone()) })
            }
            [Value::Op(_)] => Err(vec![PatternDiagnostic {
                code: "repo/unsupported-pattern",
                message: "repo requires an op exposing a raw regex (glob or re)".into(),
                byte_range: 0..0,
            }]),
            [] => Err(vec![PatternDiagnostic {
                code: "repo/missing-arg",
                message: "repo requires a glob pattern or capture (e.g. repo(myorg/*) or repo($R))".into(),
                byte_range: 0..0,
            }]),
            _ => Err(vec![PatternDiagnostic {
                code: "value/arity-mismatch",
                message: format!("repo takes 1 argument, got {}", values.len()),
                byte_range: 0..0,
            }]),
        }
    }

    pub fn filter(pat: Arc<dyn Op>) -> Self {
        RepoOp { mode: RepoMode::Filter(pat) }
    }

    pub fn bind(name: impl Into<Arc<str>>) -> Self {
        RepoOp { mode: RepoMode::Bind(name.into()) }
    }
}

impl OpCtor for RepoOp {
    const NAME: &'static str = "repo";
    const BODY_GRAMMAR: &'static str = "glob (`*` `**` `?` `$NAME`)";
    const DOC:  &'static str = "\
**repo**(_glob_ | `$NAME`)

Filter or bind on `cursor.repo`.

- `repo(myorg/*)` — keep cursors whose repo slug matches the glob.
- `repo($R)` — bind the repo slug into capture `R`.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::from_values(values)
    }
}

impl Op for RepoOp {
    fn name(&self) -> &'static str { "repo" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            match &self.mode {
                RepoMode::Filter(op) => {
                    let re = match op.try_raw_regex() {
                        Some(r) => r,
                        None => return vec![],
                    };
                    if re.is_match(c.repo.as_bytes()) { vec![c] } else { vec![] }
                }
                RepoMode::Bind(name) => {
                    let mut out = c;
                    let v = out.repo.clone();
                    out.captures.push(Capture::synthesized(name.clone(), v));
                    out.last_bound = Some(name.clone());
                    vec![out]
                }
            }
        }))
    }

    fn language(&self) -> Option<tree_sitter::Language> {
        crate::op_languages::language_of("glob")
    }

    fn arg_spec(&self) -> &[ArgSpec] { REPO_ARG_SPEC }
}

/// `$NAME` → `NAME`. Whitespace is the caller's job.
pub(crate) fn parse_capture(s: &str) -> Option<Arc<str>> {
    let rest = s.strip_prefix('$')?;
    if rest.is_empty() { return None; }
    let bytes = rest.as_bytes();
    let first_ok = matches!(bytes[0], b'A'..=b'Z' | b'a'..=b'z' | b'_');
    if !first_ok { return None; }
    if !bytes.iter().all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')) {
        return None;
    }
    Some(Arc::from(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_cursor(repo: &str) -> Cursor {
        let mut c = Cursor::default();
        c.repo = Arc::from(repo);
        c
    }

    #[tokio::test]
    async fn filter_keeps_matching_repo_drops_rest() {
        let ctx = RtCtx::default();
        let op = RepoOp::from_source("myorg/*").unwrap();
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("myorg/alpha")])).await.len(), 1);
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("myorg/beta")])).await.len(), 1);
        assert_eq!(op.pipe(&ctx, Arc::from(vec![mk_cursor("other/alpha")])).await.len(), 0);
    }

    #[tokio::test]
    async fn bind_writes_synthesized_capture_and_sets_last_bound() {
        let ctx = RtCtx::default();
        let op = RepoOp::from_source("$R").unwrap();
        let out = op.pipe(&ctx, Arc::from(vec![mk_cursor("myorg/alpha")])).await;
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.captures.len(), 1);
        assert_eq!(&*c.captures[0].name, "R");
        assert_eq!(c.captures[0].bytes(&c.content), b"myorg/alpha");
        assert_eq!(c.last_bound.as_deref(), Some("R"));
    }

    #[test]
    fn classify_capture_rejects_malformed() {
        assert!(parse_capture("$R").is_some());
        assert!(parse_capture("$_x2").is_some());
        assert!(parse_capture("$").is_none());
        assert!(parse_capture("$1A").is_none());
        assert!(parse_capture("myorg/*").is_none());
    }

    #[test]
    fn missing_arg_diagnoses() {
        let err = match RepoOp::from_source("   ") {
            Err(e) => e,
            Ok(_) => panic!("expected repo/missing-arg"),
        };
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].code, "repo/missing-arg");
    }
}
