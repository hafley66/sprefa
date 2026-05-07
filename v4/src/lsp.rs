//! `lsp_*` op family — emit Diag events anchored to a cursor's LO/HI
//! span. Per user constraint, the whole family lives in one file
//! across layers (Component + four OperatorDefs).
//!
//! Surface:
//!
//! ```text
//!   lsp_error(:code)`message ${TERM}`
//!   lsp_warn(:code)`message ${TERM}`
//!   lsp_info(:code)`message ${TERM}`
//!   lsp_hint(:code)`message ${TERM}`
//! ```
//!
//! Args:
//!   `:code` — diagnostic code atom (positional, optional; defaults to
//!             `"sprf/diag"`). One short kebab/slash atom.
//!
//! DSL body — message template; `${X}` interps resolve from `cursor.terms`
//! at render time, mirroring `str`'s template path.
//!
//! Cursor flow — each op emits `Node::Emit(child)` after firing the diag,
//! so downstream still sees the row. To filter by severity downstream,
//! chain a separate filter op.
//!
//! Span — the diag's byte range comes from `cursor.get("LO")` /
//! `cursor.get("HI")` if both parse as `u32`. Otherwise the diag is
//! span-less.

use std::sync::Arc;

use effect_runtime::v2::{
    Component, Diag, Node, Pipe, RenderCtx, Severity,
};

use crate::Cursor;
use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{
    ArgKind, ArgSig, DslBinder, DslBody, DslShape, OperatorDef,
    default_plain_dsl_parse,
};
use crate::compile::lower::value::Value;

// ─── Component ─────────────────────────────────────────────────────────────

pub struct LspDiagComponent {
    severity: Severity,
    code:     Arc<str>,
    template: Arc<str>,
    interps:  Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
}

impl LspDiagComponent {
    pub fn new(
        severity: Severity,
        code:     Arc<str>,
        template: Arc<str>,
        interps:  Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self {
        Self { severity, code, template, interps: Arc::new(interps) }
    }

    fn render_message(&self, c: &Cursor) -> String {
        let raw = self.template.as_ref();
        let mut out = String::with_capacity(raw.len());
        let mut head: usize = 0;
        for interp in self.interps.iter() {
            let lo = interp.range.lo as usize;
            let hi = interp.range.hi as usize;
            if lo > raw.len() || hi > raw.len() || lo < head { continue; }
            out.push_str(&raw[head..lo]);
            match &interp.kind {
                crate::compile::lower::op_def::InterpKind::Term { field, .. } => {
                    // Layer 0c.3 — dot-access aware lookup; mirrors StrTemplateComponent.
                    let key: std::borrow::Cow<'_, str> = match field {
                        None    => (&*interp.name).into(),
                        Some(f) => format!("{}.{}", interp.name, f).into(),
                    };
                    if let Some(v) = c.get(&key) { out.push_str(v); }
                }
                crate::compile::lower::op_def::InterpKind::SubPipe { .. } => {
                    // Diag templates render messages, not pipe values; skip
                    // sub-pipe carveouts (today they're a parse-shape used
                    // by `str`, not a diag-message form).
                }
            }
            head = hi;
        }
        out.push_str(&raw[head..]);
        out
    }

    fn span_from_cursor(c: &Cursor) -> Option<(u32, u32)> {
        let lo = c.get("LO")?.parse::<u32>().ok()?;
        let hi = c.get("HI")?.parse::<u32>().ok()?;
        Some((lo, hi))
    }
}

impl Component for LspDiagComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let message = self.render_message(c);
        let mut diag = match self.severity {
            Severity::Error => Diag::error(self.code.clone(), message),
            Severity::Warn  => Diag::warn (self.code.clone(), message),
            Severity::Info  => Diag::info (self.code.clone(), message),
            Severity::Hint  => Diag::hint (self.code.clone(), message),
        };
        if let Some((lo, hi)) = Self::span_from_cursor(c) {
            diag = diag.with_span(lo, hi);
        }
        ctx.diag.emit(diag);
        Node::Emit(Arc::new(c.clone()))
    }
}

// ─── OperatorDefs ──────────────────────────────────────────────────────────

const LSP_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "code",
        doc: "diagnostic code atom (e.g. :unused-var). Default :sprf/diag.",
        required: false,
    },
];

fn lower_lsp_diag(
    severity: Severity,
    args:     &[Value],
    dsl:      Option<&DslBody>,
) -> Result<Pipe<Cursor>, LowerError> {
    let code: Arc<str> = args.first().and_then(|v| match v {
        Value::Atom(s) => Some(s.clone()),
        _ => None,
    }).unwrap_or_else(|| Arc::<str>::from("sprf/diag"));
    let body = dsl.ok_or_else(|| LowerError::Unknown(
        "lsp_*: dsl body required (e.g. lsp_error`unused: ${X}`)".into()
    ))?;
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);
    Ok(Pipe::new().step(Arc::new(LspDiagComponent::new(
        severity,
        code,
        body.raw.clone(),
        interps,
    ))))
}

fn lsp_binders_in_dsl(raw: &str) -> Vec<DslBinder> {
    // ${IDENT} interps don't BIND, they READ. Default empty so the
    // analyzer treats them as reads (use-before-bind catches the rest).
    let _ = raw;
    Vec::new()
}

fn lsp_parse_dsl(raw: &str) -> Vec<crate::compile::lower::op_def::DslInterp> {
    default_plain_dsl_parse(raw)
}

pub struct LspErrorDef;
impl OperatorDef for LspErrorDef {
    fn name(&self) -> &'static str { "lsp_error" }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Error, args, dsl)
    }
}

pub struct LspWarnDef;
impl OperatorDef for LspWarnDef {
    fn name(&self) -> &'static str { "lsp_warn" }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Warn, args, dsl)
    }
}

pub struct LspInfoDef;
impl OperatorDef for LspInfoDef {
    fn name(&self) -> &'static str { "lsp_info" }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Info, args, dsl)
    }
}

pub struct LspHintDef;
impl OperatorDef for LspHintDef {
    fn name(&self) -> &'static str { "lsp_hint" }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Hint, args, dsl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        expand, DiagSink, ExpandOpts, MemQueue, PipeInstance, QueueBackend,
    };

    struct CollectSink { rows: Mutex<Vec<Diag>> }
    impl DiagSink for CollectSink {
        fn emit(&self, d: Diag) { self.rows.lock().unwrap().push(d); }
    }

    fn cur_with_span(lo: u32, hi: u32, terms: &[(&str, &str)]) -> Cursor {
        let mut c = Cursor::default();
        c.set("LO", lo.to_string());
        c.set("HI", hi.to_string());
        for (k, v) in terms { c.set(*k, *v); }
        c
    }

    #[test]
    fn lsp_error_emits_diag_with_span_and_template() {
        let sink = Arc::new(CollectSink { rows: Mutex::new(Vec::new()) });
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let comp = LspDiagComponent::new(
            Severity::Error,
            Arc::from("unused-var"),
            Arc::from("variable ${NAME} is unused"),
            default_plain_dsl_parse("variable ${NAME} is unused"),
        );
        let pipe = PipeInstance::new(vec![Arc::new(comp) as Arc<dyn Component<Next = Cursor>>]);
        let opts = ExpandOpts::default().with_diag(sink.clone());
        let seed = vec![Arc::new(cur_with_span(10, 20, &[("NAME", "foo")]))];
        expand(&pipe, queue, seed, opts);

        let rows = sink.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        let d = &rows[0];
        assert_eq!(&*d.code, "unused-var");
        assert_eq!(d.message, "variable foo is unused");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span.unwrap().lo, 10);
        assert_eq!(d.span.unwrap().hi, 20);
    }

    #[test]
    fn lsp_warn_default_code_and_no_span() {
        let sink = Arc::new(CollectSink { rows: Mutex::new(Vec::new()) });
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let comp = LspDiagComponent::new(
            Severity::Warn,
            Arc::from("sprf/diag"),
            Arc::from("plain"),
            Vec::new(),
        );
        let pipe = PipeInstance::new(vec![Arc::new(comp) as Arc<dyn Component<Next = Cursor>>]);
        expand(&pipe, queue, vec![Arc::new(Cursor::default())],
               ExpandOpts::default().with_diag(sink.clone()));
        let rows = sink.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].severity, Severity::Warn);
        assert!(rows[0].span.is_none());
    }
}
