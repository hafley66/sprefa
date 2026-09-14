//! The row store seam: the closure and its arena at rest.

use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Term, TermId, Universe};
use std::fmt;

/// Dense arena cursor. Everything below each index is already durable.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct Watermark {
    pub syms: usize,
    pub terms: usize,
}

#[derive(Debug)]
pub enum StoreError {
    NestedBegin,
    ArenaMismatch {
        at: usize,
        stored: String,
        expected: String,
    },
    ArityOverflow {
        arity: usize,
    },
    ColumnKind {
        table: String,
        position: usize,
        stored: CellKind,
        arriving: CellKind,
    },
    Sql(rusqlite::Error),
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sql(e)
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NestedBegin => write!(f, "begin_tick inside an open tick"),
            StoreError::ArenaMismatch {
                at,
                stored,
                expected,
            } => write!(f, "arena id {at} holds {stored}, the db carries {expected}"),
            StoreError::ArityOverflow { arity } => {
                write!(f, "kernel row of arity {arity} exceeds {KERNEL_ARITY}")
            }
            StoreError::ColumnKind {
                table,
                position,
                stored,
                arriving,
            } => write!(
                f,
                "{table} column {position} is {stored:?}, the row carries {arriving:?}"
            ),
            StoreError::Sql(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Widest kernel row the shared kernel table holds.
pub const KERNEL_ARITY: usize = 8;

/// A scalar cell stores its value, every reference cell its arena id, so no
/// key and no index of a product table ever copies text.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CellKind {
    Int,
    Float,
    Bool,
    Term,
}

impl CellKind {
    pub fn of(term: &Term) -> CellKind {
        match term {
            Term::Int(_) => CellKind::Int,
            Term::Float(_) => CellKind::Float,
            Term::Bool(_) => CellKind::Bool,
            Term::Atom(_) | Term::Str(_) | Term::Compound(_, _) => CellKind::Term,
        }
    }

    /// The column name carries the kind, so `pragma_table_info` is the only
    /// catalog a reopened table needs.
    pub fn suffix(self) -> &'static str {
        match self {
            CellKind::Int => "int",
            CellKind::Float => "float",
            CellKind::Bool => "bool",
            CellKind::Term => "term",
        }
    }

    pub fn of_suffix(text: &str) -> Option<CellKind> {
        match text {
            "int" => Some(CellKind::Int),
            "float" => Some(CellKind::Float),
            "bool" => Some(CellKind::Bool),
            "term" => Some(CellKind::Term),
            _ => None,
        }
    }

    pub fn sql_type(self) -> &'static str {
        match self {
            CellKind::Float => "REAL",
            _ => "INTEGER",
        }
    }
}

pub trait IRowStore {
    /// Idempotent DDL for one program name. Runs once per open.
    fn open(&mut self, program: &str) -> Result<(), StoreError>;

    /// `u` must be a fresh `Universe::new()`: stored ids are arena positions.
    fn load_arena(&mut self, u: &mut Universe) -> Result<Watermark, StoreError>;

    /// `u` is mutable because a scalar column stores its value, not an arena id.
    fn load_rows(&mut self, u: &mut Universe, store: &mut Store) -> Result<usize, StoreError>;

    fn watermark(&self) -> Watermark;

    fn begin_tick(&mut self) -> Result<(), StoreError>;

    /// Appends `u.syms[from.syms..]` and `u.terms[from.terms..]`, never below.
    fn commit_arena(&mut self, u: &Universe, from: Watermark) -> Result<Watermark, StoreError>;

    /// Reads `Store` directly from this store's own cursor; builds no row Vec.
    fn commit_rows(&mut self, u: &Universe, store: &Store) -> Result<usize, StoreError>;

    fn commit_tick(&mut self) -> Result<(), StoreError>;
    fn rollback_tick(&mut self) -> Result<(), StoreError>;
}

/// Kernel and prelude relations share one table: their cells are all arena
/// references and there are a hundred of them in an empty program.
pub fn kernel_owned(u: &Universe, rel: TermId) -> bool {
    let Some(inner) = u.unary(rel, "ref") else {
        return true;
    };
    if u.functor(inner).is_some_and(|(name, _)| name == "kernel") {
        return true;
    }
    match u.args::<2>(inner, "owner") {
        Some([owner, _]) => matches!(u.get(owner), Term::Atom(s) if u.sym_str(*s) == "prelude"),
        None => true,
    }
}
