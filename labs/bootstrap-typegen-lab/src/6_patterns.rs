use crate::{Span, Symbol, TypeId, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotSpelling {
    Braces,
    Colon,
}

impl SlotSpelling {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Braces => "braces",
            Self::Colon => "colon",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub name: Option<Symbol>,
    pub position: u32,
    pub ty: TypeId,
    pub spelling: SlotSpelling,
    pub source: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum PatternPart {
    Literal { text: String, span: Span },
    Slot(Slot),
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub name: Option<Symbol>,
    pub parts: Vec<PatternPart>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Bindings {
    pub positional: Vec<Value>,
    pub named: Vec<(Symbol, Value)>,
}

#[derive(Clone, Debug)]
pub enum ArgumentValue {
    Positional(Value),
    Named(String, Value),
}

#[derive(Debug, PartialEq)]
pub enum PatternError {
    MissingBinding(String),
    DuplicateBinding(String),
    ExtraBinding(String),
    TypeMismatch(String),
    NoMatch,
    AmbiguousMatch(String),
    UnknownPattern(String),
}
