//! fastembed backend (ONNX models, offline after first model download).
//! Compiled only under `--features embed-fastembed`. The seam (feature flag,
//! cascade, runtime selection) is complete; the model impl is the one-file TODO.
//!
//! To wire:
//!   1. add `fastembed = { version = "4", optional = true }` to [dependencies]
//!   2. change the feature to `embed-fastembed = ["dep:fastembed"]` in [features]
//!   3. hold a `fastembed::TextEmbedding` in `Fe`, init it in `new`, and map
//!      `encode` onto `TextEmbedding::embed(texts, None)`; set `dim()` to the
//!      chosen model's dimension (e.g. 384 for all-MiniLM-L6-v2).

use super::Embedder;
use anyhow::{bail, Result};

pub struct Fe;

impl Fe {
    pub fn new() -> Result<Self> {
        bail!(
            "embed-fastembed compiled but not yet wired; \
             add the dep and implement src/embed/fastembed_be.rs::Fe (see header)"
        )
    }
}

impl Embedder for Fe {
    fn name(&self) -> &'static str {
        "fastembed"
    }
    fn dim(&self) -> u32 {
        384
    }
    fn encode(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        bail!("embed-fastembed not yet wired")
    }
}
