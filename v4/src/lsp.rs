//! `lsp_*` / `lsp.hover` / `expect_*` op family — emit Diag events anchored
//! to a cursor coord. All in one file per user constraint.
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
//!   `:code` — diagnostic code atom (positional, optional; defaults vary
//!             per op). One short kebab/slash atom.
//!
//! DSL body — message template; `${X}` interps resolve from `cursor.terms`
//! at render time, mirroring `str`'s template path.
//!
//! Cursor flow:
//!   lsp_*           — pass-through, one diag per row.
//!   lsp.hover       — pass-through, one Hint per row IFF span resolves.
//!   expect_zero     — drops every input row, one Error diag per row.
//!                     Empty input = no diags, no rows.
//!   expect_match    — pass-through; tracks "saw any row" across the rule
//!                     run and emits one Warn diag at run-end if not.
//!
//! Span — diag byte range comes from `cursor.at` (via SprfStore), or the
//! focused term's `at` when `lsp_warn[NAME]` form is used. Legacy
//! `LO`/`HI` / `NAME_LO`/`NAME_HI` cursor terms are fallbacks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    splice_into, BarrierScope, ByteRange, Component, ComponentLifecycle, Diag, DiagPosition, Node,
    PendingSummary, Pipe, QueueBackend, QueueRow, RenderCtx, Severity,
};

use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{
    default_plain_dsl_parse, ArgKind, ArgSig, DslBinder, DslBody, DslInterp, DslShape, InterpKind,
    OperatorDef,
};
use crate::compile::lower::value::{Value, ValueKind};
use crate::sprf_introspect::PipeIntrospect;
use crate::store::SprfStore;
use crate::{Cursor, Ref, StringId};

pub const LSP_HOVER_CODE: &str = "sprf/hover";

// ─── Shared helpers ────────────────────────────────────────────────────────

/// Render a `${TERM}` / `${TERM.field}` template against a cursor. Used by
/// every diag-emitting component in this file (was three near-clones).
fn render_dsl_message(template: &str, interps: &[DslInterp], c: &Cursor) -> String {
    let mut out = String::with_capacity(template.len());
    let mut head: usize = 0;
    for interp in interps {
        let lo = interp.range.lo as usize;
        let hi = interp.range.hi as usize;
        if lo > template.len() || hi > template.len() || lo < head {
            continue;
        }
        out.push_str(&template[head..lo]);
        if let InterpKind::Term { field, .. } = &interp.kind {
            let key: std::borrow::Cow<'_, str> = match field {
                None => (&*interp.name).into(),
                Some(f) => format!("{}.{}", interp.name, f).into(),
            };
            if let Some(v) = c.get(&key) {
                out.push_str(v);
            }
        }
        // SubPipe carveouts are skipped — diag templates render messages,
        // not pipe values.
        head = hi;
    }
    out.push_str(&template[head..]);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LspDiagFocus {
    Focal,
    Term(Arc<str>),
}

/// Resolve a diag's byte span: prefer the cursor coord (via SprfStore),
/// fall back to legacy `LO`/`HI` cursor terms. Used by every diag-
/// emitting component (was three near-clones).
fn resolve_diag_span(
    focus: &LspDiagFocus,
    c: &Cursor,
    store: Option<&Arc<SprfStore>>,
) -> Option<(u32, u32)> {
    match focus {
        LspDiagFocus::Focal => span_from_ref(c.at, store).or_else(|| span_from_legacy(c, "LO", "HI")),
        LspDiagFocus::Term(name) => {
            let name_id = StringId::of(name.as_ref());
            c.term(name_id)
                .and_then(|t| span_from_ref(t.at, store))
                .or_else(|| {
                    span_from_legacy(
                        c,
                        &format!("{}_LO", name.as_ref()),
                        &format!("{}_HI", name.as_ref()),
                    )
                })
        }
    }
}

fn span_from_ref(at: Ref, store: Option<&Arc<SprfStore>>) -> Option<(u32, u32)> {
    if at == Ref::SYNTHETIC {
        return None;
    }
    let coord = store?.coord_of(at)?;
    Some((coord.lo, coord.hi))
}

/// Resolve `(file_id, bytes)` for a focal cursor coord. The bytes are
/// the SAME ones that `intern_file` retained when the source was first
/// ingested. Returns None when the cursor has no coord, no FileId, or
/// the bytes were evicted from the SprfStore LRU.
fn bytes_for_diag(
    focus: &LspDiagFocus,
    c: &Cursor,
    store: Option<&Arc<SprfStore>>,
) -> Option<Arc<[u8]>> {
    let store = store?;
    let at = match focus {
        LspDiagFocus::Focal => c.at,
        LspDiagFocus::Term(name) => {
            let name_id = StringId::of(name.as_ref());
            c.term(name_id)?.at
        }
    };
    if at == Ref::SYNTHETIC {
        return None;
    }
    let coord = store.coord_of(at)?;
    if coord.fs == 0 {
        return None;
    }
    store.file_bytes(coord.fs)
}

/// Compute LSP (line, utf16-col) for a byte offset against `src`. Same
/// shape as the publisher-side `byte_to_position` in sprefa-lsp; lifted
/// here so positions are minted T0 (at emit, against bytes that were
/// just read) rather than T1 (at publish, after a buffer edit).
fn byte_to_line_col(src: &[u8], off: usize) -> (u32, u32) {
    let off = off.min(src.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, b) in src.iter().enumerate() {
        if i == off {
            break;
        }
        if *b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    // utf16 column: count UTF-16 code units between line_start..off.
    let slice = &src[line_start..off];
    let col: u32 = if slice.is_ascii() {
        slice.len() as u32
    } else {
        match std::str::from_utf8(slice) {
            Ok(s) => s.chars().map(|c| c.len_utf16() as u32).sum(),
            Err(_) => slice.len() as u32,
        }
    };
    (line, col)
}

fn position_for_span(bytes: &[u8], lo: u32, hi: u32) -> DiagPosition {
    let (line_lo, col_lo) = byte_to_line_col(bytes, lo as usize);
    let (line_hi, col_hi) = byte_to_line_col(bytes, hi as usize);
    DiagPosition {
        line_lo,
        col_lo,
        line_hi,
        col_hi,
    }
}

fn span_from_legacy(c: &Cursor, lo_key: &str, hi_key: &str) -> Option<(u32, u32)> {
    let lo = c.get(lo_key)?.parse::<u32>().ok()?;
    let hi = c.get(hi_key)?.parse::<u32>().ok()?;
    Some((lo, hi))
}

fn mk_diag(sev: Severity, code: Arc<str>, message: String) -> Diag {
    match sev {
        Severity::Error => Diag::error(code, message),
        Severity::Warn => Diag::warn(code, message),
        Severity::Info => Diag::info(code, message),
        Severity::Hint => Diag::hint(code, message),
    }
}

/// If the cursor carries an absolute non-`.sprf` `FS` column, return a
/// `file://` URI string suitable for cross-URI diag routing. Otherwise
/// return `None`: the diag publishes on the requesting `.sprf` URI.
fn cross_file_uri_for(c: &Cursor) -> Option<String> {
    let fs = c.get("FS")?;
    if fs.is_empty() {
        return None;
    }
    let p = std::path::Path::new(fs);
    if !p.is_absolute() {
        return None;
    }
    if fs.ends_with(".sprf") {
        return None;
    }
    // Url::from_file_path returns Err(()) for non-absolute paths or
    // platforms that cannot represent the path; we just checked
    // is_absolute() so the only remaining failure is platform shape
    // (Windows UNC paths on non-Windows), which we treat as None.
    url::Url::from_file_path(p).ok().map(|u| u.to_string())
}

// ─── LspBodyComponent ──────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LspBodyKind {
    /// Per-row: emit diag, pass row through. lsp_error / lsp_warn / lsp_info / lsp_hint.
    Diag(Severity),
    /// Per-row: emit Hint with `sprf/hover` code, only if span resolves; pass row through.
    Hover,
    /// Per-row: emit diag, DROP the row. expect_zero.
    ExpectZero(Severity),
}

pub struct LspBodyComponent {
    kind: LspBodyKind,
    code: Arc<str>,
    template: Arc<str>,
    interps: Arc<Vec<DslInterp>>,
    focus: LspDiagFocus,
    store: Option<Arc<SprfStore>>,
}

impl LspBodyComponent {
    pub fn diag(
        severity: Severity,
        code: Arc<str>,
        template: Arc<str>,
        interps: Vec<DslInterp>,
    ) -> Self {
        Self {
            kind: LspBodyKind::Diag(severity),
            code,
            template,
            interps: Arc::new(interps),
            focus: LspDiagFocus::Focal,
            store: None,
        }
    }

    pub fn hover(template: Arc<str>, interps: Vec<DslInterp>) -> Self {
        Self {
            kind: LspBodyKind::Hover,
            code: Arc::<str>::from(LSP_HOVER_CODE),
            template,
            interps: Arc::new(interps),
            focus: LspDiagFocus::Focal,
            store: None,
        }
    }

    pub fn expect_zero(
        severity: Severity,
        code: Arc<str>,
        template: Arc<str>,
        interps: Vec<DslInterp>,
    ) -> Self {
        Self {
            kind: LspBodyKind::ExpectZero(severity),
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
}

impl Component for LspBodyComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        match self.kind {
            LspBodyKind::Diag(sev) => {
                let message = render_dsl_message(&self.template, &self.interps, c);
                let mut diag = mk_diag(sev, self.code.clone(), message);
                if let Some((lo, hi)) = resolve_diag_span(&self.focus, c, self.store.as_ref()) {
                    diag = diag.with_span(lo, hi);
                    if let Some(bytes) = bytes_for_diag(&self.focus, c, self.store.as_ref()) {
                        diag = diag.with_position(position_for_span(&bytes, lo, hi));
                    }
                }
                if let Some(uri) = cross_file_uri_for(c) {
                    diag = diag.with_target_uri(uri);
                }
                ctx.diag.emit(diag);
                Node::Emit(Arc::new(c.clone()))
            }
            LspBodyKind::Hover => {
                if let Some((lo, hi)) = resolve_diag_span(&self.focus, c, self.store.as_ref()) {
                    let message = render_dsl_message(&self.template, &self.interps, c);
                    let mut diag = Diag::hint(self.code.clone(), message).with_span(lo, hi);
                    if let Some(bytes) = bytes_for_diag(&self.focus, c, self.store.as_ref()) {
                        diag = diag.with_position(position_for_span(&bytes, lo, hi));
                    }
                    if let Some(uri) = cross_file_uri_for(c) {
                        diag = diag.with_target_uri(uri);
                    }
                    ctx.diag.emit(diag);
                }
                Node::Emit(Arc::new(c.clone()))
            }
            LspBodyKind::ExpectZero(sev) => {
                let message = render_dsl_message(&self.template, &self.interps, c);
                let mut diag = mk_diag(sev, self.code.clone(), message);
                if let Some((lo, hi)) = resolve_diag_span(&self.focus, c, self.store.as_ref()) {
                    diag = diag.with_span(lo, hi);
                    if let Some(bytes) = bytes_for_diag(&self.focus, c, self.store.as_ref()) {
                        diag = diag.with_position(position_for_span(&bytes, lo, hi));
                    }
                }
                if let Some(uri) = cross_file_uri_for(c) {
                    diag = diag.with_target_uri(uri);
                }
                ctx.diag.emit(diag);
                Node::Done
            }
        }
    }
}

// ─── LspBodyDef (one parameterized OperatorDef) ────────────────────────────

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

#[derive(Clone, Copy)]
enum DefShape {
    /// lsp_error / lsp_warn / lsp_info / lsp_hint: flow-focus + code paren arg.
    Diag(Severity, &'static str), // (sev, default_code)
    /// lsp.hover / lsp_hover: flow-focus, no code paren arg (code is LSP_HOVER_CODE).
    Hover,
    /// expect_zero: no flow-focus, code paren arg, dsl optional.
    ExpectZero(Severity, &'static str),
}

pub struct LspBodyDef {
    name: &'static str,
    shape: DefShape,
}

impl LspBodyDef {
    pub const fn lsp_error() -> Self {
        Self {
            name: "lsp_error",
            shape: DefShape::Diag(Severity::Error, "sprf/diag"),
        }
    }
    pub const fn lsp_warn() -> Self {
        Self {
            name: "lsp_warn",
            shape: DefShape::Diag(Severity::Warn, "sprf/diag"),
        }
    }
    pub const fn lsp_info() -> Self {
        Self {
            name: "lsp_info",
            shape: DefShape::Diag(Severity::Info, "sprf/diag"),
        }
    }
    pub const fn lsp_hint() -> Self {
        Self {
            name: "lsp_hint",
            shape: DefShape::Diag(Severity::Hint, "sprf/diag"),
        }
    }
    pub const fn lsp_hover_dot() -> Self {
        Self {
            name: "lsp.hover",
            shape: DefShape::Hover,
        }
    }
    pub const fn lsp_hover_alias() -> Self {
        Self {
            name: "lsp_hover",
            shape: DefShape::Hover,
        }
    }
    pub const fn expect_zero() -> Self {
        Self {
            name: "expect_zero",
            shape: DefShape::ExpectZero(Severity::Error, "expect_zero"),
        }
    }
}

impl OperatorDef for LspBodyDef {
    fn name(&self) -> &'static str {
        self.name
    }

    fn flow_arg(&self) -> Option<ArgSig> {
        match self.shape {
            DefShape::Diag(..) | DefShape::Hover => Some(LSP_FLOW),
            DefShape::ExpectZero(..) => None,
        }
    }

    fn paren_args(&self) -> &[ArgSig] {
        match self.shape {
            DefShape::Diag(..) | DefShape::ExpectZero(..) => LSP_SPEC,
            DefShape::Hover => &[],
        }
    }

    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }

    fn dsl_required(&self) -> bool {
        match self.shape {
            DefShape::Diag(..) | DefShape::Hover => true,
            DefShape::ExpectZero(..) => false,
        }
    }

    fn binders_in_dsl(&self, _raw: &str) -> Vec<DslBinder> {
        // ${IDENT} interps don't BIND, they READ.
        Vec::new()
    }

    fn parse_dsl(&self, raw: &str) -> Result<Vec<DslInterp>, LowerError> {
        Ok(default_plain_dsl_parse(raw))
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let mut comp = match self.shape {
            DefShape::Diag(sev, default_code) => {
                let code = code_arg_or_default(args, default_code);
                let (template, interps) = take_required_dsl_body(self.name, dsl)?;
                LspBodyComponent::diag(sev, code, template, interps)
                    .with_focus(lsp_focus_from_flow(flow)?)
            }
            DefShape::Hover => {
                let (template, interps) = take_required_dsl_body(self.name, dsl)?;
                LspBodyComponent::hover(template, interps).with_focus(lsp_focus_from_flow(flow)?)
            }
            DefShape::ExpectZero(sev, default_code) => {
                let code = code_arg_or_default(args, default_code);
                let (template, interps) = take_optional_dsl_body(dsl);
                // ExpectZero has no [TERM] focus (flow_arg = None ⇒ flow = None).
                LspBodyComponent::expect_zero(sev, code, template, interps)
            }
        };
        if let Some(store) = &ctx.sprf_store {
            comp = comp.with_sprf_store(store.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

fn code_arg_or_default(args: &[Value], default_code: &'static str) -> Arc<str> {
    args.first()
        .and_then(|v| match v.kind() {
            ValueKind::Atom(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| Arc::<str>::from(default_code))
}

fn take_required_dsl_body(
    op: &'static str,
    dsl: Option<&DslBody>,
) -> Result<(Arc<str>, Vec<DslInterp>), LowerError> {
    let body = dsl.ok_or_else(|| {
        LowerError::Unknown(format!("{op}: dsl body required (e.g. {op}`msg ${{X}}`)"))
    })?;
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);
    Ok((body.raw.clone(), interps))
}

fn take_optional_dsl_body(dsl: Option<&DslBody>) -> (Arc<str>, Vec<DslInterp>) {
    match dsl {
        Some(body) => {
            let mut interps = body.interps.clone();
            interps.sort_by_key(|i| i.range.lo);
            (body.raw.clone(), interps)
        }
        None => (Arc::<str>::from(""), Vec::new()),
    }
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

// ─── expect_match — barrier-scoped, keeps its own shape ────────────────────
//
// `expect_match` tracks "saw any row" per barrier scope. `complete()`
// (called by the runtime when upstream is drained at this depth) emits
// one Warn diag if no row was seen. Cannot share LspBodyComponent because
// its lifecycle is BARRIER, not the per-row dispatch path.

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
    fn binders_in_dsl(&self, _raw: &str) -> Vec<DslBinder> {
        Vec::new()
    }
    fn parse_dsl(&self, raw: &str) -> Result<Vec<DslInterp>, LowerError> {
        Ok(default_plain_dsl_parse(raw))
    }
    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let code = code_arg_or_default(args, "expect_match");
        let template: Arc<str> = dsl
            .map(|b| b.raw.clone())
            .unwrap_or_else(|| Arc::<str>::from("expected at least one row, got none"));
        let comp = ExpectMatchComponent::new(code, template, ctx.current_call_span.get());
        Ok(Pipe::new().step(Arc::new(comp)))
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
        let comp = LspBodyComponent::diag(
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
        let comp = LspBodyComponent::diag(
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
        let comp = LspBodyComponent::diag(
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
        let comp = LspBodyComponent::diag(
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
