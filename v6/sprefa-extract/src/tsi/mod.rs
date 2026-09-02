//! The type-system-interchange envelope riding the `FlatFact` JSONL wire:
//! protocol, run, witness, coverage, diagnostic.

pub mod types;

pub use types::{
    Arg, CoverageOut, DiagnosticOut, FactOut, Method, Mode, RunOut, WitnessOut, PROTOCOL_VERSION,
};
