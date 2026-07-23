//! sprefa-extract — source bytes in, normalized nodes + edges out. ONE crate.
//!
//! Pure, SYNC, CPU-bound, rayon-parallel, arena-mastered. No DB, no async, no
//! reactor; the async-eval flip is a later layer's problem. The full theory
//! (normalization, two-phase split, tier model, RAM discipline) + the parity
//! contract live in [`_7_tasks`] — the task ledger, mirroring
//! `v6/sprefa-store/src/tasks.rs`. The type math is the rest of this module tree:
//!
//!   _0_shape   ONE coordinate + ONE typed-kind ordinal per family + the output
//!              rows (RawNode/RawEdge/ProjectEdge). The normalization that kills
//!              v5's 4 span shapes / 3 kind reps / split node identity.
//!   _1_mask    FamilyMask (the demand cone at the extract layer) + File/Project
//!              bundles + the binding side table.
//!   _2_traits  the seams: FileExtract / ProjectExtract (two traits, kept two —
//!              the cache-key split), ProjectCx (the IO seam), Parser (the parse
//!              tier), Source (per-lang SCIP/AST/floor binding), ExtractBudget.
//!   _3_facts   the per-family fact structs, v5's inventory re-expressed with
//!              spans + typed kinds (sym/fn_sym strings -> Span/NodeRef).
//!   _4_scip    diet SCIP: occurrence/symbol/relation + OccurrenceRole bitfield
//!              + the shell-out ScipSource (foreign indexers, no bespoke FFI).
//!   _5_term    the term-extract axis (sg/ast/json/regex/yaml — pattern -> rows),
//!              a different axis from the four graph families, sharing the arena.
//!   _6_facade  the ASYNC shell (ReactiveExtract + owned ProjectView) the reactive
//!              engine holds — wraps the sync core on the blocking pool. Sync core
//!              + async shell, mirroring store::Store / StoreHandle.
//!   _7_tasks   the parity surface: Extract trait + Tasks stub + ExtractPlan.
//!
//! Companion epic plan: `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`.
#![allow(dead_code, unused_imports, clippy::new_without_default)]

pub mod _0_shape;
pub mod _1_mask;
pub mod _2_traits;
pub mod _3_facts;
pub mod _4_scip;
pub mod _5_term;
pub mod _6_facade;
pub mod _7_tasks;

// Re-export the contract surface so the crate reads as one vocabulary.
pub use _6_facade::{ProjectView, ReactiveExtract};
pub use _7_tasks::{AuxFacts, Evidence};
pub use _7_tasks::{Extract, ExtractPlan, ExtractOutput, Merged, MergedBundle, Tasks};
pub use _7_tasks::{Arened, Dispatched, FlowUnified};
