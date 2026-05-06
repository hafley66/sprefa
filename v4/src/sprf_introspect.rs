//! `SprfIntrospect` — sprf-side downcast surface over the generic
//! `Component::describe()` blob in effect_runtime.
//!
//! `effect_runtime::v2::Component` exposes a framework-neutral
//! `describe(&self) -> Option<&dyn Any>` (saga-style descriptor seam).
//! sprf-specific descriptor types stay here so effect_runtime never
//! names them. Callers that already know they're holding a sprf
//! Component (Pipe<Cursor>) walk through this trait.
//!
//! The `Pipe<Cursor>` helpers (`binds_terms`, `reads_terms`) are the
//! direct fix for today's `RuleDef` gap: column-name extraction over a
//! Value::Pipe sub-pipe that carried a Term, without downcasting on
//! Component (which lacks `Any`).

use std::sync::Arc;

use effect_runtime::v2::{Component, Pipe};

use crate::Cursor;
use crate::term::{Term, TermMode};

/// Sprf-side shim over `Component::describe()`. Default-empty methods
/// return `None`; the blanket impl runs the downcast for any Component.
pub trait SprfIntrospect {
    fn as_term(&self) -> Option<&Term>;
}

impl<C: Component + ?Sized> SprfIntrospect for C {
    fn as_term(&self) -> Option<&Term> {
        self.describe()?.downcast_ref::<Term>()
    }
}

/// Pipe-level fold helpers. Walk the lowered pipe's steps and collect
/// the term names each step declares it binds / reads.
///
/// These exist because `Pipe<Cursor>::steps` is `Arc<dyn Component>` —
/// callers cannot enumerate Components directly. Going through
/// `as_term()` keeps the partition: effect_runtime stays sprf-blind,
/// sprf semantics live here.
pub trait PipeIntrospect {
    fn binds_terms(&self) -> Vec<Arc<str>>;
    fn reads_terms(&self) -> Vec<Arc<str>>;
}

impl PipeIntrospect for Pipe<Cursor> {
    fn binds_terms(&self) -> Vec<Arc<str>> {
        self.steps.iter()
            .filter_map(|s| s.as_term())
            .filter(|t| matches!(t.mode, TermMode::Bind))
            .map(|t| t.name.clone())
            .collect()
    }

    fn reads_terms(&self) -> Vec<Arc<str>> {
        self.steps.iter()
            .filter_map(|s| s.as_term())
            .filter(|t| matches!(t.mode, TermMode::Read))
            .map(|t| t.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_binds_and_reads_via_describe() {
        let p: Pipe<Cursor> = Pipe::new()
            .step(Arc::new(Term::bind("X")))
            .step(Arc::new(Term::read("Y")))
            .step(Arc::new(Term::bind("Z")));

        let binds = p.binds_terms();
        let reads = p.reads_terms();

        assert_eq!(binds.iter().map(|s| s.as_ref()).collect::<Vec<_>>(),
                   vec!["X", "Z"]);
        assert_eq!(reads.iter().map(|s| s.as_ref()).collect::<Vec<_>>(),
                   vec!["Y"]);
    }

    #[test]
    fn as_term_misses_when_describe_returns_none() {
        // StrConstComponent doesn't override describe, default returns None.
        let p: Pipe<Cursor> = crate::pipeline::str_pipe("hello");
        assert!(p.binds_terms().is_empty());
        assert!(p.reads_terms().is_empty());
    }
}
