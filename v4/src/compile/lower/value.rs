use std::sync::{Arc, Mutex};

use effect_runtime::v2::{
    expand, Component, ExpandOpts, MemQueue, Node, Pipe, PipeInstance, QueueBackend, RenderCtx,
};

use crate::Cursor;

use super::ctx::{LowerCtx, LowerError};

#[derive(Clone)]
pub enum Value {
    Atom(Arc<str>),
    Pipe(Pipe<Cursor>),
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
        Value::Atom(s.into())
    }
    pub fn pipe(p: Pipe<Cursor>) -> Self {
        Value::Pipe(p)
    }

    pub fn kind_str(&self) -> &'static str {
        match self {
            Value::Atom(_) => "atom",
            Value::Pipe(_) => "pipe",
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
