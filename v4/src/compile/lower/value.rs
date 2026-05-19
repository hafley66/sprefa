use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, ByteRange, Component, ExpandOpts, MemQueue, Node, Pipe, PipeInstance, QueueBackend,
    RenderCtx,
};

use crate::rule::{RuleInvokeAssign, RuleInvokeComponent, RuleInvokeValue};
use crate::Cursor;

use super::ctx::{LowerCtx, LowerError};
use super::registry::Registry;

/// Which kind of callable a `CallableRef` names. `Debug` is derived so
/// callers (and the spec) can format the discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallableKind {
    Op,
    Rule,
}

/// An un-applied callable handle. References its target by NAME only —
/// no `Arc<dyn OperatorDef>`, no embedded rule body. Resolution is
/// on-use (`apply`) against the owning `Registry` (Op) or the ctx's
/// rule table (Rule). Identity is `(kind, name)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallableRef {
    pub kind: CallableKind,
    pub name: Arc<str>,
}

/// The actual payload of a value. Was `Value`; renamed so `Value` can
/// wrap it with a per-value dot table (Ruby-metaclass model).
#[derive(Clone)]
pub enum ValueKind {
    Atom(Arc<str>),
    Pipe(Pipe<Cursor>),
    /// An un-applied callable. Inert until `apply`; the third variant
    /// of the §Premise collapse (Atom | Pipe = applied | Callable =
    /// un-applied).
    Callable(CallableRef),
}

/// Per-value singleton dot table. `map` = instance dots (read path now;
/// decls later). `ty` = the rule name that is this value's TYPE; a dot
/// miss on `map` falls through to that rule's parametric projection
/// (see `LowerCtx::resolve_dot`). Cloned copy-on-write via
/// `Arc::make_mut` (user ruling 2026-05-18: clone => independent).
#[derive(Clone, Default)]
pub struct DotTable {
    pub map: HashMap<Arc<str>, Value>,
    pub ty: Option<Arc<str>>,
}

/// Every value carries its own dot table. `dots` is `Arc`-shared until
/// first mutation, then `Arc::make_mut` copies it — so `v.clone()`
/// is cheap and writes do not propagate to prior clones.
#[derive(Clone)]
pub struct Value {
    pub kind: ValueKind,
    pub dots: Arc<DotTable>,
}

impl From<ValueKind> for Value {
    fn from(kind: ValueKind) -> Self {
        Value {
            kind,
            dots: Arc::new(DotTable::default()),
        }
    }
}

#[derive(Clone)]
pub struct CallArg {
    pub keyword: Option<Arc<str>>,
    pub value: Value,
}

impl CallArg {
    pub fn positional(value: Value) -> Self {
        Self {
            keyword: None,
            value,
        }
    }

    pub fn keyword(name: impl Into<Arc<str>>, value: Value) -> Self {
        Self {
            keyword: Some(name.into()),
            value,
        }
    }
}

impl Value {
    pub fn atom(s: impl Into<Arc<str>>) -> Self {
        ValueKind::Atom(s.into()).into()
    }
    pub fn pipe(p: Pipe<Cursor>) -> Self {
        ValueKind::Pipe(p).into()
    }
    /// Construct an un-applied callable handle. Op callables are NOT
    /// auto-`.typed()` (only `Callable(Rule)` projects columns; see
    /// `resolve_dot`'s H4 arm).
    pub fn callable(kind: CallableKind, name: impl Into<Arc<str>>) -> Self {
        ValueKind::Callable(CallableRef {
            kind,
            name: name.into(),
        })
        .into()
    }

    /// The payload, for matching: `match v.kind() { ValueKind::Atom .. }`.
    pub fn kind(&self) -> &ValueKind {
        &self.kind
    }

    /// Tag this value's TYPE (the rule whose columns are its fields).
    /// Copy-on-write: mutates this value's own table only.
    pub fn typed(mut self, rule: impl Into<Arc<str>>) -> Self {
        Arc::make_mut(&mut self.dots).ty = Some(rule.into());
        self
    }

    /// Set an instance dot (metaclass override). Copy-on-write.
    pub fn set_dot(&mut self, key: impl Into<Arc<str>>, val: Value) {
        Arc::make_mut(&mut self.dots).map.insert(key.into(), val);
    }

    pub fn kind_str(&self) -> &'static str {
        match self.kind {
            ValueKind::Atom(_) => "atom",
            ValueKind::Pipe(_) => "pipe",
            ValueKind::Callable(_) => "callable",
        }
    }
}

/// Map a positional arg `Value` to a `RuleInvokeValue`. Atom → a
/// literal cell; Pipe → the cursor's primary value (the applied
/// callable's output flows in as the seed value); Callable → not yet
/// supported as a rule arg (loud error, never a silent miss).
fn rule_invoke_value_of(v: &Value) -> Result<RuleInvokeValue, LowerError> {
    match v.kind() {
        ValueKind::Atom(s) => Ok(RuleInvokeValue::Literal(s.clone())),
        ValueKind::Pipe(_) => Ok(RuleInvokeValue::Value),
        ValueKind::Callable(_) => Err(LowerError::Unknown(
            "callable passed as a rule argument is not supported yet".into(),
        )),
    }
}

/// Apply a callable to args = lower the callable+args into one Pipe
/// step (the §Premise collapse: application IS a Pipe). Arity-exact,
/// no curry. `reg` is explicit (H1: `LowerCtx` has no registry
/// back-pointer; the caller — the walker — holds both).
///
/// pseudo:
///   Op   → reg.lower(ctx, name, None, args⊗span, None, None, span)?  (H2)
///   Rule → RuleInvokeComponent::new(rule, assigns, false) in a Pipe  (H3/H7)
///   out  = Value::pipe(that)                                         (Premise)
pub fn apply(
    ctx: &LowerCtx,
    reg: &Registry,
    c: &CallableRef,
    args: Vec<Value>,
) -> Result<Value, LowerError> {
    let pipe = match c.kind {
        CallableKind::Op => {
            // H2: real 7-arg Registry::lower; Vec<Diag> → Validate.
            let span = ctx
                .current_call_span
                .get()
                .unwrap_or(ByteRange { lo: 0, hi: 0 });
            let lowered: Vec<(Value, ByteRange)> =
                args.into_iter().map(|v| (v, span)).collect();
            reg.lower(ctx, &c.name, None, lowered, None, None, span)
                .map_err(LowerError::Validate)?
        }
        CallableKind::Rule => {
            // H3: the EXISTING invoke path (rule.rs:164), wrapped in a
            // Pipe. H7: this is exactly the component a normal rule
            // invoke lowers to, so a self-applying rule inherits the
            // recursion wake-guard — no new subscription path.
            let rule = ctx
                .get_rule(&c.name)
                .ok_or_else(|| LowerError::Unknown(format!("no rule `{}`", c.name)))?;
            let assigns: Vec<RuleInvokeAssign> = rule
                .sink_cols
                .iter()
                .zip(args.iter())
                .map(|(col, v)| {
                    rule_invoke_value_of(v).map(|value| RuleInvokeAssign {
                        col: col.clone(),
                        value,
                    })
                })
                .collect::<Result<_, _>>()?;
            let comp = RuleInvokeComponent::new(rule, assigns, false);
            Pipe::new().step(Arc::new(comp))
        }
    };
    Ok(Value::pipe(pipe))
}

/// Run a Pipe<Cursor> over an empty seed and concatenate the values of
/// emitted cursors into one string. Used by the str op to ground
/// `${X}` bindings into the literal.
pub fn run_once_const(p: &Pipe<Cursor>, _ctx: &LowerCtx) -> Result<String, LowerError> {
    let sink: Arc<Mutex<Vec<Cursor>>> = Arc::new(Mutex::new(Vec::new()));
    let mut all_steps: Vec<Arc<dyn Component<Next = Cursor>>> = p.steps.iter().cloned().collect();
    all_steps.push(Arc::new(Collector { sink: sink.clone() }));
    let inst = PipeInstance::new(all_steps);
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    expand(
        &inst,
        queue,
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default(),
    );
    let rows = sink.lock().unwrap();
    if rows.is_empty() {
        return Err(LowerError::Unknown(
            "run_once_const: pipe emitted nothing".into(),
        ));
    }
    Ok(rows
        .iter()
        .map(|c| c.value.as_ref())
        .collect::<Vec<_>>()
        .join(""))
}

struct Collector {
    sink: Arc<Mutex<Vec<Cursor>>>,
}
impl Component for Collector {
    type Next = Cursor;
    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.sink.lock().unwrap().push(c.clone());
        Node::Done
    }
}
