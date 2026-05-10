//! `lsp_*` op family — emit Diag events anchored to a cursor coord.
//! Per user constraint, the whole family lives in one file
//! across layers (Component + four OperatorDefs).
//!
//! Surface:
//!
//! ```text
//!   lsp_error(:code)`message ${TERM}`
//!   lsp_warn(:code)`message ${TERM}`
//!   lsp_info(:code)`message ${TERM}`
//!   lsp_hint(:code)`message ${TERM}`
//!   lsp.hover[TERM]`hover ${TERM}`
//!   lsp_hover[TERM]`hover ${TERM}`
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
//! Span — the diag's byte range comes from `cursor.at` when it resolves
//! through SprfStore. `lsp_warn[NAME]` focuses the span on term `NAME`'s
//! coord. Legacy `LO`/`HI` and `NAME_LO`/`NAME_HI` remain fallbacks.

use std::sync::Arc;

use effect_runtime::v2::{
    Component, Diag, Node, Pipe, RenderCtx, Severity,
};

use crate::{Cursor, Ref, StringId};
use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{
    ArgKind, ArgSig, DslBinder, DslBody, DslShape, OperatorDef,
    default_plain_dsl_parse,
};
use crate::compile::lower::value::Value;
use crate::sprf_introspect::PipeIntrospect;
use crate::store::SprfStore;

pub const LSP_HOVER_CODE: &str = "sprf/hover";

// ─── Component ─────────────────────────────────────────────────────────────

pub struct LspDiagComponent {
    severity: Severity,
    code:     Arc<str>,
    template: Arc<str>,
    interps:  Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    focus:    LspDiagFocus,
    store:    Option<Arc<SprfStore>>,
}

pub struct LspHoverComponent {
    template: Arc<str>,
    interps:  Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    focus:    LspDiagFocus,
    store:    Option<Arc<SprfStore>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LspDiagFocus {
    Focal,
    Term(Arc<str>),
}

impl LspDiagComponent {
    pub fn new(
        severity: Severity,
        code:     Arc<str>,
        template: Arc<str>,
        interps:  Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self {
        Self {
            severity,
            code,
            template,
            interps: Arc::new(interps),
            focus: LspDiagFocus::Focal,
            store: None,
        }
    }

    fn with_focus(mut self, focus: LspDiagFocus) -> Self {
        self.focus = focus;
        self
    }

    fn with_sprf_store(mut self, store: Arc<SprfStore>) -> Self {
        self.store = Some(store);
        self
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

    fn span_from_cursor(&self, c: &Cursor) -> Option<(u32, u32)> {
        match &self.focus {
            LspDiagFocus::Focal => {
                self.span_from_ref(c.at)
                    .or_else(|| Self::span_from_legacy(c, "LO", "HI"))
            }
            LspDiagFocus::Term(name) => {
                let name_id = StringId::of(name.as_ref());
                c.term(name_id)
                    .and_then(|t| self.span_from_ref(t.at))
                    .or_else(|| {
                        Self::span_from_legacy(
                            c,
                            &format!("{}_LO", name.as_ref()),
                            &format!("{}_HI", name.as_ref()),
                        )
                    })
            }
        }
    }

    fn span_from_ref(&self, at: Ref) -> Option<(u32, u32)> {
        if at == Ref::SYNTHETIC {
            return None;
        }
        let coord = self.store.as_ref()?.coord_of(at)?;
        Some((coord.lo, coord.hi))
    }

    fn span_from_legacy(c: &Cursor, lo_key: &str, hi_key: &str) -> Option<(u32, u32)> {
        let lo = c.get(lo_key)?.parse::<u32>().ok()?;
        let hi = c.get(hi_key)?.parse::<u32>().ok()?;
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
        if let Some((lo, hi)) = self.span_from_cursor(c) {
            diag = diag.with_span(lo, hi);
        }
        ctx.diag.emit(diag);
        Node::Emit(Arc::new(c.clone()))
    }
}

impl LspHoverComponent {
    pub fn new(
        template: Arc<str>,
        interps:  Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self {
        Self {
            template,
            interps: Arc::new(interps),
            focus: LspDiagFocus::Focal,
            store: None,
        }
    }

    fn with_focus(mut self, focus: LspDiagFocus) -> Self {
        self.focus = focus;
        self
    }

    fn with_sprf_store(mut self, store: Arc<SprfStore>) -> Self {
        self.store = Some(store);
        self
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
                    let key: std::borrow::Cow<'_, str> = match field {
                        None    => (&*interp.name).into(),
                        Some(f) => format!("{}.{}", interp.name, f).into(),
                    };
                    if let Some(v) = c.get(&key) { out.push_str(v); }
                }
                crate::compile::lower::op_def::InterpKind::SubPipe { .. } => {}
            }
            head = hi;
        }
        out.push_str(&raw[head..]);
        out
    }

    fn span_from_cursor(&self, c: &Cursor) -> Option<(u32, u32)> {
        match &self.focus {
            LspDiagFocus::Focal => {
                self.span_from_ref(c.at)
                    .or_else(|| LspDiagComponent::span_from_legacy(c, "LO", "HI"))
            }
            LspDiagFocus::Term(name) => {
                let name_id = StringId::of(name.as_ref());
                c.term(name_id)
                    .and_then(|t| self.span_from_ref(t.at))
                    .or_else(|| {
                        LspDiagComponent::span_from_legacy(
                            c,
                            &format!("{}_LO", name.as_ref()),
                            &format!("{}_HI", name.as_ref()),
                        )
                    })
            }
        }
    }

    fn span_from_ref(&self, at: Ref) -> Option<(u32, u32)> {
        if at == Ref::SYNTHETIC {
            return None;
        }
        let coord = self.store.as_ref()?.coord_of(at)?;
        Some((coord.lo, coord.hi))
    }
}

impl Component for LspHoverComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        if let Some((lo, hi)) = self.span_from_cursor(c) {
            ctx.diag.emit(
                Diag::hint(LSP_HOVER_CODE, self.render_message(c)).with_span(lo, hi),
            );
        }
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

const LSP_FLOW: ArgSig = ArgSig {
    kind: ArgKind::Pipe,
    name: "focus",
    doc: "optional term focus for diagnostic span, e.g. lsp_warn[NAME](...)",
    required: false,
};

fn lower_lsp_diag(
    severity: Severity,
    ctx:      &LowerCtx,
    flow:     Option<Value>,
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
    let mut comp = LspDiagComponent::new(
        severity,
        code,
        body.raw.clone(),
        interps,
    ).with_focus(lsp_focus_from_flow(flow)?);
    if let Some(store) = &ctx.sprf_store {
        comp = comp.with_sprf_store(store.clone());
    }
    Ok(Pipe::new().step(Arc::new(comp)))
}

fn lower_lsp_hover(
    ctx:  &LowerCtx,
    flow: Option<Value>,
    dsl:  Option<&DslBody>,
) -> Result<Pipe<Cursor>, LowerError> {
    let body = dsl.ok_or_else(|| LowerError::Unknown(
        "lsp.hover: dsl body required (e.g. lsp.hover`details ${X}`)".into()
    ))?;
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);
    let mut comp = LspHoverComponent::new(
        body.raw.clone(),
        interps,
    ).with_focus(lsp_focus_from_flow(flow)?);
    if let Some(store) = &ctx.sprf_store {
        comp = comp.with_sprf_store(store.clone());
    }
    Ok(Pipe::new().step(Arc::new(comp)))
}

fn lsp_focus_from_flow(flow: Option<Value>) -> Result<LspDiagFocus, LowerError> {
    let Some(flow) = flow else {
        return Ok(LspDiagFocus::Focal);
    };
    let Value::Pipe(pipe) = flow else {
        return Err(LowerError::Unknown(
            "lsp_* [] focus must be a term read, e.g. lsp_warn[NAME]".into(),
        ));
    };
    let reads = pipe.reads_terms();
    if reads.len() == 1 && pipe.binds_terms().is_empty() {
        return Ok(LspDiagFocus::Term(reads[0].clone()));
    }
    Err(LowerError::Unknown(
        "lsp_* [] focus must read exactly one term, e.g. lsp_warn[NAME]".into(),
    ))
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
    fn flow_arg(&self) -> Option<ArgSig> { Some(LSP_FLOW) }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx:    &LowerCtx,
        flow:   Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Error, ctx, flow, args, dsl)
    }
}

pub struct LspWarnDef;
impl OperatorDef for LspWarnDef {
    fn name(&self) -> &'static str { "lsp_warn" }
    fn flow_arg(&self) -> Option<ArgSig> { Some(LSP_FLOW) }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx:    &LowerCtx,
        flow:   Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Warn, ctx, flow, args, dsl)
    }
}

pub struct LspInfoDef;
impl OperatorDef for LspInfoDef {
    fn name(&self) -> &'static str { "lsp_info" }
    fn flow_arg(&self) -> Option<ArgSig> { Some(LSP_FLOW) }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx:    &LowerCtx,
        flow:   Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Info, ctx, flow, args, dsl)
    }
}

pub struct LspHintDef;
impl OperatorDef for LspHintDef {
    fn name(&self) -> &'static str { "lsp_hint" }
    fn flow_arg(&self) -> Option<ArgSig> { Some(LSP_FLOW) }
    fn paren_args(&self) -> &[ArgSig] { LSP_SPEC }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx:    &LowerCtx,
        flow:   Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Hint, ctx, flow, args, dsl)
    }
}

pub struct LspHoverDef;
impl OperatorDef for LspHoverDef {
    fn name(&self) -> &'static str { "lsp.hover" }
    fn flow_arg(&self) -> Option<ArgSig> { Some(LSP_FLOW) }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx:    &LowerCtx,
        flow:   Option<Value>,
        _args:  &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_hover(ctx, flow, dsl)
    }
}

pub struct LspHoverAliasDef;
impl OperatorDef for LspHoverAliasDef {
    fn name(&self) -> &'static str { "lsp_hover" }
    fn flow_arg(&self) -> Option<ArgSig> { Some(LSP_FLOW) }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> { lsp_binders_in_dsl(raw) }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx:    &LowerCtx,
        flow:   Option<Value>,
        _args:  &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_hover(ctx, flow, dsl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        expand, DiagSink, ExpandOpts, FactStore, MemFactStore, MemQueue, PipeInstance,
        QueueBackend,
    };
    use crate::Coord;
    use crate::store::SprfStore;

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
    fn lsp_warn_uses_cursor_at_before_legacy_span() {
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let store = SprfStore::new(facts);
        let file_id = store.intern_file(b"abcdef", "demo.rs");
        let at = store.intern_ref(Coord { repo: 0, rev: 0, fs: file_id, lo: 2, hi: 5 });

        let sink = Arc::new(CollectSink { rows: Mutex::new(Vec::new()) });
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let comp = LspDiagComponent::new(
            Severity::Warn,
            Arc::from("cursor-at"),
            Arc::from("at"),
            Vec::new(),
        ).with_sprf_store(store);
        let pipe = PipeInstance::new(vec![Arc::new(comp) as Arc<dyn Component<Next = Cursor>>]);
        let mut cursor = Cursor::default();
        cursor.at = at;
        cursor.set("LO", "0");
        cursor.set("HI", "6");

        expand(&pipe, queue, vec![Arc::new(cursor)], ExpandOpts::default().with_diag(sink.clone()));

        let rows = sink.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].span.unwrap().lo, 2);
        assert_eq!(rows[0].span.unwrap().hi, 5);
    }

    #[test]
    fn lsp_warn_term_focus_uses_term_at_before_focal_span() {
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let store = SprfStore::new(facts);
        let file_id = store.intern_file(b"before sh! after", "demo.rs");
        let focal = store.intern_ref(Coord { repo: 0, rev: 0, fs: file_id, lo: 0, hi: 16 });
        let term_coord = Coord { repo: 0, rev: 0, fs: file_id, lo: 7, hi: 10 };

        let sink = Arc::new(CollectSink { rows: Mutex::new(Vec::new()) });
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let comp = LspDiagComponent::new(
            Severity::Warn,
            Arc::from("term-at"),
            Arc::from("shell ${NAME}"),
            default_plain_dsl_parse("shell ${NAME}"),
        )
        .with_focus(LspDiagFocus::Term(Arc::from("NAME")))
        .with_sprf_store(store.clone());
        let pipe = PipeInstance::new(vec![Arc::new(comp) as Arc<dyn Component<Next = Cursor>>]);
        let mut cursor = Cursor::default();
        cursor.at = focal;
        cursor.set("NAME", "sh!");
        cursor.set_at("NAME", "sh!", term_coord, &store);

        expand(&pipe, queue, vec![Arc::new(cursor)], ExpandOpts::default().with_diag(sink.clone()));

        let rows = sink.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message, "shell sh!");
        assert_eq!(rows[0].span.unwrap().lo, 7);
        assert_eq!(rows[0].span.unwrap().hi, 10);
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
