//! Effect kind: read bytes for a fake "path."
//!
//! Entire surface is this one file. Adding this effect touched:
//! - this file (new)
//! - src/effects/mod.rs (one line)
//! Zero framework edits.

use effect_runtime::{Batcher, BoxFuture, CancellationToken, EffectKind};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ReadBytes {
    pub path: String,
}

impl EffectKind for ReadBytes {
    type Response = Vec<u8>;
}

/// Fake in-memory filesystem to stand in for git/blob/reader.
pub struct InMemReader {
    files: Arc<HashMap<String, Vec<u8>>>,
}

impl InMemReader {
    pub fn new(files: HashMap<String, Vec<u8>>) -> Self {
        Self { files: Arc::new(files) }
    }
}

impl Batcher<ReadBytes> for InMemReader {
    fn run(&self, req: ReadBytes, _cancel: CancellationToken) -> BoxFuture<'static, Vec<u8>> {
        let files = self.files.clone();
        Box::pin(async move { files.get(&req.path).cloned().unwrap_or_default() })
    }
}
