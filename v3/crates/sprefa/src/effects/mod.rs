//! Effect kinds and their default batchers.
//!
//! This module is the seam between v2's `Reader` trait and v3's
//! `effect_runtime`. Ops that historically called `reader.bytes(...)`
//! will progressively migrate to `ctx.rt.put(ReadBytes { ... })`.
//!
//! One file per `EffectKind`. Framework core never grows — adding a
//! new effect is `mod new_effect;` + register in `build_default_rt`.

pub mod read_bytes;

pub use read_bytes::ReadBytes;

use std::sync::Arc;

use effect_runtime::{RtCtx, RtCtxBuilder};

use crate::reader::Reader;

/// Default RtCtx wiring. Takes the composed `Reader` stack and registers
/// batchers that translate effect requests into Reader calls. Ops stay
/// free of Reader-trait coupling.
///
/// Over time, domain-specific batchers (sqlite writes, git walks,
/// ast-grep scans) register here alongside `ReadBytes`, keyed by their
/// own `EffectKind` type so registration is additive.
pub fn build_default_rt(reader: &Arc<dyn Reader>) -> RtCtx {
    RtCtxBuilder::new()
        .register::<ReadBytes, _>(read_bytes::ReaderBackedBatcher {
            reader: reader.clone(),
        })
        .build()
}
