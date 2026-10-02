//! Portable initial frontier and term dictionary for an IR program.

use super::ir_lower::Lowered;
use super::json::term_to_json;
use super::term::{TermId, Universe};
use ivm_ir as ir;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Deserialize, Serialize)]
pub struct AtomEntry {
    pub original: u32,
    pub printed: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Bundle {
    pub program: ir::Program,
    pub frontier0: ir::Frontier,
    pub atoms: BTreeMap<ir::Cell, AtomEntry>,
    pub texts: Vec<String>,
}

pub fn bundle(u: &Universe, lowered: &Lowered) -> Result<Bundle, String> {
    let mut program = lowered.ir.clone();
    program.texts = lowered.texts.iter().map(|(_, text)| text.clone()).collect();
    let mut interner = ir::Interner::default();
    if program.uses_strings() {
        interner.mint_text("");
    }
    for text in &program.texts {
        interner.mint_text(text);
    }
    let mut mapped = HashMap::<TermId, ir::Cell>::new();
    let mut atoms = BTreeMap::new();
    let text_cells: Vec<ir::Cell> = lowered
        .texts
        .iter()
        .map(|(_, text)| interner.mint_text(text))
        .collect();

    let mut specs = lowered.terms.iter().collect::<Vec<_>>();
    specs.sort_by_key(|spec| (spec.source, spec.original.0));
    // The generated runtime mints in prewarm stratum order. Reserve those IDs
    // before building rows because one constructor may refer to a later stratum.
    let first_cell = i64::try_from(interner.len()).map_err(|error| error.to_string())? + 1;
    for (index, spec) in specs.iter().enumerate() {
        mapped.insert(spec.original, first_cell + index as i64);
    }
    let mut changes = Vec::new();
    for spec in specs {
        let mut args = Vec::new();
        if let Some(raw) = spec.raw {
            args.push(raw);
        } else if let Some(text) = spec.text {
            args.push(text_cells[text]);
        } else {
            for child in &spec.args {
                args.push(*mapped.get(child).ok_or_else(|| {
                    format!("term {} before dependency {}", spec.original.0, child.0)
                })?);
            }
        }
        let constructor = program
            .rel(spec.functor)
            .ok_or_else(|| format!("constructor {} missing", spec.functor))?;
        let cell = interner.mint(&constructor.name, &args, &constructor.cols[1..]);
        if cell != mapped[&spec.original] {
            return Err(format!(
                "constructor cell order differs for term {}",
                spec.original.0
            ));
        }
        atoms.insert(
            cell,
            AtomEntry {
                original: spec.original.0,
                printed: term_to_json(u, spec.original),
            },
        );
        let mut row = vec![i64::from(spec.original.0)];
        row.extend(args);
        changes.push(ir::SourceChange {
            rel: spec.source,
            row,
            w: 1,
        });
    }

    changes.push(ir::SourceChange {
        rel: lowered.unit,
        row: vec![1],
        w: 1,
    });
    for (&original, &cell) in &mapped {
        changes.push(ir::SourceChange {
            rel: lowered.term_map,
            row: vec![i64::from(original.0), cell],
            w: 1,
        });
    }
    for (rel, seed) in &lowered.seeds {
        let row = if seed.args.is_empty() {
            vec![1]
        } else {
            seed.args
                .iter()
                .map(|term| {
                    mapped
                        .get(term)
                        .copied()
                        .ok_or_else(|| format!("seed term {} missing", term.0))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        changes.push(ir::SourceChange {
            rel: *rel,
            row,
            w: 1,
        });
    }
    for (rel, terms) in &lowered.internal_seeds {
        let row = terms
            .iter()
            .map(|term| {
                mapped
                    .get(term)
                    .copied()
                    .ok_or_else(|| format!("internal seed term {} missing", term.0))
            })
            .collect::<Result<Vec<_>, _>>()?;
        changes.push(ir::SourceChange {
            rel: *rel,
            row,
            w: 1,
        });
    }
    changes.sort_by(|a, b| (a.rel, &a.row).cmp(&(b.rel, &b.row)));
    changes.dedup_by(|a, b| a.rel == b.rel && a.row == b.row);
    Ok(Bundle {
        texts: program.texts.clone(),
        program,
        frontier0: ir::Frontier { changes },
        atoms,
    })
}
