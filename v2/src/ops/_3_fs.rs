//! fs — file-path enumeration across (repo, rev). Not a filesystem listing.
//!
//! Paths come from `Reader::files(repo, rev, pattern)`. For the real git2-
//! backed Reader this means walking the rev's commit tree (blobs, not the
//! working directory). MemReader is a static stand-in with the same shape.
//!
//! Paren slot:
//!   - literal glob (e.g. `src/**/*.rs`) → Filter: pattern passed to Reader
//!   - capture     (e.g. `$F`)           → Bind: pattern = `**`, bind each
//!                                          matched path into `F`
//!
//! Fan-out: one input cursor × N matched files → N output cursors, each
//! with `cursor.fs = Some(fp)`. Empty match drops the input cursor (zero-
//! match diagnostic is a §12 future; not emitted here).

use std::sync::Arc;

use futures_core::stream::BoxStream;
use futures_util::stream::{self, StreamExt};

use crate::_0_types::{Capture, Cursor, FilePath, ParseSite};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation, Operator, Pipeline, ProgramCtx,
};
use crate::_8_parse::{classify_token, TokenClass};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct FsFactory;

impl Operator for FsFactory {
    fn name(&self) -> &'static str { "fs" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("fs-arg")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "fs".to_string(),
            detail: "fs(glob | $CAP)".to_string(),
            doc:    "# fs\n\nFan out cursors over files matching a glob, or bind each matched path to a capture.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(FsDiag::MissingArg { site: (*inv.parse_site).clone() })
                as Box<dyn Diagnostic>]
        })?;
        let arg = paren.src.trim();
        let mode = match classify_token(arg) {
            TokenClass::Capture(c)  => FsMode::Bind(c.name, Arc::from("**")),
            TokenClass::Literal     => FsMode::Filter(Arc::from(arg)),
            _ => return Err(vec![Box::new(FsDiag::BadArg {
                site: (*inv.parse_site).clone(),
                got:  Arc::from(arg),
            }) as _]),
        };
        Ok(Pipeline::Op(Arc::new(FsOp { mode, parse_site: inv.parse_site.clone() })))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct FsOp {
    mode:       FsMode,
    parse_site: Arc<ParseSite>,
}

enum FsMode {
    Filter(Arc<str>),             // glob pattern handed to Reader.files
    Bind(Arc<str>, Arc<str>),     // (capture name, pattern = "**")
}

impl FsMode {
    fn pattern(&self) -> Arc<str> {
        match self {
            FsMode::Filter(p) | FsMode::Bind(_, p) => p.clone(),
        }
    }
    fn bind_name(&self) -> Option<Arc<str>> {
        match self { FsMode::Bind(n, _) => Some(n.clone()), _ => None }
    }
}

fn fs_path_md(path_str: &str) -> String {
    // Paths are repo-relative; we don't know the git-root in op scope, so
    // render as inline code. VS Code's path detector still offers cmd+click
    // inside the workspace.
    format!("- `{}`", path_str)
}

impl Op for FsOp {
    fn name(&self) -> &'static str { "fs" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        c.fs.as_ref().map(|fp| Arc::from(fp.0.to_string_lossy().as_ref()))
    }

    fn capture_name(&self) -> Option<Arc<str>> { self.mode.bind_name() }

    fn hover_self(&self) -> String {
        let src = match &self.mode {
            FsMode::Filter(p)    => p.as_ref().to_string(),
            FsMode::Bind(n, _)   => format!("${}", n),
        };
        format!("# fs({})", src)
    }

    fn hover_capture(&self, cap: &str, cursors: &[Cursor]) -> Option<String> {
        let mut vals: Vec<String> = Vec::new();
        for c in cursors {
            if let Some(capture) = c.captures.get(cap) {
                let v = capture.value.as_ref().to_string();
                if !vals.contains(&v) {
                    vals.push(v);
                    if vals.len() >= 20 { break; }
                }
            }
        }
        if vals.is_empty() { return None; }
        let lines: Vec<String> = vals.iter().map(|v| fs_path_md(v)).collect();
        Some(format!("**`${cap}`** paths:\n\n{}", lines.join("\n")))
    }

    fn hover_match(&self, site: &crate::_0_types::ParseSite, cursors: &[Cursor]) -> Option<String> {
        let pattern = self.mode.pattern();
        let mut vals: Vec<String> = Vec::new();
        for c in cursors {
            let touched = c.evidence.iter().any(|ev|
                ev.op_name == "fs" && ev.parse_site.as_ref() == site
            );
            if !touched { continue; }
            if let Some(fp) = &c.fs {
                let v = fp.0.to_string_lossy().into_owned();
                if !vals.contains(&v) {
                    vals.push(v);
                    if vals.len() >= 20 { break; }
                }
            }
        }
        if vals.is_empty() {
            return Some(format!("**fs** glob: `{pattern}`\n\n(no matches yet)"));
        }
        let lines: Vec<String> = vals.iter().map(|v| fs_path_md(v)).collect();
        Some(format!("**fs** glob: `{pattern}`\n\n{} matches:\n\n{}", vals.len(), lines.join("\n")))
    }

    fn pipe(&self, input: BoxStream<'static, Cursor>, ctx: OpCtx)
        -> BoxStream<'static, Cursor>
    {
        let reader    = ctx.reader.clone();
        let pattern   = self.mode.pattern();
        let bind_name = self.mode.bind_name();

        input.then(move |c| {
            let reader    = reader.clone();
            let pattern   = pattern.clone();
            let bind_name = bind_name.clone();
            async move {
                let mut s = reader.files(&c.repo, &c.rev, &pattern);
                let files: Vec<FilePath> = s.next().await.unwrap_or_default();
                let items: Vec<Cursor> = files.into_iter().map(|fp| {
                    let mut c2 = c.clone();
                    let v: Arc<str> = Arc::from(fp.0.to_string_lossy().as_ref());
                    c2.fs = Some(fp);
                    if let Some(name) = bind_name.as_ref() {
                        c2.captures.insert(name.clone(), Capture { value: v, ref_id: None });
                    }
                    c2
                }).collect();
                stream::iter(items)
            }
        }).flatten().boxed()
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum FsDiag {
    MissingArg { site: ParseSite },
    BadArg     { site: ParseSite, got: Arc<str> },
}

impl Diagnostic for FsDiag {
    fn code(&self) -> &str {
        match self {
            FsDiag::MissingArg { .. } => "fs/missing-arg",
            FsDiag::BadArg     { .. } => "fs/bad-arg",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            FsDiag::MissingArg { site }        => site,
            FsDiag::BadArg     { site, .. }    => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            FsDiag::MissingArg { site } => {
                out.header(self.code(), self.severity(),
                    "fs requires a glob pattern or capture (e.g. fs(src/**/*.rs) or fs($F))");
                out.primary(site);
            }
            FsDiag::BadArg { site, got } => {
                out.header(self.code(), self.severity(),
                    &format!("fs argument `{got}` is not a glob or capture"));
                out.primary(site);
            }
        }
    }
}
