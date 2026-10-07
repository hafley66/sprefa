//! The per-process keep of exporter lowerings. One entry per interned
//! `(unit, policy, environment, imports)` tuple; a repeat compile takes the
//! kept `module_basement`, `module_origins` and importer environment instead
//! of re-running the lowerer on the exporter.

use super::cx::CallPolicy;
use crate::_6_eval::term::{TermId, Universe};
use std::cell::RefCell;

/// The kept lowering of one unit under one input key.
#[derive(Clone)]
pub struct UnitLowered {
    /// `module_basement(owner, program)`.
    pub basement: TermId,
    /// `module_origins(owner, list)`.
    pub origins: TermId,
    /// `module_expression_environment` for the importers. `None` when the
    /// lowering ended in diagnostics, which returns before the environment
    /// is built, or when the entry came from the flat path.
    pub exports: Option<TermId>,
    pub diagnostics: Vec<TermId>,
}

#[derive(Clone)]
struct Entry {
    key: Key,
    lowered: UnitLowered,
}

#[derive(Clone, PartialEq, Eq)]
struct Key {
    unit: TermId,
    policy: CallPolicy,
    environment: TermId,
    imports: Vec<TermId>,
}

/// Terms are interned in an index set, so equal structure is equal `TermId`
/// and key equality is structural.
#[derive(Default)]
pub struct LowerKeep {
    entries: RefCell<Vec<Entry>>,
}

impl LowerKeep {
    fn key(policy: CallPolicy, unit: TermId, environment: TermId, imports: &[TermId]) -> Key {
        Key {
            unit,
            policy,
            environment,
            imports: imports.to_vec(),
        }
    }

    /// The kept lowering of `unit`, or `None`. A miss beside an entry with the
    /// same unit and policy reports which input component differs.
    pub fn exporter(
        &self,
        u: &Universe,
        policy: CallPolicy,
        unit: TermId,
        environment: TermId,
        imports: &[TermId],
    ) -> Option<UnitLowered> {
        let entries = self.entries.borrow();
        let sought = Self::key(policy, unit, environment, imports);
        if let Some(entry) = entries.iter().find(|entry| entry.key == sought) {
            let owner = u
                .args::<2>(entry.lowered.basement, "module_basement")
                .map(|parts| parts[0])
                .unwrap_or(entry.lowered.basement);
            tracing::info!(target: "dl8::a1", unit = %u.display(owner), "lower_hit");
            return Some(entry.lowered.clone());
        }
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.key.unit == unit && entry.key.policy == policy)
        {
            let component = if entry.key.imports != sought.imports {
                "imports"
            } else {
                "environment"
            };
            tracing::info!(
                target: "dl8::a1",
                unit = %u.display(unit),
                differs = component,
                "lower_key_differs"
            );
        }
        None
    }

    pub fn insert(
        &self,
        policy: CallPolicy,
        unit: TermId,
        environment: TermId,
        imports: &[TermId],
        lowered: UnitLowered,
    ) {
        let key = Self::key(policy, unit, environment, imports);
        let mut entries = self.entries.borrow_mut();
        if !entries.iter().any(|entry| entry.key == key) {
            entries.push(Entry { key, lowered });
        }
    }
}
