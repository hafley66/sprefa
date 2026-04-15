//! read — load file bytes into cursor.content.
//!
//! Takes no arguments. Expects cursor.fs to be set (from upstream `fs` op).
//! Cursors lacking a file path pass through with a diagnostic.
//!
//! Usage: fs(**) > read

use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream::StreamExt;

use crate::_0_types::{Cursor, ParseSite};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    BraceMode, CompletionItem, GrammarRef, Op, OpCtx, OpInvocation, Operator, Pipeline, ProgramCtx,
};

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct ReadFactory;

impl Operator for ReadFactory {
    fn name(&self) -> &'static str { "read" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("read-none")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::WalkerPattern }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "read".to_string(),
            detail: "read".to_string(),
            doc:    "# read\n\nLoad file bytes into `cursor.content`. Requires an upstream `fs` op.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        if inv.paren_src.is_some() || !inv.brackets.is_empty() {
            return Err(vec![Box::new(ReadDiag::UnexpectedArg {
                site: (*inv.parse_site).clone(),
            }) as _]);
        }
        Ok(Pipeline::Op(Arc::new(ReadOp { parse_site: inv.parse_site.clone() }).into()))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct ReadOp { parse_site: Arc<ParseSite> }

impl Op for ReadOp {
    fn name(&self) -> &'static str { "read" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        c.content.as_ref().map(|b| Arc::from(format!("{} bytes", b.len())))
    }

    fn hover_self(&self) -> String {
        "# read\n\nLoad file bytes into `cursor.content`. Requires an upstream `fs` op.".to_string()
    }

    fn pipe(&self, input: BoxStream<'static, Arc<[Cursor]>>, ctx: OpCtx)
        -> BoxStream<'static, Arc<[Cursor]>>
    {
        let reader    = ctx.reader.clone();
        let diags     = ctx.diags.clone();
        let parse_site = self.parse_site.clone();

        input.then(move |batch| {
            let reader     = reader.clone();
            let diags      = diags.clone();
            let parse_site = parse_site.clone();
            async move {
                let mut out: Vec<Cursor> = Vec::with_capacity(batch.len());
                for c in batch.iter() {
                    let mut c = c.clone();
                    match &c.fs {
                        None => {
                            diags.0(Box::new(ReadDiag::NoFile { site: (*parse_site).clone() }));
                            // Pass cursor through unchanged — callers may handle
                            // missing content defensively.
                        }
                        Some(fp) => {
                            let mut s = reader.bytes(&c.repo, &c.rev, fp);
                            let raw: Bytes = s.next().await.unwrap_or_default();
                            if !raw.is_empty() {
                                c.content = Some(Arc::new(raw));
                            }
                        }
                    }
                    out.push(c);
                }
                Arc::<[Cursor]>::from(out.into_boxed_slice())
            }
        }).boxed()
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ReadDiag {
    UnexpectedArg { site: crate::_0_types::ParseSite },
    NoFile        { site: crate::_0_types::ParseSite },
}

impl Diagnostic for ReadDiag {
    fn code(&self) -> &str {
        match self {
            ReadDiag::UnexpectedArg { .. } => "read/unexpected-arg",
            ReadDiag::NoFile        { .. } => "read/no-file",
        }
    }
    fn severity(&self) -> crate::_0_types::Severity { crate::_0_types::Severity::Error }
    fn primary(&self) -> &crate::_0_types::ParseSite {
        match self {
            ReadDiag::UnexpectedArg { site } => site,
            ReadDiag::NoFile        { site } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        match self {
            ReadDiag::UnexpectedArg { site } => {
                out.header(self.code(), self.severity(),
                    "read takes no arguments (e.g. fs(**) > read)");
                out.primary(site);
            }
            ReadDiag::NoFile { site } => {
                out.header(self.code(), self.severity(),
                    "read requires a file path; add fs(...) upstream");
                out.primary(site);
            }
        }
    }
}
