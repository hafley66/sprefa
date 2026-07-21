//! The TYPE SYSTEM / value space. Nested RECORD types + dot-access (projection).
//! "Nesting must make sense" = well-typed projection: every `.field` resolves to a
//! FieldKind, which decides how it lowers (see below + lang::lower). No FieldKind
//! for a `.field` -> type error. So the lowered join can never dangle.

use crate::_0_key::{FieldId, SymId};

pub enum Type {
    Scalar(Scalar),
    Record(RecordType),   // Souffle-style interned record (one dense id, fields in a table)
    Adt(AdtType),         // Souffle $Branch sum type
    RelRef(SymId),        // a reference to another relation (for relation-valued fields)
}

pub enum Scalar { Int, Str, Sym, Bool }

pub struct RecordType {
    pub fields: Vec<Field>,
}

pub struct Field {
    pub name: FieldId,
    pub kind: FieldKind,   // how `x.name` lowers
    pub ty: Box<Type>,
}

pub struct AdtType {
    pub branches: Vec<(SymId, Vec<Type>)>,
}

/// The GENERAL dot dispatch (not functional-only). One `Proj` operator; its
/// lowering is chosen by the field's kind, so it covers every scenario:
///   Functional  -> one join            (loc.file)              [Datomic card-one]
///   Record      -> record-table lookup                        [Souffle interning]
///   AdtBranch   -> a match / guard                            [Souffle $Branch]
///   Many        -> a join that FANS OUT (explicit set)        [Datomic card-many]
/// New scenario = ONE new FieldKind + its one lowering rule. Nothing excluded.
pub enum FieldKind {
    Functional,
    Record,
    AdtBranch,
    Many,
}
