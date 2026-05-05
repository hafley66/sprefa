use std::collections::HashMap;
use std::sync::Arc;

use effect_runtime::v2::{ByteRange, Diag, Pipe};

use crate::Cursor;

use super::ctx::{LowerCtx, LowerError};
use super::op_def::{ArgKind, DslBody, OperatorDef};
use super::value::Value;

pub struct Registry {
    map: HashMap<&'static str, Arc<dyn OperatorDef>>,
}

impl Registry {
    pub fn new() -> Self { Self { map: HashMap::new() } }
    pub fn register(&mut self, def: Arc<dyn OperatorDef>) {
        self.map.insert(def.name(), def);
    }
    pub fn get(&self, name: &str) -> Option<Arc<dyn OperatorDef>> {
        self.map.get(name).cloned()
    }
    pub fn names(&self) -> Vec<&'static str> { self.map.keys().copied().collect() }

    /// Validate then lower. The four-slot tuple shape mirrors what the
    /// host parser will hand in: every slot carries an optional value
    /// and the byte range it occupied in source.
    #[allow(clippy::too_many_arguments)]
    pub fn lower(
        &self,
        ctx:       &LowerCtx,
        name:      &str,
        flow:      Option<(Value, ByteRange)>,
        args:      Vec<(Value, ByteRange)>,
        block:     Option<(Pipe<Cursor>, ByteRange)>,
        dsl:       Option<(DslBody, ByteRange)>,
        call_span: ByteRange,
    ) -> Result<Pipe<Cursor>, Vec<Diag>> {
        let def = self.get(name).ok_or_else(|| vec![
            Diag::error("lower/unknown-op", format!("unknown op: {name}"))
                .with_span(call_span.lo, call_span.hi),
        ])?;
        let diags = validate_call(&*def, &flow, &args, &block, &dsl, call_span);
        if !diags.is_empty() { return Err(diags); }
        let flow_val  = flow.map(|(v, _)| v);
        let arg_vals: Vec<Value> = args.into_iter().map(|(v, _)| v).collect();
        let block_val = block.map(|(p, _)| p);
        let dsl_ref   = dsl.as_ref().map(|(d, _)| d);
        def.lower(ctx, flow_val, &arg_vals, block_val, dsl_ref).map_err(|e| match e {
            LowerError::Validate(ds) => ds,
            LowerError::UnboundCapture(n) => vec![
                Diag::error("lower/unbound-capture",
                    format!("unbound capture: ${{{n}}}"))
                    .with_span(call_span.lo, call_span.hi),
            ],
            LowerError::Unknown(s) => vec![
                Diag::error("lower/unknown", s)
                    .with_span(call_span.lo, call_span.hi),
            ],
        })
    }
}

impl Default for Registry { fn default() -> Self { Self::new() } }

/// Diag-emit pass over a call site. Per-slot:
///   - present-but-not-allowed → `lower/slot-not-allowed` on the slot range
///   - absent-but-required     → `lower/missing-slot`     on call_span
/// For `args`:
///   - arity over the trailing `Variadic` is open-ended
///   - per-positional kind mismatch → `lower/wrong-arg-kind`
pub fn validate_call(
    def:       &dyn OperatorDef,
    flow:      &Option<(Value, ByteRange)>,
    args:      &[(Value, ByteRange)],
    block:     &Option<(Pipe<Cursor>, ByteRange)>,
    dsl:       &Option<(DslBody, ByteRange)>,
    call_span: ByteRange,
) -> Vec<Diag> {
    let mut diags = Vec::new();

    // []  flow
    match (def.flow_arg(), flow) {
        (None, Some((_, r))) => diags.push(
            Diag::error("lower/slot-not-allowed",
                format!("op `{}` does not accept a [] flow override", def.name()))
                .with_span(r.lo, r.hi)),
        (Some(sig), None) if sig.required => diags.push(
            Diag::error("lower/missing-slot",
                format!("op `{}` requires a [] flow override", def.name()))
                .with_span(call_span.lo, call_span.hi)),
        (Some(sig), Some((v, r))) if !sig.kind.matches(v) => diags.push(
            Diag::error("lower/wrong-arg-kind",
                format!("flow expected {}, got {}", sig.kind.label(), v.kind_str()))
                .with_span(r.lo, r.hi)),
        _ => {}
    }

    // ()  paren args
    let spec = def.paren_args();
    let has_variadic = spec.last().map(|s| matches!(s.kind, ArgKind::Variadic(_))).unwrap_or(false);
    let n_fixed = if has_variadic { spec.len() - 1 } else { spec.len() };
    let n_required = spec.iter().filter(|s| s.required && !matches!(s.kind, ArgKind::Variadic(_))).count();

    if args.len() < n_required {
        diags.push(
            Diag::error("lower/missing-slot",
                format!("op `{}` requires at least {} positional arg(s), got {}",
                    def.name(), n_required, args.len()))
                .with_span(call_span.lo, call_span.hi));
    }
    if !has_variadic && args.len() > spec.len() {
        // extra trailing args without a variadic to absorb them
        for (extra, r) in &args[spec.len()..] {
            diags.push(
                Diag::error("lower/slot-not-allowed",
                    format!("op `{}` does not accept arg #{} ({})",
                        def.name(), spec.len(), extra.kind_str()))
                    .with_span(r.lo, r.hi));
        }
    }
    for (i, (val, r)) in args.iter().enumerate() {
        let sig = if i < n_fixed { &spec[i] }
                  else if has_variadic { &spec[spec.len() - 1] }
                  else { break };
        let inner_kind: &ArgKind = match &sig.kind {
            ArgKind::Variadic(inner) => *inner,
            other => other,
        };
        if !inner_kind.matches(val) {
            diags.push(
                Diag::error("lower/wrong-arg-kind",
                    format!("arg {} ({}) expected {}, got {}",
                        i, sig.name, inner_kind.label(), val.kind_str()))
                    .with_span(r.lo, r.hi));
        }
    }

    // {}  block
    match (def.brace_block(), block) {
        (None, Some((_, r))) => diags.push(
            Diag::error("lower/slot-not-allowed",
                format!("op `{}` does not accept a {{}} block", def.name()))
                .with_span(r.lo, r.hi)),
        (Some(_), None) => diags.push(
            Diag::error("lower/missing-slot",
                format!("op `{}` requires a {{}} block", def.name()))
                .with_span(call_span.lo, call_span.hi)),
        _ => {}
    }

    // ``  dsl
    match (def.dsl_body(), dsl) {
        (None, Some((_, r))) => diags.push(
            Diag::error("lower/slot-not-allowed",
                format!("op `{}` does not accept a `` dsl body", def.name()))
                .with_span(r.lo, r.hi)),
        (Some(_), None) => diags.push(
            Diag::error("lower/missing-slot",
                format!("op `{}` requires a `` dsl body", def.name()))
                .with_span(call_span.lo, call_span.hi)),
        _ => {}
    }

    diags
}
