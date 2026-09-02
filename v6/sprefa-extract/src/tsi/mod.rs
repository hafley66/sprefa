//! The type-system-interchange envelope riding the `FlatFact` JSONL wire:
//! protocol, run, witness, coverage, diagnostic.

pub mod ingest;
pub mod registry;
pub mod semantic;
pub mod sink;
pub mod types;

pub use ingest::{ingest, IngestError};
pub use registry::{relation, ArgKind, Relation, REGISTRY};
pub use semantic::{emit_semantic, CoverageClaim, SemanticRows};
pub use sink::TsiSink;
pub use types::{
    Arg, CoverageOut, DiagnosticOut, FactOut, Method, Mode, RunOut, WitnessOut, PROTOCOL_VERSION,
};
