//! Embedding backends behind a compile-time feature switch.
//!
//! `dl` embeds interned strings (the `_strings` spine) into vectors so the
//! `similar(a, b, score)` relation can answer "what content is near this." The
//! vector is model-dependent, so it is keyed by `(StringId, backend)` in
//! `_embeddings` — content addressing keys the *content*, the backend names the
//! *model*. Two builds with different backends can share one db without ever
//! cosine-comparing across spaces.
//!
//! Swap backends at compile time:
//!   cargo build                              # `stub` only (zero deps, offline, deterministic)
//!   cargo build --features embed-fastembed   # ONNX models (once wired, see fastembed_be.rs)
//!   cargo build --features embed-candle      # candle (once wired, see candle_be.rs)
//!   cargo build --features "embed-fastembed,embed-candle"  # both in; pick at runtime via env
//!
//! At runtime, `SPREFA_EMBED_BACKEND=<name>` (or an explicit caller arg) picks
//! among the backends THIS binary compiled in; unset falls to the compile-time
//! default cascade (heaviest real model wins, `stub` is the floor that always
//! exists). `available()` lists what this build can run.

use anyhow::{bail, Result};

/// One embedding model. `encode` returns one vector per input, each `dim()`
/// long. Implementations should L2-normalize (the engine normalizes again
/// defensively, so cosine == dot product downstream either way).
pub trait Embedder: Send + Sync {
    fn name(&self) -> &'static str;
    fn dim(&self) -> u32;
    fn encode(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

mod stub;
pub use stub::Stub;

#[cfg(feature = "embed-fastembed")]
mod fastembed_be;
#[cfg(feature = "embed-candle")]
mod candle_be;

/// Backends compiled into THIS binary, for `make` error messages and tooling.
pub fn available() -> &'static [&'static str] {
    &[
        "stub",
        #[cfg(feature = "embed-fastembed")]
        "fastembed",
        #[cfg(feature = "embed-candle")]
        "candle",
    ]
}

/// Construct the active embedder: explicit name > `SPREFA_EMBED_BACKEND` env >
/// compile-time default cascade. A name not compiled into this build errors with
/// the available set rather than failing to link.
pub fn make(explicit: Option<&str>) -> Result<Box<dyn Embedder>> {
    let want = explicit
        .map(str::to_owned)
        .or_else(|| std::env::var("SPREFA_EMBED_BACKEND").ok())
        .filter(|s| !s.is_empty());
    match want.as_deref() {
        Some("stub") => Ok(Box::new(Stub::default())),
        Some("fastembed") => mk_fastembed(),
        Some("candle") => mk_candle(),
        Some(o) => bail!("unknown embed backend '{o}'; this build has {:?}", available()),
        None => default_backend(),
    }
}

/// Default when nothing is named: the heaviest real model compiled in, else the
/// stub floor.
fn default_backend() -> Result<Box<dyn Embedder>> {
    #[cfg(feature = "embed-candle")]
    {
        return mk_candle();
    }
    #[cfg(feature = "embed-fastembed")]
    {
        return mk_fastembed();
    }
    #[allow(unreachable_code)]
    Ok(Box::new(Stub::default()))
}

// cfg-paired constructors: the `not(feature)` twin gives a clean "built without
// this feature" error instead of a missing-symbol link failure when a backend
// is named that this build did not compile in.
#[cfg(feature = "embed-fastembed")]
fn mk_fastembed() -> Result<Box<dyn Embedder>> {
    Ok(Box::new(fastembed_be::Fe::new()?))
}
#[cfg(not(feature = "embed-fastembed"))]
fn mk_fastembed() -> Result<Box<dyn Embedder>> {
    bail!("dl built without --features embed-fastembed")
}

#[cfg(feature = "embed-candle")]
fn mk_candle() -> Result<Box<dyn Embedder>> {
    Ok(Box::new(candle_be::Candle::new()?))
}
#[cfg(not(feature = "embed-candle"))]
fn mk_candle() -> Result<Box<dyn Embedder>> {
    bail!("dl built without --features embed-candle")
}

/// Serialize a vector as the TEXT stored in `_embeddings.vec`: comma-joined f32.
/// TEXT (not BLOB) so the existing Value::Text plural insert path applies, no
/// new write seam. The sqlite-vec ANN follow-on mirrors this into a vec table.
pub fn encode_vec(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 8);
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{x:.6}"));
    }
    s
}

/// Inverse of `encode_vec`.
pub fn parse_vec(s: &str) -> Vec<f32> {
    s.split(',').filter_map(|p| p.parse::<f32>().ok()).collect()
}

/// L2-normalize in place so cosine reduces to a dot product downstream.
pub fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Cosine of two L2-normalized vectors (dot product). Compares over the shared
/// prefix on a length mismatch (defensive; one backend yields one dim).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
