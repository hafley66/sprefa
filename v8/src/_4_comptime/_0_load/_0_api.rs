//! Result shapes and the term helpers every loader file uses.

use crate::_6_eval::term::{Term, TermId, Universe};

/// `load_tsi_stream/3`, `load_tsi_text/3`, `load_source_fact_files/3`.
pub struct Loaded {
    pub rows: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `install_project_graph/6`, `install_tsi_graph/6`,
/// `install_source_fact_graph/6`. Basements and origins are the whole output
/// list, which v7 builds by consing onto the caller's list.
pub struct Installed {
    pub basements: Vec<TermId>,
    pub origins: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `sort/2`: SWI standard order, duplicates removed. `TermId` equality is
/// exact because the arena is hash consed.
pub fn sorted(u: &Universe, mut terms: Vec<TermId>) -> Vec<TermId> {
    terms.sort_by(|a, b| u.cmp(*a, *b));
    terms.dedup();
    terms
}

/// The arguments of `name/arity`, copied so the caller may mint terms.
pub fn parts(u: &Universe, term: TermId, name: &str, arity: usize) -> Option<Vec<TermId>> {
    let (found, args) = u.functor(term)?;
    if found != name || args.len() != arity {
        return None;
    }
    Some(args.to_vec())
}

pub fn atom_text(u: &Universe, term: TermId) -> Option<&str> {
    match u.get(term) {
        Term::Atom(s) => Some(u.sym_str(*s)),
        _ => None,
    }
}

pub fn string_text(u: &Universe, term: TermId) -> Option<&str> {
    match u.get(term) {
        Term::Str(s) => Some(u.sym_str(*s)),
        _ => None,
    }
}

/// `id(Id)` with an integer payload.
pub fn wire_id(u: &Universe, term: TermId) -> Option<i64> {
    u.as_int(u.unary(term, "id")?)
}

pub fn diagnostic(u: &mut Universe, phase: &str, subject: TermId, payload: TermId) -> TermId {
    let phase = u.atom(phase);
    u.compound("diagnostic", vec![phase, subject, payload])
}

/// `next_owner_index/4`: the run restarts whenever the owner changes from the
/// claim before it, which is well defined only over a list sorted by owner.
pub fn owner_index(owner: TermId, previous: &mut Option<TermId>, index: &mut i64) -> i64 {
    if *previous == Some(owner) {
        *index += 1;
    } else {
        *index = 0;
        *previous = Some(owner);
    }
    *index
}
