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
//!   expect_zero(:code)`message ${TERM}`
//!   expect_match(:code)`message`
//! ```
//!
//! Args:
//!   `:code` — diagnostic code atom (positional, optional; defaults to
//!             `"sprf/diag"`). One short kebab/slash atom.
//!
//! DSL body — message template; `${X}` interps resolve from `cursor.terms`
//! at render time, mirroring `str`'s template path.
//!
//! Cursor flow — every op in this file is rows-in → rows-out.
//!   lsp_*           — pass-through, one diag per row.
//!   expect_zero     — drops every input row, one Error diag per row.
//!                     Empty input = no diags, no rows. Composes anywhere
//!                     in the pipe (zero rows out is jq-`empty`).
//!   expect_match    — pass-through; tracks "saw any row" across the rule
//!                     run and emits one Warn diag at run-end if not.
//!                     Barrier `complete()` is the end-of-run hook.
//!
//! Span — the diag's byte range comes from `cursor.at` when it resolves
//! through SprfStore. `lsp_warn[NAME]` focuses the span on term `NAME`'s
//! coord. Legacy `LO`/`HI` and `NAME_LO`/`NAME_HI` remain fallbacks.
//! `expect_match` anchors its end-of-run warn to the op call site (no
//! per-row span is available at that point).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    splice_into, BarrierScope, ByteRange, Component, ComponentLifecycle, Diag, Node, PendingSummary,
    Pipe, QueueBackend, QueueRow, RenderCtx, Severity,
};

use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{
    default_plain_dsl_parse, ArgKind, ArgSig, DslBinder, DslBody, DslShape, OperatorDef,
};
use crate::compile::lower::value::{Value, ValueKind};
use crate::sprf_introspect::PipeIntrospect;
use crate::store::SprfStore;
use crate::{Cursor, Ref, StringId};

pub const LSP_HOVER_CODE: &str = "sprf/hover";

// ─── Component ─────────────────────────────────────────────────────────────

pub struct LspDiagComponent {
    severity: Severity,
    code: Arc<str>,
    template: Arc<str>,
    interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    focus: LspDiagFocus,
    store: Option<Arc<SprfStore>>,
}

pub struct LspHoverComponent {
    template: Arc<str>,
    interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    focus: LspDiagFocus,
    store: Option<Arc<SprfStore>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LspDiagFocus {
    Focal,
    Term(Arc<str>),
}

impl LspDiagComponent {
    pub fn new(
        severity: Severity,
        code: Arc<str>,
        template: Arc<str>,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
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
            if lo > raw.len() || hi > raw.len() || lo < head {
                continue;
            }
            out.push_str(&raw[head..lo]);
            match &interp.kind {
                crate::compile::lower::op_def::InterpKind::Term { field, .. } => {
                    // Layer 0c.3 — dot-access aware lookup; mirrors StrTemplateComponent.
                    let key: std::borrow::Cow<'_, str> = match field {
                        None => (&*interp.name).into(),
                        Some(f) => format!("{}.{}", interp.name, f).into(),
                    };
                    if let Some(v) = c.get(&key) {
                        out.push_str(v);
                    }
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
            LspDiagFocus::Focal => self
                .span_from_ref(c.at)
                .or_else(|| Self::span_from_legacy(c, "LO", "HI")),
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
            Severity::Warn => Diag::warn(self.code.clone(), message),
            Severity::Info => Diag::info(self.code.clone(), message),
            Severity::Hint => Diag::hint(self.code.clone(), message),
        };
        if let Some((lo, hi)) = self.span_from_cursor(c) {
            diag = diag.with_span(lo, hi);
        }
        ctx.diag.emit(diag);
        Node::Emit(Arc::new(c.clone()))
    }
}

impl LspHoverComponent {
    pub fn new(template: Arc<str>, interps: Vec<crate::compile::lower::op_def::DslInterp>) -> Self {
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
            if lo > raw.len() || hi > raw.len() || lo < head {
                continue;
            }
            out.push_str(&raw[head..lo]);
            match &interp.kind {
                crate::compile::lower::op_def::InterpKind::Term { field, .. } => {
                    let key: std::borrow::Cow<'_, str> = match field {
                        None => (&*interp.name).into(),
                        Some(f) => format!("{}.{}", interp.name, f).into(),
                    };
                    if let Some(v) = c.get(&key) {
                        out.push_str(v);
                    }
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
            LspDiagFocus::Focal => self
                .span_from_ref(c.at)
                .or_else(|| LspDiagComponent::span_from_legacy(c, "LO", "HI")),
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
            ctx.diag
                .emit(Diag::hint(LSP_HOVER_CODE, self.render_message(c)).with_span(lo, hi));
        }
        Node::Emit(Arc::new(c.clone()))
    }
}

// ─── OperatorDefs ──────────────────────────────────────────────────────────

const LSP_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "code",
    doc: "diagnostic code atom (e.g. :unused-var). Default :sprf/diag.",
    required: false,
}];

const LSP_FLOW: ArgSig = ArgSig {
    kind: ArgKind::Pipe,
    name: "focus",
    doc: "optional term focus for diagnostic span, e.g. lsp_warn[NAME](...)",
    required: false,
};

fn lower_lsp_diag(
    severity: Severity,
    ctx: &LowerCtx,
    flow: Option<Value>,
    args: &[Value],
    dsl: Option<&DslBody>,
) -> Result<Pipe<Cursor>, LowerError> {
    let code: Arc<str> = args
        .first()
        .and_then(|v| match v.kind() {
            ValueKind::Atom(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| Arc::<str>::from("sprf/diag"));
    let body = dsl.ok_or_else(|| {
        LowerError::Unknown("lsp_*: dsl body required (e.g. lsp_error`unused: ${X}`)".into())
    })?;
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);
    let mut comp = LspDiagComponent::new(severity, code, body.raw.clone(), interps)
        .with_focus(lsp_focus_from_flow(flow)?);
    if let Some(store) = &ctx.sprf_store {
        comp = comp.with_sprf_store(store.clone());
    }
    Ok(Pipe::new().step(Arc::new(comp)))
}

fn lower_lsp_hover(
    ctx: &LowerCtx,
    flow: Option<Value>,
    dsl: Option<&DslBody>,
) -> Result<Pipe<Cursor>, LowerError> {
    let body = dsl.ok_or_else(|| {
        LowerError::Unknown("lsp.hover: dsl body required (e.g. lsp.hover`details ${X}`)".into())
    })?;
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);
    let mut comp =
        LspHoverComponent::new(body.raw.clone(), interps).with_focus(lsp_focus_from_flow(flow)?);
    if let Some(store) = &ctx.sprf_store {
        comp = comp.with_sprf_store(store.clone());
    }
    Ok(Pipe::new().step(Arc::new(comp)))
}

fn lsp_focus_from_flow(flow: Option<Value>) -> Result<LspDiagFocus, LowerError> {
    let Some(flow) = flow else {
        return Ok(LspDiagFocus::Focal);
    };
    let ValueKind::Pipe(pipe) = flow.kind() else {
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
    fn name(&self) -> &'static str {
        "lsp_error"
    }
    fn flow_arg(&self) -> Option<ArgSig> {
        Some(LSP_FLOW)
    }
    fn paren_args(&self) -> &[ArgSig] {
        LSP_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Error, ctx, flow, args, dsl)
    }
}

pub struct LspWarnDef;
impl OperatorDef for LspWarnDef {
    fn name(&self) -> &'static str {
        "lsp_warn"
    }
    fn flow_arg(&self) -> Option<ArgSig> {
        Some(LSP_FLOW)
    }
    fn paren_args(&self) -> &[ArgSig] {
        LSP_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Warn, ctx, flow, args, dsl)
    }
}

pub struct LspInfoDef;
impl OperatorDef for LspInfoDef {
    fn name(&self) -> &'static str {
        "lsp_info"
    }
    fn flow_arg(&self) -> Option<ArgSig> {
        Some(LSP_FLOW)
    }
    fn paren_args(&self) -> &[ArgSig] {
        LSP_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Info, ctx, flow, args, dsl)
    }
}

pub struct LspHintDef;
impl OperatorDef for LspHintDef {
    fn name(&self) -> &'static str {
        "lsp_hint"
    }
    fn flow_arg(&self) -> Option<ArgSig> {
        Some(LSP_FLOW)
    }
    fn paren_args(&self) -> &[ArgSig] {
        LSP_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_diag(Severity::Hint, ctx, flow, args, dsl)
    }
}

pub struct LspHoverDef;
impl OperatorDef for LspHoverDef {
    fn name(&self) -> &'static str {
        "lsp.hover"
    }
    fn flow_arg(&self) -> Option<ArgSig> {
        Some(LSP_FLOW)
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_hover(ctx, flow, dsl)
    }
}

// ─── expect_zero / expect_match ───────────────────────────────────────────
//
// Pipey contract:
//   `expect_zero(:c)`m``  — rows-in → 0 rows-out. 1 Error per input row.
//   `expect_match(:c)`m`` — rows-in → same rows-out. 1 Warn at run-end iff
//                           zero rows ever arrived (Barrier `complete()`).
// Neither op is positional. Both compose anywhere in the pipe.

pub struct ExpectZeroComponent {
    code: Arc<str>,
    template: Arc<str>,
    interps: Arc<Vec<crate::compile::lower::op_def::DslInterp>>,
    store: Option<Arc<SprfStore>>,
}

impl ExpectZeroComponent {
    pub fn new(
        code: Arc<str>,
        template: Arc<str>,
        interps: Vec<crate::compile::lower::op_def::DslInterp>,
    ) -> Self {
        Self {
            code,
            template,
            interps: Arc::new(interps),
            store: None,
        }
    }

    fn with_sprf_store(mut self, store: Arc<SprfStore>) -> Self {
        self.store = Some(store);
        self
    }

    fn render_message(&self, c: &Cursor) -> String {
        // Term-only template interp; mirrors LspDiagComponent. SubPipe
        // carveouts are skipped — diag templates render messages, not
        // pipe values.
        let raw = self.template.as_ref();
        let mut out = String::with_capacity(raw.len());
        let mut head: usize = 0;
        for interp in self.interps.iter() {
            let lo = interp.range.lo as usize;
            let hi = interp.range.hi as usize;
            if lo > raw.len() || hi > raw.len() || lo < head {
                continue;
            }
            out.push_str(&raw[head..lo]);
            if let crate::compile::lower::op_def::InterpKind::Term { field, .. } = &interp.kind {
                let key: std::borrow::Cow<'_, str> = match field {
                    None => (&*interp.name).into(),
                    Some(f) => format!("{}.{}", interp.name, f).into(),
                };
                if let Some(v) = c.get(&key) {
                    out.push_str(v);
                }
            }
            head = hi;
        }
        out.push_str(&raw[head..]);
        out
    }

    fn span_from_cursor(&self, c: &Cursor) -> Option<(u32, u32)> {
        if c.at != Ref::SYNTHETIC {
            if let Some(store) = &self.store {
                if let Some(coord) = store.coord_of(c.at) {
                    return Some((coord.lo, coord.hi));
                }
            }
        }
        LspDiagComponent::span_from_legacy(c, "LO", "HI")
    }
}

impl Component for ExpectZeroComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let message = self.render_message(c);
        let mut diag = Diag::error(self.code.clone(), message);
        if let Some((lo, hi)) = self.span_from_cursor(c) {
            diag = diag.with_span(lo, hi);
        }
        ctx.diag.emit(diag);
        Node::Done
    }
}

pub struct ExpectZeroDef;
impl OperatorDef for ExpectZeroDef {
    fn name(&self) -> &'static str {
        "expect_zero"
    }
    fn paren_args(&self) -> &[ArgSig] {
        LSP_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let code: Arc<str> = args
            .first()
            .and_then(|v| match v.kind() {
                ValueKind::Atom(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| Arc::<str>::from("expect_zero"));
        let (template, interps): (Arc<str>, Vec<_>) = match dsl {
            Some(body) => {
                let mut interps = body.interps.clone();
                interps.sort_by_key(|i| i.range.lo);
                (body.raw.clone(), interps)
            }
            None => (Arc::<str>::from(""), Vec::new()),
        };
        let mut comp = ExpectZeroComponent::new(code, template, interps);
        if let Some(store) = &ctx.sprf_store {
            comp = comp.with_sprf_store(store.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

/// `expect_match` — pass-through that tracks "saw any row" per barrier
/// scope. `complete()` (called by the runtime when upstream is drained
/// at this depth) emits one Warn diag if no row was seen.
pub struct ExpectMatchComponent {
    code: Arc<str>,
    template: Arc<str>,
    call_span: Option<ByteRange>,
    saw_any: Mutex<HashMap<BarrierScope, bool>>,
}

impl ExpectMatchComponent {
    pub fn new(code: Arc<str>, template: Arc<str>, call_span: Option<ByteRange>) -> Self {
        Self {
            code,
            template,
            call_span,
            saw_any: Mutex::new(HashMap::new()),
        }
    }

    fn scope_of(&self, ctx: &RenderCtx, row: &QueueRow<Cursor>) -> BarrierScope {
        BarrierScope {
            pipe_hash: row.pipe_hash,
            instance_id: row.instance_id,
            expand_tick: ctx.expand_tick,
            depth: ctx.depth,
        }
    }
}

impl Component for ExpectMatchComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx: &RenderCtx,
        rows: &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        // Pass each row through; mark scope as "saw a row". Multiple
        // distinct scopes can run through one component (re-runs across
        // ticks), so key on the row's scope, not a single bool.
        if let Some(first) = rows.first() {
            let scope = self.scope_of(ctx, first);
            self.saw_any.lock().unwrap().insert(scope, true);
        }
        for row in rows {
            let node = Node::Emit(Arc::new(row.value.as_ref().clone()));
            splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
        }
    }

    fn lifecycle(&self) -> ComponentLifecycle {
        ComponentLifecycle::Barrier
    }

    fn idle(
        &self,
        _ctx: &RenderCtx,
        _scope: BarrierScope,
        _pending: PendingSummary,
        _queue: &dyn QueueBackend<Cursor>,
    ) {
    }

    fn complete(&self, ctx: &RenderCtx, scope: BarrierScope, _queue: &dyn QueueBackend<Cursor>) {
        let saw = self.saw_any.lock().unwrap().remove(&scope).unwrap_or(false);
        if !saw {
            let mut diag = Diag::warn(self.code.clone(), self.template.as_ref().to_string());
            if let Some(span) = self.call_span {
                diag = diag.with_span(span.lo, span.hi);
            }
            ctx.diag.emit(diag);
        }
    }
}

pub struct ExpectMatchDef;
impl OperatorDef for ExpectMatchDef {
    fn name(&self) -> &'static str {
        "expect_match"
    }
    fn paren_args(&self) -> &[ArgSig] {
        LSP_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let code: Arc<str> = args
            .first()
            .and_then(|v| match v.kind() {
                ValueKind::Atom(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| Arc::<str>::from("expect_match"));
        let template: Arc<str> = dsl
            .map(|b| b.raw.clone())
            .unwrap_or_else(|| Arc::<str>::from("expected at least one row, got none"));
        let comp = ExpectMatchComponent::new(code, template, ctx.current_call_span.get());
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

pub struct LspHoverAliasDef;
impl OperatorDef for LspHoverAliasDef {
    fn name(&self) -> &'static str {
        "lsp_hover"
    }
    fn flow_arg(&self) -> Option<ArgSig> {
        Some(LSP_FLOW)
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        lsp_binders_in_dsl(raw)
    }
    fn parse_dsl(
        &self,
        raw: &str,
    ) -> Result<Vec<crate::compile::lower::op_def::DslInterp>, LowerError> {
        Ok(lsp_parse_dsl(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_lsp_hover(ctx, flow, dsl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SprfStore;
    use crate::Coord;
    use effect_runtime::v2::{
        expand, DiagSink, ExpandOpts, FactStore, MemFactStore, MemQueue, PipeInstance, QueueBackend,
    };
    use std::sync::Mutex;

    struct CollectSink {
        rows: Mutex<Vec<Diag>>,
    }
    impl DiagSink for CollectSink {
        fn emit(&self, d: Diag) {
            self.rows.lock().unwrap().push(d);
        }
    }

    fn cur_with_span(lo: u32, hi: u32, terms: &[(&str, &str)]) -> Cursor {
        let mut c = Cursor::default();
        c.set("LO", lo.to_string());
        c.set("HI", hi.to_string());
        for (k, v) in terms {
            c.set(*k, *v);
        }
        c
    }

    #[test]
    fn lsp_error_emits_diag_with_span_and_template() {
        let sink = Arc::new(CollectSink {
            rows: Mutex::new(Vec::new()),
        });
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
        let at = store.intern_ref(Coord {
            repo: 0,
            rev: 0,
            fs: file_id,
            lo: 2,
            hi: 5,
        });

        let sink = Arc::new(CollectSink {
            rows: Mutex::new(Vec::new()),
        });
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let comp = LspDiagComponent::new(
            Severity::Warn,
            Arc::from("cursor-at"),
            Arc::from("at"),
            Vec::new(),
        )
        .with_sprf_store(store);
        let pipe = PipeInstance::new(vec![Arc::new(comp) as Arc<dyn Component<Next = Cursor>>]);
        let mut cursor = Cursor::default();
        cursor.at = at;
        cursor.set("LO", "0");
        cursor.set("HI", "6");

        expand(
            &pipe,
            queue,
            vec![Arc::new(cursor)],
            ExpandOpts::default().with_diag(sink.clone()),
        );

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
        let focal = store.intern_ref(Coord {
            repo: 0,
            rev: 0,
            fs: file_id,
            lo: 0,
            hi: 16,
        });
        let term_coord = Coord {
            repo: 0,
            rev: 0,
            fs: file_id,
            lo: 7,
            hi: 10,
        };

        let sink = Arc::new(CollectSink {
            rows: Mutex::new(Vec::new()),
        });
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

        expand(
            &pipe,
            queue,
            vec![Arc::new(cursor)],
            ExpandOpts::default().with_diag(sink.clone()),
        );

        let rows = sink.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message, "shell sh!");
        assert_eq!(rows[0].span.unwrap().lo, 7);
        assert_eq!(rows[0].span.unwrap().hi, 10);
    }

    #[test]
    fn lsp_warn_default_code_and_no_span() {
        let sink = Arc::new(CollectSink {
            rows: Mutex::new(Vec::new()),
        });
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let comp = LspDiagComponent::new(
            Severity::Warn,
            Arc::from("sprf/diag"),
            Arc::from("plain"),
            Vec::new(),
        );
        let pipe = PipeInstance::new(vec![Arc::new(comp) as Arc<dyn Component<Next = Cursor>>]);
        expand(
            &pipe,
            queue,
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default().with_diag(sink.clone()),
        );
        let rows = sink.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].severity, Severity::Warn);
        assert!(rows[0].span.is_none());
    }
}
