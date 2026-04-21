use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Atom(Arc<str>),
}

#[derive(Clone, Debug)]
pub enum Term {
    Scalar(Scalar),
    CaptureRef(CaptureSlot),
    Carveout(Box<Pipeline>),
    CursorRef(CursorRef),
    Xref { rule: Arc<str>, var: Arc<str> },
}

#[derive(Clone, Debug)]
pub enum CursorRef {
    Capture(CaptureSlot),
    Content,
}

pub type CaptureSlot = super::cursor::CaptureSlot;

#[derive(Clone, Debug)]
pub enum Pipeline {
    Op(OpCall),
    Chain(Vec<Pipeline>),
    Group(Box<Pipeline>),
    Fork(Vec<Pipeline>),
}

#[derive(Clone, Debug)]
pub struct OpCall {
    pub callee: Callee,
    pub terms: Vec<Term>,
}

#[derive(Clone, Debug)]
pub enum Callee {
    Builtin(Arc<str>),
    Rule(Arc<str>),
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub name: Arc<str>,
    pub params: Vec<Param>,
    pub body: Arc<Pipeline>,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Arc<str>,
    pub slot: CaptureSlot,
    pub mode: AcceptsMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AcceptsMode {
    BoundOnly,
    UnboundOnly,
    Either,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TermMode {
    Bound(Scalar),
    Unbound,
}

#[derive(Clone, Debug)]
pub enum OpAction {
    Emit(Vec<super::cursor::Cursor>),
    Drop,
}
