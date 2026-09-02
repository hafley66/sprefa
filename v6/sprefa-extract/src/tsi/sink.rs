//! What every adapter writes into: fresh ids, fact rows and their witness,
//! coverage per relation. The registry check is a `debug_assert!`.

use super::registry::check;
use super::types::{Arg, CoverageOut, FactOut, Method, WitnessOut};
use crate::types::FlatFact;

/// Ordinals start at 0 and are run-local; `--ingest` renumbers them.
pub struct TsiSink {
    ids: u32,
    facts: Vec<FactOut>,
    witnesses: Vec<WitnessOut>,
    coverage: Vec<CoverageOut>,
    run: u32,
    method: Method,
}

impl TsiSink {
    pub fn new(run: u32, method: Method) -> Self {
        Self {
            ids: 0,
            facts: Vec::new(),
            witnesses: Vec::new(),
            coverage: Vec::new(),
            run,
            method,
        }
    }

    pub fn fresh_id(&mut self) -> u32 {
        let id = self.ids;
        self.ids += 1;
        id
    }

    /// Returns the fact ordinal, which is what a later witness names.
    pub fn fact(&mut self, relation: &'static str, args: Vec<Arg>) -> u32 {
        debug_assert!(
            check(relation, &args).is_ok(),
            "{relation}: {}",
            check(relation, &args).unwrap_err()
        );
        let fact = self.facts.len() as u32;
        self.facts.push(FactOut {
            fact,
            relation: relation.to_string(),
            args,
        });
        self.witnesses.push(WitnessOut {
            fact,
            run: self.run,
            method: self.method,
        });
        fact
    }

    /// Claims every reachable row of the relation was emitted, so absence from
    /// it is meaningful. A syntax run never calls this.
    pub fn complete(&mut self, relation: &'static str) {
        self.cover(relation, true);
    }

    pub fn partial(&mut self, relation: &'static str) {
        self.cover(relation, false);
    }

    fn cover(&mut self, relation: &'static str, complete: bool) {
        debug_assert!(
            super::registry::relation(relation).is_some(),
            "{relation}: not in registry"
        );
        self.coverage.push(CoverageOut {
            run: self.run,
            relation: relation.to_string(),
            complete,
        });
    }

    /// Facts, then their witnesses, then coverage: the order the wire reads in.
    pub fn rows(self) -> Vec<FlatFact> {
        let mut rows: Vec<FlatFact> = self.facts.into_iter().map(FlatFact::Fact).collect();
        rows.extend(self.witnesses.into_iter().map(FlatFact::Witness));
        rows.extend(self.coverage.into_iter().map(FlatFact::Coverage));
        rows
    }
}
