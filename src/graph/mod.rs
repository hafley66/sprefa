//! Pure graph algorithms over the code model, grouped in one place:
//! - `walk`: multi-source BFS (reachability, halt-contraction, depth-lattice).
//! - `scc`: Tarjan strongly-connected components + condensation.
//! - `modgraph`: module-dependency graph construction.
//! - `typegraph`: type/reference/dataflow graph extraction.
//!
//! Each is re-exported at the crate root (`pub use graph::...` in lib.rs), so
//! existing `crate::walk::`, `crate::scc::`, `crate::modgraph::`,
//! `crate::typegraph::` paths resolve unchanged.

pub mod modgraph;
pub mod scc;
pub mod typegraph;
pub mod walk;
