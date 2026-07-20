use crate::{Span, Symbol, TypeId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Primitive {
    String,
    Int,
    Bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Bool(bool),
}

#[derive(Clone, Debug)]
pub enum Type {
    Primitive(Primitive),
    Literal(Value),
    Record(RecordType),
    Union(Vec<TypeId>),
    Array(TypeId),
    Map { key: TypeId, value: TypeId },
    Optional(TypeId),
    Alias { name: Symbol, target: TypeId },
    Error,
}

#[derive(Clone, Debug)]
pub struct RecordType {
    pub name: Symbol,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: Symbol,
    pub ty: TypeId,
    pub span: Span,
}

impl Type {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Primitive(_) => "primitive",
            Self::Literal(_) => "literal",
            Self::Record(_) => "record",
            Self::Union(_) => "union",
            Self::Array(_) => "array",
            Self::Map { .. } => "map",
            Self::Optional(_) => "optional",
            Self::Alias { .. } => "alias",
            Self::Error => "error",
        }
    }
}
