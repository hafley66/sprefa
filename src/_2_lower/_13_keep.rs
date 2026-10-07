//! The per-process keep of unit lowerings. One entry per interned
//! `(unit, policy, environment, imports)` tuple, where `imports` is the
//! unit's own `import` rows; a repeat compile takes the kept
//! `module_basement`, `module_origins` and importer environment instead
//! of re-running the lowerer on the unit.

use super::api::unit_parts;
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
    fn key(
        u: &Universe,
        policy: CallPolicy,
        unit: TermId,
        environment: TermId,
        imports: &[TermId],
    ) -> Key {
        Key {
            unit,
            policy,
            environment,
            imports: read_imports(u, unit, imports),
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
        let sought = Self::key(u, policy, unit, environment, imports);
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
        u: &Universe,
        policy: CallPolicy,
        unit: TermId,
        environment: TermId,
        imports: &[TermId],
        lowered: UnitLowered,
    ) {
        let key = Self::key(u, policy, unit, environment, imports);
        let mut entries = self.entries.borrow_mut();
        if !entries.iter().any(|entry| entry.key == key) {
            entries.push(Entry { key, lowered });
        }
    }
}

/// The `import` rows `lower_one` reads for `unit`: `bind_imports` skips every
/// row whose importer is another unit's origin, and an invalid unit reads none.
fn read_imports(u: &Universe, unit: TermId, imports: &[TermId]) -> Vec<TermId> {
    let identity = unit_parts(u, unit).map(|parts| parts.origin);
    imports
        .iter()
        .copied()
        .filter(|row| {
            u.args::<3>(*row, "import")
                .is_some_and(|parts| Some(parts[0]) == identity)
        })
        .collect()
}
