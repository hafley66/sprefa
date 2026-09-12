//! `validate_functional_rows/3`. Port of `v7/src/1_libtime/0_evaluator.pl:557`
//! and `:580-646`.
//!
//! This predicate lives in v7's evaluator and the v8 `_6_eval` port stops at
//! `evaluate/4`. It moves to `_6_eval` when a lane owns that file; the emitter
//! at `1_artifact_emitter.pl:189` is its only caller.

use crate::_3_check::api::prolog_sort;
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashMap;

/// `call(Relation, Arguments)`.
fn call_parts(u: &Universe, row: TermId) -> Option<(TermId, Vec<TermId>)> {
    let (name, parts) = u.functor(row)?;
    if name != "call" || parts.len() != 2 {
        return None;
    }
    let (relation, arguments) = (parts[0], parts[1]);
    Some((relation, u.as_list(arguments)?))
}

/// `:557-568`. Complete-row set identity is already enforced by `evaluate/4`
/// sorting its output, so a relation with no declared keys needs no check.
pub fn validate_functional_rows(
    u: &mut Universe,
    relations: &[TermId],
    rows: &[TermId],
) -> Vec<TermId> {
    let sorted = prolog_sort(u, rows.to_vec());

    // :598-604. One pass, so each relation's key validation reads only its own
    // rows instead of rescanning the whole closure.
    let mut by_relation: HashMap<TermId, Vec<TermId>> = HashMap::new();
    for row in &sorted {
        if let Some((relation, _)) = call_parts(u, *row) {
            by_relation.entry(relation).or_default().push(*row);
        }
    }

    let mut diagnostics = Vec::new();
    for declaration in relations {
        let Some(("relation", args)) = u.functor(*declaration) else {
            continue;
        };
        if args.len() != 3 {
            continue;
        }
        let (relation, key_sets) = (args[0], args[2]);
        let Some(key_sets) = u.as_list(key_sets) else {
            continue;
        };
        let relation_rows = by_relation.get(&relation).cloned().unwrap_or_default();
        for key_set in key_sets {
            let Some(positions) = u.as_list(key_set) else {
                continue;
            };
            key_set_diagnostics(
                u,
                relation,
                key_set,
                &positions,
                &relation_rows,
                &mut diagnostics,
            );
        }
    }
    prolog_sort(u, diagnostics)
}

/// `:613-642`. Group by key once, then compare inside a group only, keeping
/// row order so the `Left`/`Right` orientation is v7's.
fn key_set_diagnostics(
    u: &mut Universe,
    relation: TermId,
    key_set: TermId,
    positions: &[TermId],
    rows: &[TermId],
    out: &mut Vec<TermId>,
) {
    let mut groups: Vec<(TermId, Vec<TermId>)> = Vec::new();
    for row in rows {
        let Some(key) = key_values(u, *row, positions) else {
            continue;
        };
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, group)) => group.push(*row),
            None => groups.push((key, vec![*row])),
        }
    }
    for (values, group) in groups {
        for (offset, left) in group.iter().enumerate() {
            for right in &group[offset + 1..] {
                let reason = u.compound(
                    "functional_key_conflict",
                    vec![relation, key_set, values, *left, *right],
                );
                let evaluate = u.atom("evaluate");
                let none = u.atom("none");
                out.push(u.compound("diagnostic", vec![evaluate, none, reason]));
            }
        }
    }
}

/// `:644-646`. `nth0/3` fails on a short row, which drops that row from the
/// group set.
fn key_values(u: &mut Universe, row: TermId, positions: &[TermId]) -> Option<TermId> {
    let (_, arguments) = call_parts(u, row)?;
    let mut values = Vec::with_capacity(positions.len());
    for position in positions {
        let index = u.as_int(*position)?;
        values.push(*arguments.get(usize::try_from(index).ok()?)?);
    }
    Some(u.list(&values))
}
