//! The reverse door: decode a foreign TSI stream, validate it against the
//! registry, canonicalize its ids and ordinals, re-emit in sorted order.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::registry::check;
use super::types::{Arg, FactOut, Method, WitnessOut, PROTOCOL_VERSION};
use crate::types::FlatFact;

/// Sorting and first-appearance numbering feed each other, so the renumber
/// runs to a fixpoint. The cap bounds a stream that never settles.
const RENUMBER_PASSES: usize = 16;

/// One variant per step of the door. Every one names the line it stopped on,
/// except coverage, which is a claim about a whole run.
#[derive(Debug)]
pub enum IngestError {
    Decode {
        line: usize,
        detail: String,
    },
    Relation {
        line: usize,
        relation: String,
        detail: String,
    },
    Dangling {
        line: usize,
        id: u32,
    },
    Coverage {
        run: u32,
        relation: String,
    },
}

impl fmt::Display for IngestError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestError::Decode { line, detail } => write!(out, "line {line}: {detail}"),
            IngestError::Relation {
                line,
                relation,
                detail,
            } => write!(out, "line {line}: {relation}: {detail}"),
            IngestError::Dangling { line, id } => write!(
                out,
                "line {line}: id {id} is declared by no tsi.type, tsi.symbol, tsi.value, tsi.edge, rust.impl or tsi.called row"
            ),
            IngestError::Coverage { run, relation } => write!(
                out,
                "run {run}: coverage complete for {relation} with no fact row"
            ),
        }
    }
}

impl std::error::Error for IngestError {}

/// Lines are numbered from 1 in the order they arrive, which is the
/// concatenation order when the caller reads several files.
pub fn ingest(lines: impl Iterator<Item = String>) -> Result<Vec<String>, IngestError> {
    let (mut rows, line_of) = decode(lines)?;
    id_closure(&rows, &line_of)?;
    coverage(&rows)?;
    renumber_ids(&mut rows);
    renumber_ordinals(&mut rows);
    witness_foreign(&mut rows);
    let protocol = serde_json::to_string(&FlatFact::Protocol {
        version: PROTOCOL_VERSION,
    })
    .expect("a flat fact is serializable");
    let mut out = vec![protocol];
    out.extend(crate::project::sorted_lines(rows));
    Ok(out)
}

/// Step 0 and step 1. The protocol row is dropped here and minted again on the
/// way out, so a stream concatenated from several files carries one.
fn decode(lines: impl Iterator<Item = String>) -> Result<(Vec<FlatFact>, Vec<usize>), IngestError> {
    let mut rows = Vec::new();
    let mut line_of = Vec::new();
    for (index, text) in lines.enumerate() {
        let line = index + 1;
        if text.trim().is_empty() {
            continue;
        }
        let row: FlatFact = serde_json::from_str(&text).map_err(|error| IngestError::Decode {
            line,
            detail: error.to_string(),
        })?;
        match &row {
            FlatFact::Protocol { version } => {
                if *version != PROTOCOL_VERSION {
                    return Err(IngestError::Decode {
                        line,
                        detail: format!(
                            "protocol version {version}, this door speaks {PROTOCOL_VERSION}"
                        ),
                    });
                }
                continue;
            }
            FlatFact::Fact(fact) => {
                check(&fact.relation, &fact.args).map_err(|detail| IngestError::Relation {
                    line,
                    relation: fact.relation.clone(),
                    detail,
                })?;
            }
            _ => {}
        }
        rows.push(row);
        line_of.push(line);
    }
    Ok((rows, line_of))
}

/// Step 2. Declaring positions are `tsi.type` 0, `tsi.symbol` 0, `tsi.value` 0,
/// `tsi.edge` 0, `rust.impl` 0 and `tsi.called` 2, so a recursive type closes through ids and
/// the check is one pass.
fn id_closure(rows: &[FlatFact], line_of: &[usize]) -> Result<(), IngestError> {
    let mut declared: BTreeSet<u32> = BTreeSet::new();
    for fact in facts(rows) {
        let position = match fact.relation.as_str() {
            "tsi.type" | "tsi.symbol" | "tsi.value" | "tsi.edge" | "rust.impl" => 0,
            "tsi.called" => 2,
            _ => continue,
        };
        if let Some(Arg::Id(id)) = fact.args.get(position) {
            declared.insert(*id);
        }
    }
    for (row, line) in rows.iter().zip(line_of) {
        let FlatFact::Fact(fact) = row else { continue };
        for arg in &fact.args {
            if let Arg::Id(id) = arg {
                if !declared.contains(id) {
                    return Err(IngestError::Dangling {
                        line: *line,
                        id: *id,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Step 3. `complete` claims every reachable row was emitted, so a complete
/// row with nothing to cover is a producer defect, never an empty relation.
fn coverage(rows: &[FlatFact]) -> Result<(), IngestError> {
    for row in rows {
        let FlatFact::Coverage(claim) = row else {
            continue;
        };
        if !claim.complete {
            continue;
        }
        if !facts(rows).any(|fact| fact.relation == claim.relation) {
            return Err(IngestError::Coverage {
                run: claim.run,
                relation: claim.relation.clone(),
            });
        }
    }
    Ok(())
}

fn facts(rows: &[FlatFact]) -> impl Iterator<Item = &FactOut> {
    rows.iter().filter_map(|row| match row {
        FlatFact::Fact(fact) => Some(fact),
        _ => None,
    })
}

/// Step 4, the id half: ids take first-appearance order over the fact rows in
/// canonical order, which is an order the ids themselves decide.
fn renumber_ids(rows: &mut [FlatFact]) {
    let indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| matches!(row, FlatFact::Fact(_)))
        .map(|(index, _)| index)
        .collect();
    for _ in 0..RENUMBER_PASSES {
        let mut order = indices.clone();
        order.sort_by(|left, right| compare_facts(fact_at(rows, *left), fact_at(rows, *right)));
        let mut map: BTreeMap<u32, u32> = BTreeMap::new();
        for index in &order {
            for arg in &fact_at(rows, *index).args {
                if let Arg::Id(id) = arg {
                    if !map.contains_key(id) {
                        let next = map.len() as u32;
                        map.insert(*id, next);
                    }
                }
            }
        }
        if map.iter().all(|(old, new)| old == new) {
            return;
        }
        for index in &indices {
            let FlatFact::Fact(fact) = &mut rows[*index] else {
                continue;
            };
            for arg in &mut fact.args {
                if let Arg::Id(id) = arg {
                    *id = map[id];
                }
            }
        }
    }
}

fn fact_at(rows: &[FlatFact], index: usize) -> &FactOut {
    match &rows[index] {
        FlatFact::Fact(fact) => fact,
        _ => unreachable!("the index came from a fact row"),
    }
}

/// Canonical fact order: relation, then arguments. The ordinal is excluded, so
/// the order does not move when the ordinals are reassigned.
fn compare_facts(left: &FactOut, right: &FactOut) -> Ordering {
    left.relation
        .cmp(&right.relation)
        .then_with(|| compare_args(&left.args, &right.args))
}

fn compare_args(left: &[Arg], right: &[Arg]) -> Ordering {
    for (one, other) in left.iter().zip(right) {
        let step = compare_arg(one, other);
        if step != Ordering::Equal {
            return step;
        }
    }
    left.len().cmp(&right.len())
}

/// Ids compare as numbers, never as their JSON spelling, so `{"id":9}` sorts
/// before `{"id":40}`.
fn compare_arg(left: &Arg, right: &Arg) -> Ordering {
    fn rank(arg: &Arg) -> u8 {
        match arg {
            Arg::Id(_) => 0,
            Arg::Span(_, _, _) => 1,
            Arg::Text(_) => 2,
            Arg::Int(_) => 3,
            Arg::Atom(_) => 4,
        }
    }
    match (left, right) {
        (Arg::Id(one), Arg::Id(other)) => one.cmp(other),
        (Arg::Span(digest, start, end), Arg::Span(other_digest, other_start, other_end)) => digest
            .cmp(other_digest)
            .then(start.cmp(other_start))
            .then(end.cmp(other_end)),
        (Arg::Text(one), Arg::Text(other)) | (Arg::Atom(one), Arg::Atom(other)) => one.cmp(other),
        (Arg::Int(one), Arg::Int(other)) => one.cmp(other),
        _ => rank(left).cmp(&rank(right)),
    }
}

/// Step 4, the ordinal half. The sort key is the row with its ordinal erased,
/// so the assignment is a function of content alone and re-ingest repeats it.
fn renumber_ordinals(rows: &mut [FlatFact]) {
    let mut keyed: Vec<(String, usize)> = Vec::new();
    for (index, row) in rows.iter_mut().enumerate() {
        let Some(held) = ordinal(row) else {
            continue;
        };
        clear_ordinal(row);
        let key = serde_json::to_string(&row).expect("a flat fact is serializable");
        set_ordinal(row, held);
        keyed.push((key, index));
    }
    keyed.sort();
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    for (next, (_, index)) in keyed.iter().enumerate() {
        if let Some(old) = ordinal(&mut rows[*index]) {
            map.insert(old, next as u32);
        }
    }
    for (next, (_, index)) in keyed.iter().enumerate() {
        set_ordinal(&mut rows[*index], next as u32);
    }
    for row in rows.iter_mut() {
        if let FlatFact::Witness(witness) = row {
            if let Some(new) = map.get(&witness.fact) {
                witness.fact = *new;
            }
        }
    }
}

fn ordinal(row: &mut FlatFact) -> Option<u32> {
    match row {
        FlatFact::Fact(fact) => Some(fact.fact),
        other => other.fact_slot().and_then(|slot| *slot),
    }
}

fn set_ordinal(row: &mut FlatFact, value: u32) {
    match row {
        FlatFact::Fact(fact) => fact.fact = value,
        other => {
            if let Some(slot) = other.fact_slot() {
                *slot = Some(value);
            }
        }
    }
}

fn clear_ordinal(row: &mut FlatFact) {
    match row {
        FlatFact::Fact(fact) => fact.fact = 0,
        other => {
            if let Some(slot) = other.fact_slot() {
                *slot = None;
            }
        }
    }
}

/// Step 5. The door produces no row, so it mints no run: the foreign witness is
/// filed under the run the row's own stream already declared.
fn witness_foreign(rows: &mut Vec<FlatFact>) {
    let fallback = rows
        .iter()
        .filter_map(|row| match row {
            FlatFact::Run(run) => Some(run.run),
            _ => None,
        })
        .min()
        .unwrap_or(0);
    let mut run_of: BTreeMap<u32, u32> = BTreeMap::new();
    let mut already: BTreeSet<(u32, u32)> = BTreeSet::new();
    for row in rows.iter() {
        if let FlatFact::Witness(witness) = row {
            run_of
                .entry(witness.fact)
                .and_modify(|run| *run = (*run).min(witness.run))
                .or_insert(witness.run);
            if witness.method == Method::Foreign {
                already.insert((witness.fact, witness.run));
            }
        }
    }
    let minted: Vec<FlatFact> = facts(rows)
        .filter_map(|fact| {
            let run = run_of.get(&fact.fact).copied().unwrap_or(fallback);
            (!already.contains(&(fact.fact, run))).then_some(FlatFact::Witness(WitnessOut {
                fact: fact.fact,
                run,
                method: Method::Foreign,
            }))
        })
        .collect();
    rows.extend(minted);
}
