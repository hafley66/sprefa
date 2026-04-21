//! sprefa::writer — re-export shim over `spine::writer`.
//!
//! The `Writer` trait + row types live in spine. Concrete writers
//! (`MemWriter`, sqlite writer) stay in `sprefa::writers::*`.
pub use spine::writer::*;
