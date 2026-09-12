//! `logical_program_calls/4,5` and `logical_program_rows_calls/5`. Port of
//! `0_logical_program_reifier.pl:42-188`.
//!
//! Each argument occurrence is an opaque node whose checked variable,
//! reference, literal or aggregate alternative is another ordinary relation
//! (`:38-41`).

use super::api::{Calls, Stop};
use super::rows::logical_program_rows_term;
use crate::_3_check::api::prolog_sort;
use crate::_6_eval::term::{Sym, Term, TermId, Universe};
use std::collections::HashMap;

/// Every public program relation the prelude published, keyed by the name it
/// published it under. v7 re-runs `findall/3` over the whole fact list once per
/// row (`:126-134`); one pass answers the same question with the same rows in
/// the same order.
pub struct PreludeIndex {
    pub by_name: HashMap<Sym, Vec<TermId>>,
}

/// `const(Inner)`.
fn constant(u: &Universe, term: TermId) -> Option<TermId> {
    u.unary(term, "const")
}

/// The four arguments of a `call(ref(kernel(':')), [_, _, _, _])` fact, or
/// `None` when the fact is any other shape.
pub fn colon_arguments(u: &Universe, fact: TermId) -> Option<Vec<TermId>> {
    let (name, parts) = u.functor(fact)?;
    if name != "call" || parts.len() != 2 {
        return None;
    }
    let kernel = u.unary(parts[0], "ref")?;
    let name = u.unary(kernel, "kernel")?;
    if !matches!(u.get(name), Term::Atom(s) if u.sym_str(*s) == ":") {
        return None;
    }
    let arguments = u.as_list(parts[1])?;
    if arguments.len() != 4 {
        return None;
    }
    Some(arguments)
}

impl PreludeIndex {
    /// `:125-134`, the pattern
    /// `call(ref(kernel(':')), [ref(module(prelude)), const(Name),
    /// ref(Relation), const(_)])`.
    pub fn build(u: &Universe, compiler_facts: &[TermId]) -> Self {
        let mut by_name: HashMap<Sym, Vec<TermId>> = HashMap::new();
        for fact in compiler_facts {
            let Some(arguments) = colon_arguments(u, *fact) else {
                continue;
            };
            let Some(module) = u
                .unary(arguments[0], "ref")
                .and_then(|m| u.unary(m, "module"))
            else {
                continue;
            };
            if !matches!(u.get(module), Term::Atom(s) if u.sym_str(*s) == "prelude") {
                continue;
            }
            let (Some(name), Some(relation), Some(_)) = (
                constant(u, arguments[1]),
                u.unary(arguments[2], "ref"),
                constant(u, arguments[3]),
            ) else {
                continue;
            };
            let Term::Atom(name) = u.get(name) else {
                continue;
            };
            by_name.entry(*name).or_default().push(relation);
        }
        for relations in by_name.values_mut() {
            relations.sort_by(|a, b| u.cmp(*a, *b));
            relations.dedup();
        }
        Self { by_name }
    }

    /// `:137-142`.
    pub fn resolve(&self, u: &mut Universe, name: Sym) -> Result<TermId, TermId> {
        let empty = Vec::new();
        let relations = self.by_name.get(&name).unwrap_or(&empty);
        match relations.len() {
            1 => Ok(relations[0]),
            0 => {
                let name = u.intern(Term::Atom(name));
                Err(u.compound("logical_program_protocol_missing", vec![name]))
            }
            _ => {
                let list = u.list(relations);
                let name = u.intern(Term::Atom(name));
                Err(u.compound("logical_program_protocol_ambiguous", vec![name, list]))
            }
        }
    }
}

/// `diagnostic(emit, none, Reason)`.
pub fn emit_diagnostic(u: &mut Universe, reason: TermId) -> TermId {
    let emit = u.atom("emit");
    let none = u.atom("none");
    u.compound("diagnostic", vec![emit, none, reason])
}

/// `:52-56`. `relations` is `None` for v7's atom `all` (`:86`).
pub fn logical_program_calls(
    u: &mut Universe,
    compiler_facts: &[TermId],
    checked: TermId,
    relations: Option<&[TermId]>,
) -> Result<Calls, Stop> {
    let rows = logical_program_rows_term(u, checked)?;
    logical_program_rows_calls(u, compiler_facts, &rows, relations)
}

/// `:63-84`. Calls keep row order; diagnostics are sorted.
pub fn logical_program_rows_calls(
    u: &mut Universe,
    compiler_facts: &[TermId],
    rows: &[TermId],
    relations: Option<&[TermId]>,
) -> Result<Calls, Stop> {
    let index = PreludeIndex::build(u, compiler_facts);
    let mut calls = Vec::new();
    let mut diagnostics = Vec::new();
    for row in rows {
        let Term::Compound(name, _) = *u.get(*row) else {
            return Err(Stop::Fail("logical program row expected"));
        };
        match index.resolve(u, name) {
            Ok(relation) => {
                if relations.is_some_and(|wanted| !wanted.contains(&relation)) {
                    continue;
                }
                let arguments = logical_row_arguments(u, *row)?;
                let arguments = u.list(&arguments);
                let target = u.compound("ref", vec![relation]);
                calls.push(u.compound("call", vec![target, arguments]));
            }
            Err(reason) => diagnostics.push(emit_diagnostic(u, reason)),
        }
    }
    Ok(Calls {
        calls,
        diagnostics: prolog_sort(u, diagnostics),
    })
}

/// `:188`.
fn logical_identity(u: &mut Universe, identity: TermId) -> TermId {
    let wrapped = u.compound("logical_program", vec![identity]);
    u.compound("ref", vec![wrapped])
}

/// `:113-116`. A `ref` target is wrapped, a `const` target passes through.
fn logical_argument_target(u: &mut Universe, target: TermId) -> TermId {
    match u.unary(target, "ref") {
        Some(identity) => logical_identity(u, identity),
        None => target,
    }
}

/// `ref(Relation)`.
fn relation_reference(u: &mut Universe, relation: TermId) -> TermId {
    u.compound("ref", vec![relation])
}

/// `const(Value)`.
fn constant_of(u: &mut Universe, value: TermId) -> TermId {
    u.compound("const", vec![value])
}

/// `atom_string/2` then `const/1`: the text of an atom as a Prolog string.
fn constant_text(u: &mut Universe, atom: TermId) -> Result<TermId, Stop> {
    let Term::Atom(s) = u.get(atom) else {
        return Err(Stop::Fail("atom expected"));
    };
    let text = u.sym_str(*s).to_string();
    let text = u.string(&text);
    Ok(constant_of(u, text))
}

/// `:144-186`, one arm per reified row functor.
pub fn logical_row_arguments(u: &mut Universe, row: TermId) -> Result<Vec<TermId>, Stop> {
    let Some((name, args)) = u.functor(row).map(|(n, a)| (n.to_string(), a.to_vec())) else {
        return Err(Stop::Fail("logical program row expected"));
    };
    let arguments = match (name.as_str(), args.len()) {
        ("program_relation", 2) | ("program_key", 2) => {
            let relation = relation_reference(u, args[0]);
            vec![relation, constant_of(u, args[1])]
        }
        ("program_key_position", 3) => {
            let relation = relation_reference(u, args[0]);
            let ordinal = constant_of(u, args[1]);
            vec![relation, ordinal, constant_of(u, args[2])]
        }
        ("program_seed", 2) | ("program_rule", 2) => {
            let owner = logical_identity(u, args[0]);
            vec![owner, logical_identity(u, args[1])]
        }
        ("program_rule_kind", 2) => {
            let rule = logical_identity(u, args[0]);
            vec![rule, constant_text(u, args[1])?]
        }
        ("program_goal", 4) => {
            let rule = logical_identity(u, args[0]);
            let position = constant_of(u, args[1]);
            let polarity = constant_text(u, args[2])?;
            vec![rule, position, polarity, logical_identity(u, args[3])]
        }
        ("program_apply", 2) => {
            let call = logical_identity(u, args[0]);
            vec![call, relation_reference(u, args[1])]
        }
        ("program_argument", 3) => {
            let call = logical_identity(u, args[0]);
            let position = constant_of(u, args[1]);
            vec![call, position, logical_identity(u, args[2])]
        }
        ("program_edge", 4) => {
            let owner = logical_identity(u, args[0]);
            let label = constant_of(u, args[1]);
            let target = logical_argument_target(u, args[2]);
            vec![owner, label, target, constant_of(u, args[3])]
        }
        ("program_dependency", 3) => {
            let head = relation_reference(u, args[0]);
            let body = relation_reference(u, args[1]);
            vec![head, body, constant_text(u, args[2])?]
        }
        ("program_stratum", 2) => {
            let relation = relation_reference(u, args[0]);
            vec![relation, constant_of(u, args[1])]
        }
        _ => return Err(Stop::Fail("logical program row expected")),
    };
    Ok(arguments)
}
