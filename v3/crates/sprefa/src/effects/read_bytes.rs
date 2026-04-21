//! `ReadBytes` — read a `(repo, rev, fs)` blob as full bytes.
//!
//! Currently: request → response is fulfilled by delegating to the
//! `Reader` stack stashed in the batcher. As effect-layer batching matures,
//! this turns into a coalescing batcher (e.g. `BoundedBatched` with a
//! single `git cat-file --batch` worker per repo) without touching op
//! code; the request/response shape is stable.

use std::sync::Arc;

use bytes::Bytes;
use effect_runtime::{Batcher, BoxFuture, CancellationToken, EffectKind};
use futures_util::StreamExt;

use crate::reader::Reader;
use crate::types::FilePath;

/// Request: full bytes for `(repo, rev, fs)`. Empty bytes on miss.
#[derive(Clone, Debug)]
pub struct ReadBytes {
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
    pub fs:   FilePath,
}

impl EffectKind for ReadBytes {
    type Response = Bytes;

    fn payload_bytes(&self) -> Option<usize> { None }
    fn response_bytes(r: &Bytes) -> Option<usize> { Some(r.len()) }
}

/// Default pass-through batcher: delegates every request to a stashed
/// `Reader` stack. One roundtrip per put. Replace via `rt.rebind` when
/// a coalescing/shell-out batcher is desired.
pub struct ReaderBackedBatcher {
    pub reader: Arc<dyn Reader>,
}

impl Batcher<ReadBytes> for ReaderBackedBatcher {
    fn run(&self, req: ReadBytes, _cancel: CancellationToken)
        -> BoxFuture<'static, Bytes>
    {
        let reader = self.reader.clone();
        Box::pin(async move {
            reader
                .bytes(&req.repo, &req.rev, &req.fs)
                .next()
                .await
                .unwrap_or_else(Bytes::new)
        })
    }
}
