use super::cursor::Cursor;
use super::diag::Diags;
use super::lower::{OpAction, TermMode};
use super::store::Store;
use std::sync::Arc;

pub mod fs;
pub mod re;
pub mod tag;
pub mod tags;

pub struct OpCtx<'a> {
    pub store: &'a mut Store,
    pub diags: &'a mut Diags,
}

pub fn resolve_term<'a>(cursor: &'a Cursor, mode: &'a TermMode) -> Option<&'a super::lower::Scalar> {
    match mode {
        TermMode::Bound(v) => Some(v),
        TermMode::Unbound => None,
    }
}

pub fn term_to_string(mode: &TermMode) -> Option<String> {
    match mode {
        TermMode::Bound(super::lower::Scalar::String(s)) => Some(s.to_string()),
        TermMode::Bound(super::lower::Scalar::Atom(s)) => Some(s.to_string()),
        TermMode::Bound(super::lower::Scalar::Int(i)) => Some(i.to_string()),
        _ => None,
    }
}
