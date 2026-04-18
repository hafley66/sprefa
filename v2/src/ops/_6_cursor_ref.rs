//! cursor_ref — rebase a cursor onto a captured sub-content.
//!
//! Sigil: `&.$NAME`, `&.repo`, `&.rev`, `&.fs`
//!
//! This op is never typed directly by users; the parser desugars `&.<path>`
//! pipe steps into `OpInvocation { name: "cursor_ref", paren_src: path }`.
//!
//! Usage:
//!   json({ code: $C }) > &.$C > line(re: /TODO/)
//!   fs(**/pkg.json) > json({ name: $N }) > &.fs > line(...)

use std::sync::Arc;

use futures_core::stream::BoxStream;
use futures_util::stream::StreamExt;
use tracing::info_span;

use crate::_0_types::{Capture, Cursor, ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_5_op::{
    hover_render_grouped, BraceMode, CompletionItem, GrammarRef, LoweredOp, Op, OpCtx,
    OpInvocation, Operator, Pipeline, ProgramCtx,
};
use crate::path_expr::PathExpr;

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct CursorRefFactory;

impl Operator for CursorRefFactory {
    fn name(&self) -> &'static str { "cursor_ref" }
    fn paren_grammar(&self) -> GrammarRef { GrammarRef(Arc::from("cursor-ref-path")) }
    fn brace_mode(&self) -> BraceMode { BraceMode::DefaultFork }

    // Not user-addressable by name; only reachable via &. desugar.
    fn valid_at_head(&self, _upstream: &[Arc<dyn Op>]) -> bool { false }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label:  "&.$CAP".to_string(),
            detail: "&.$CAP".to_string(),
            doc:    "Rebase cursor onto a captured sub-content.".to_string(),
        }
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        let paren = inv.paren_src.as_ref().ok_or_else(|| {
            vec![Box::new(CrDiag::MissingPath {
                site: (*inv.parse_site).clone(),
            }) as Box<dyn Diagnostic>]
        })?;

        let path = PathExpr::parse(&paren.src).map_err(|msg| {
            vec![Box::new(CrDiag::BadPath {
                site: (*inv.parse_site).clone(),
                msg:  Arc::from(msg.as_str()),
            }) as Box<dyn Diagnostic>]
        })?;

        Ok(Pipeline::Op(LoweredOp::from(Arc::new(CursorRefOp {
            path,
            parse_site: inv.parse_site.clone(),
        }))))
    }
}

// ---------------------------------------------------------------------------
// Op
// ---------------------------------------------------------------------------

pub struct CursorRefOp {
    path:       PathExpr,
    parse_site: Arc<ParseSite>,
}

impl Op for CursorRefOp {
    fn name(&self) -> &'static str { "cursor_ref" }
    fn step(&self) -> u16 { 0 }
    fn parse_site(&self) -> &Arc<ParseSite> { &self.parse_site }

    fn witness(&self, c: &Cursor) -> Option<Arc<str>> {
        self.path.resolve(c).map(|v| Arc::from(v.as_ref()))
    }

    // cursor_ref does not bind a new capture — it redirects the cursor's
    // active content. Downstream ops see modified content/byte_range.
    fn binds_captures(&self) -> Vec<Arc<str>> { vec![] }

    fn hover_match(&self, _site: &ParseSite, cursors: &[Cursor]) -> Option<String> {
        let header = format!("`&{}`", self.path.render());
        let entries: Vec<(Option<String>, String, String)> = cursors.iter()
            .filter_map(|c| {
                self.path.resolve(c).map(|val| (
                    c.fs.as_ref().map(|fp| fp.0.to_string_lossy().into_owned()),
                    c.rev.to_string(),
                    val.into_owned(),
                ))
            })
            .collect();
        hover_render_grouped(&header, &entries)
    }

    fn pipe(
        &self,
        input: BoxStream<'static, Arc<[Cursor]>>,
        ctx:   OpCtx,
    ) -> BoxStream<'static, Arc<[Cursor]>> {
        let _pipe_span = info_span!("op.cursor_ref.pipe").entered();
        let path  = self.path.clone();
        let diags = ctx.diags.clone();
        let site  = self.parse_site.clone();

        input
            .map(move |batch| {
                let _batch_span = info_span!("op.cursor_ref.batch", n = batch.len()).entered();
                let path  = path.clone();
                let diags = diags.clone();
                let site  = site.clone();
                let mut out: Vec<Cursor> = Vec::with_capacity(batch.len());
                for cursor in batch.iter() {
                    match &path {
                        PathExpr::SelfCap(name) => {
                            match cursor.captures.get(name.as_ref()) {
                                Some(cap) => out.push(cursor.rebase(cap)),
                                None => {
                                    diags.0(Box::new(CrDiag::UnknownCapture {
                                        site: (*site).clone(),
                                        name: name.clone(),
                                    }));
                                    // cursor dropped
                                }
                            }
                        }
                        PathExpr::Field(_) => {
                            match path.resolve(cursor) {
                                Some(val) => {
                                    let cap = Capture::new(Arc::from(val.as_ref()));
                                    out.push(cursor.rebase(&cap));
                                }
                                None => {
                                    diags.0(Box::new(CrDiag::FieldUnset {
                                        site: (*site).clone(),
                                        path: format!("{path:?}").into(),
                                    }));
                                    // cursor dropped
                                }
                            }
                        }
                    }
                }
                Arc::<[Cursor]>::from(out.into_boxed_slice())
            })
            .boxed()
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum CrDiag {
    MissingPath    { site: ParseSite },
    BadPath        { site: ParseSite, msg: Arc<str> },
    UnknownCapture { site: ParseSite, name: Arc<str> },
    FieldUnset     { site: ParseSite, path: Arc<str> },
}

impl Diagnostic for CrDiag {
    fn code(&self) -> &str {
        match self {
            CrDiag::MissingPath    { .. } => "cursor_ref/missing-path",
            CrDiag::BadPath        { .. } => "cursor_ref/bad-path",
            CrDiag::UnknownCapture { .. } => "cursor_ref/unknown-capture",
            CrDiag::FieldUnset     { .. } => "cursor_ref/field-unset",
        }
    }
    fn severity(&self) -> Severity { Severity::Error }
    fn primary(&self) -> &ParseSite {
        match self {
            CrDiag::MissingPath    { site, .. } => site,
            CrDiag::BadPath        { site, .. } => site,
            CrDiag::UnknownCapture { site, .. } => site,
            CrDiag::FieldUnset     { site, .. } => site,
        }
    }
    fn render(&self, out: &mut dyn Renderer) {
        let msg = match self {
            CrDiag::MissingPath { .. } =>
                "cursor_ref requires a path argument (e.g. &.$CAP)".to_string(),
            CrDiag::BadPath { msg, .. } =>
                format!("invalid cursor_ref path: {msg}"),
            CrDiag::UnknownCapture { name, .. } =>
                format!("capture `${name}` not in scope at this cursor"),
            CrDiag::FieldUnset { path, .. } =>
                format!("cursor field `{path}` is unset"),
        };
        out.header(self.code(), self.severity(), &msg);
        out.primary(self.primary());
    }
}
