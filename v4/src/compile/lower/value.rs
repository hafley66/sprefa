use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, MemQueue, Node, Pipe, PipeInstance, QueueBackend, RenderCtx,
};

use crate::Cursor;

use super::ctx::{LowerCtx, LowerError};

/// The actual payload of a value. Was `Value`; renamed so `Value` can
/// wrap it with a per-value dot table (Ruby-metaclass model).
#[derive(Clone)]
pub enum ValueKind {
    Atom(Arc<str>),
    Pipe(Pipe<Cursor>),
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
        }
    }
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
