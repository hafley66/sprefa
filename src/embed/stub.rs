//! Deterministic hashed pseudo-embedding. The shipped default: zero deps, no
//! network, reproducible. It is NOT semantically meaningful — it is a hashed
//! bag-of-tokens, so it tracks lexical overlap, not meaning — but it exercises
//! the whole encode -> store -> KNN -> `similar` pipeline so the plumbing and
//! the seam-rig A/B run before any real model is pulled. Swap in a real backend
//! with `--features embed-fastembed` / `embed-candle` when you want meaning.

use super::{l2_normalize, Embedder};
use anyhow::Result;

const DIM: u32 = 64;

#[derive(Default)]
pub struct Stub;

impl Embedder for Stub {
    fn name(&self) -> &'static str {
        "stub"
    }
    fn dim(&self) -> u32 {
        DIM
    }
    fn encode(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| embed_one(t)).collect())
    }
}

/// Hash each lowercased token into one of DIM buckets and count it. Two texts
/// sharing tokens get a positive cosine that rises with overlap — enough to
/// sanity-check the pipeline and weakly rank lexical similarity.
fn embed_one(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM as usize];
    for tok in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        let h = blake3::hash(tok.to_ascii_lowercase().as_bytes());
        let bucket =
            (u64::from_le_bytes(h.as_bytes()[0..8].try_into().unwrap()) % DIM as u64) as usize;
        v[bucket] += 1.0;
    }
    l2_normalize(&mut v);
    v
}
