//! The semantic tier's rows on the wire: a native checker enumerates whole
//! relations, so its facts arrive as a block rather than one answer per site.

use super::types::{Arg, CoverageOut, DiagnosticOut, FactOut, Method, WitnessOut};
use crate::types::FlatFact;

/// `complete` claims every reachable row of the relation was emitted. A partial
/// claim carries the sentence that says what was left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageClaim {
    pub relation: String,
    pub complete: bool,
    pub diagnostic: Option<String>,
}

/// What a checker tier hands the envelope. Ids inside `facts` are run-local and
/// already closed: an argument names an id some other row declares.
pub trait SemanticRows {
    fn facts(&self) -> &[FactOut];
    fn coverage(&self) -> &[CoverageClaim];
}

/// Append one run's rows to a stream that already numbered its own. Ordinals
/// continue the stream's, so a witness names exactly one fact. `ids` is the
/// first id free in the stream; the returned one is free after these rows.
pub fn emit_semantic(
    run: u32,
    rows: &dyn SemanticRows,
    ids: u32,
    out: &mut Vec<FlatFact>,
) -> u32 {
    let span = crate::trace::phase_span("-", crate::trace::Phase::TsiSemantic);
    let _entered = span.enter();
    crate::trace::record_phase(
        &span,
        0,
        rows.facts().len() as u64,
        rows.coverage().len() as u64,
    );
    let base = highest_ordinal(out);
    let mut next_id = ids;
    let mut witnesses: Vec<FlatFact> = Vec::with_capacity(rows.facts().len());
    for (offset, fact) in rows.facts().iter().enumerate() {
        let ordinal = base + 1 + offset as u32;
        let mut fact = fact.clone();
        fact.fact = ordinal;
        // Every tier numbers its own ids from 0, so two tiers in one stream
        // would give one number to two types.
        for arg in &mut fact.args {
            if let Arg::Id(id) = arg {
                *id += ids;
                next_id = next_id.max(*id + 1);
            }
        }
        out.push(FlatFact::Fact(fact));
        witnesses.push(FlatFact::Witness(WitnessOut {
            fact: ordinal,
            run,
            method: Method::CheckerWalk,
        }));
    }
    out.extend(witnesses);
    for claim in rows.coverage() {
        out.push(FlatFact::Coverage(CoverageOut {
            run,
            relation: claim.relation.clone(),
            complete: claim.complete,
        }));
        if claim.complete {
            continue;
        }
        if let Some(detail) = &claim.diagnostic {
            out.push(FlatFact::Diagnostic(DiagnosticOut {
                run,
                relation: claim.relation.clone(),
                detail: detail.clone(),
            }));
        }
    }
    next_id
}

/// The stream numbers a TSI `fact` row in a required field and every other row
/// in an optional slot, so both are read to find where the numbering stopped.
fn highest_ordinal(rows: &mut [FlatFact]) -> u32 {
    let mut highest = 0;
    for row in rows.iter_mut() {
        let ordinal = match row {
            FlatFact::Fact(fact) => Some(fact.fact),
            other => other.fact_slot().and_then(|slot| *slot),
        };
        highest = highest.max(ordinal.unwrap_or(0));
    }
    highest
}
