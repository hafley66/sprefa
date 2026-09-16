//! candle backend (pure-Rust inference, local model weights). Compiled only
//! under `--features embed-candle`. The seam is complete; the model impl is the
//! one-file TODO.
//!
//! To wire:
//!   1. add candle-core / candle-nn / tokenizers / hf-hub (all optional) to
//!      [dependencies]
//!   2. change the feature to
//!      `embed-candle = ["dep:candle-core", "dep:candle-nn", "dep:tokenizers", "dep:hf-hub"]`
//!   3. load a sentence-transformer (e.g. BERT) + its tokenizer in `new`, run
//!      the forward pass + mean-pool in `encode`, and set `dim()` accordingly.

use super::Embedder;
use anyhow::{bail, Result};

pub struct Candle;

impl Candle {
    pub fn new() -> Result<Self> {
        bail!(
            "embed-candle compiled but not yet wired; \
             add the deps and implement src/embed/candle_be.rs::Candle (see header)"
        )
    }
}

impl Embedder for Candle {
    fn name(&self) -> &'static str {
        "candle"
    }
    fn dim(&self) -> u32 {
        384
    }
    fn encode(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        bail!("embed-candle not yet wired")
    }
}
