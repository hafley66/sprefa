//! $repo — filter/bind on `cursor.repo`.
//!
//! Paren slot is either:
//!   - a glob pattern (e.g. `myorg/*`) → filter cursors whose repo matches
//!   - a capture (e.g. `$R`)           → bind `R` to `cursor.repo`, pass all through
//!
//! This op does not expand cursors; seeding comes from `$rule`.

use std::sync::Arc;

use futures_util::stream::StreamExt;
use futures_core::stream::BoxStream;

use crate::_0_types::{Capture, Cursor, ParseSite};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation, Operator, Pipeline, ProgramCtx,
};
use crate::_8_parse::{classify_token, glob_match, TokenClass};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct RepoFactory;

impl Operator for RepoFactory {
    fn name(&self) -> &'static str { "repo" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("repo-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "repo".to_string(),
            detail: "repo(glob | $CAP)".to_string(),
            doc:    "# repo\n\nFilter cursors by repo slug glob, or bind the slug to a capture.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(RepoDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let mode = match classify_token(arg) {
            TokenClass::Capture(c)     => RepoMode::Bind(c.name),
            TokenClass::Literal if arg == "*" => {
                return Err(vec![Box::new(RepoDiag::NoOp {
                    site: (*inv.parse_site).clone(),
                }) as _]);
            }
            TokenClass::Literal        => RepoMode::Filter(Arc::from(arg)),
            TokenClass::Provenance(_)
          | TokenClass::CrossRef(_)    => {
                return Err(vec![Box::new(RepoDiag::BadArg {
                    site: (*inv.parse_site).clone(),
                    got:  Arc::from(arg),
                }) as _]);
            }
        };
        Ok(Pipeline::Op(Arc::new(RepoOp {
            mode,
            parse_site: inv.parse_site.clone(),
        })))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct RepoOp {
    mode:       RepoMode,
    parse_site: Arc<ParseSite>,
}

enum RepoMode {
    Filter(Arc<str>),   // glob
    Bind(Arc<str>),     // capture name
}

impl Op for RepoOp {
    fn name(&self) -> &'static str { "repo" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> { Some(c.repo.clone()) }

    fn capture_name(&self) -> Option<Arc<str>> {
        match &self.mode { RepoMode::Bind(n) => Some(n.clone()), _ => None }
    }

    fn hover_self(&self) -> String {
        let src = match &self.mode {
            RepoMode::Filter(g) => g.as_ref().to_string(),
            RepoMode::Bind(n)   => format!("${}", n),
        };
        format!("# repo({})", src)
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let mut vals: Vec<&str> = Vec::new();
        for c in cursors {
            if let Some(capture) = c.captures.get(cap) {
                let v = capture.value.as_ref();
                if !vals.contains(&v) {
                    vals.push(v);
                    if vals.len() >= 20 { break; }
                }
            }
        }
        if vals.is_empty() { return None; }
        let lines: Vec<String> = vals.iter().map(|v| format!("- `{}`", v)).collect();
        Some(format!("**`${cap}`** repos:\n\n{}", lines.join("\n")))
    }

    fn hover_match(&self, site: &crate::_0_types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let mut vals: Vec<&str> = Vec::new();
        for c in cursors {
            let touched = c.evidence.iter().any(|ev|
                ev.op_name == "repo" && ev.parse_site.as_ref() == site
            );
            if !touched { continue; }
            let v = c.repo.as_ref();
            if !vals.contains(&v) {
                vals.push(v);
                if vals.len() >= 20 { break; }
            }
        }
        if vals.is_empty() { return None; }
        let lines: Vec<String> = vals.iter().map(|v| format!("- `{}`", v)).collect();
        Some(format!("matches:\n\n{}", lines.join("\n")))
    }

    fn pipe(&self, input: BoxStream<'static, Cursor>, _ctx: OpCtx)
        -> BoxStream<'static, Cursor>
    {
        match &self.mode {
            RepoMode::Filter(glob) => {
                let glob = glob.clone();
                input.filter(move |c| {
                    let keep = glob_match(&glob, &c.repo);
                    async move { keep }
                }).boxed()
            }
            RepoMode::Bind(name) => {
                let name = name.clone();
                input.map(move |mut c| {
                    let v = c.repo.clone();
                    c.captures.insert(name.clone(), Capture { value: v, ref_id: None });
                    c
                }).boxed()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostics — owned here
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum RepoDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, got: Arc<str> },
    NoOp       { site: ParseSite },
}

impl Diagnostic for RepoDiag {
    fn code(&self) -> &str {
        match self {
            RepoDiag::MissingArg { .. } => "repo/missing-arg",
            RepoDiag::BadArg     { .. } => "repo/bad-arg",
            RepoDiag::NoOp       { .. } => "repo/no-op-glob",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            RepoDiag::MissingArg { site } => site,
            RepoDiag::BadArg     { site, .. } => site,
            RepoDiag::NoOp       { site } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            RepoDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "repo requires a glob pattern or capture (e.g. repo(myorg/*) or repo($R))");
                out.primary(site);
            }
            RepoDiag::BadArg { site, got } => {
                out.header(self.code(), self.severity(),
                    &format!("repo argument `{got}` is not a glob or capture"));
                out.primary(site);
            }
            RepoDiag::NoOp { site } => {
                out.header(self.code(), self.severity(),
                    "repo(*) matches every repo — it's a no-op. Omit it (seeding uses all configured repos), or bind with repo($R).");
                out.primary(site);
            }
        }
    }
}
