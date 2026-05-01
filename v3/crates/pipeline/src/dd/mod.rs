//! Differential-Dataflow bridge (feature `dd`, off by default).
//!
//! Card sprefa-07b under epic sprefa-avl. Lands the module structure and a
//! synthetic smoke that round-trips a fact through DD's InputSession +
//! inspect_batch + probe. No production code path consumes this yet; the
//! existing RelationStore stays the default backend until the perf gate
//! clears and card E swaps the runtime.
//!
//! Layer map:
//!   - _0_row:     Row + FactName + Time + Diff aliases
//!   - _1_scope:   build_scope_synthetic — hand-wired smoke graph
//!   - _2_runner:  DdRunner facade; the long-lived Worker story is card E
//!   - _3_persist: SQLite input log (stub here, lands in card E)
//!
//! Reading order: 20260501.0.dd-adoption-effects-control-flow-types-perf.md
//! and 20260501.1.dd-effects-control-flow-types.md, plus
//! ~/projects/ext/sprefa-dd-poc/src/bin/larger.rs for the proven shape.

#[allow(non_snake_case)]
pub mod _0_row;
#[allow(non_snake_case)]
pub mod _1_scope;
#[allow(non_snake_case)]
pub mod _2_runner;
#[allow(non_snake_case)]
pub mod _3_persist;

pub use _0_row::{Diff, FactName, Row, Time, Value};
pub use _1_scope::{run_synthetic_scope, EffectDesc, SmokeReport};
pub use _2_runner::DdRunner;
