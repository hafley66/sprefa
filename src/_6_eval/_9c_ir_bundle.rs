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

/// The generated runtime has no `intern_terms`: per constructor, in `Lowered::constructors` order,
/// a source of `(TermId, args)` rows and a Let minting them, ahead of every other stratum. The
/// constructor's `(stratum order, source)`.
fn with_prewarm(program: &mut ir::Program, lowered: &Lowered) -> Result<HashMap<ir::RelId, (usize, ir::RelId)>, String> {
    let mut next = program.rels.iter().map(|rel| rel.id + 1).max().unwrap_or(0);
    let mut prewarm = HashMap::new();
    let mut strata = Vec::new();
    let mut outputs = Vec::new();
    for (order, &ctor) in lowered.constructors.values().enumerate() {
        let args = program.rel(ctor).ok_or_else(|| format!("constructor {ctor} missing"))?.cols[1..].to_vec();
        let mut source_cols = vec![ir::Ty::Int];
        source_cols.extend(args.iter().copied());
        let mut output_cols = source_cols.clone();
        output_cols.push(ir::Ty::Id);
        let (source, output) = (next, next + 1);
        next += 2;
        program.rels.push(ir::Relation { id: source, name: format!("__ir_prewarm_{ctor}"), cols: source_cols, kind: ir::RelKind::Source });
        program.rels.push(ir::Relation { id: output, name: format!("__ir_prewarm_out_{ctor}"), cols: output_cols, kind: ir::RelKind::Derived });
        let get = program.nodes.len() as ir::NodeId;
        program.nodes.push(ir::Op::Get(source));
        let mint = program.nodes.len() as ir::NodeId;
        program.nodes.push(ir::Op::Mint {
            input: get,
            functor: ctor,
            args: (1..=args.len()).map(|col| col as ir::ColId).collect(),
        });
        strata.push(ir::Stratum::Let { id: output, body: mint });
        outputs.push(output);
        prewarm.insert(ctor, (order, source));
    }
    program.strata.splice(0..0, strata);
    program.outputs.splice(0..0, outputs);
    Ok(prewarm)
}

pub fn bundle(u: &Universe, lowered: &Lowered) -> Result<Bundle, String> {
    let mut program = lowered.ir.clone();
    program.texts = lowered.texts.iter().map(|(_, text)| text.clone()).collect();
    let prewarm = with_prewarm(&mut program, lowered)?;
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
    specs.sort_by_key(|spec| (prewarm.get(&spec.functor).map(|(order, _)| *order), spec.original.0));
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
            rel: prewarm.get(&spec.functor).ok_or_else(|| format!("constructor {} missing", spec.functor))?.1,
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
        let raw = lowered.raw.get(rel);
        let row = if seed.args.is_empty() {
            vec![1]
        } else {
            seed.args
                .iter()
                .enumerate()
                .map(|(column, term)| match raw.is_some_and(|raw| raw[column]) {
                    true => Ok(lowered.seed_ints[term]),
                    false => mapped
                        .get(term)
                        .copied()
                        .ok_or_else(|| format!("seed term {} missing", term.0)),
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
    // The generated runtime has no term table: each term literal becomes its reserved cell.
    let cells = lowered
        .literals
        .iter()
        .map(|term| mapped.get(term).copied().ok_or_else(|| format!("literal term {} missing", term.0)))
        .collect::<Result<Vec<_>, _>>()?;
    for op in &mut program.nodes {
        if let ir::Op::Mfp { filter, map, .. } = op {
            for expr in filter.iter_mut().chain(map.iter_mut()) {
                *expr = with_literal_cells(expr, &cells);
            }
        }
    }
    program.terms.clear();
    Ok(Bundle {
        texts: program.texts.clone(),
        program,
        frontier0: ir::Frontier { changes },
        atoms,
    })
}

/// `expr` with each `Term(i)` replaced by `Lit(cells[i])`: post-order over an explicit stack.
fn with_literal_cells(expr: &ir::Expr, cells: &[ir::Cell]) -> ir::Expr {
    enum Step<'e> {
        Enter(&'e ir::Expr),
        Build(ir::Func, usize),
    }
    let mut steps = vec![Step::Enter(expr)];
    let mut done: Vec<ir::Expr> = Vec::new();
    while let Some(step) = steps.pop() {
        match step {
            Step::Enter(ir::Expr::Term(index)) => done.push(ir::Expr::Lit(cells[*index as usize])),
            Step::Enter(ir::Expr::Call(func, args)) => {
                steps.push(Step::Build(*func, args.len()));
                steps.extend(args.iter().rev().map(Step::Enter));
            }
            Step::Enter(other) => done.push(other.clone()),
            Step::Build(func, arity) => {
                let args = done.split_off(done.len() - arity);
                done.push(ir::Expr::Call(func, args));
            }
        }
    }
    done.pop().expect("one expression")
}
