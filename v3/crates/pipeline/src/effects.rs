//! Pipeline effects — the typed seam between ops and the outside world.
//!
//! v3 doctrine (parse.md §14.5c, FINDINGS §5.2): byte + listing reads
//! flow through `RtCtx.put` so the framework can coalesce, cache, and
//! measure them once instead of every op rolling its own reader
//! plumbing.
//!
//! This module defines the first read-shaped effect:
//!
//!   * [`FsListFilesEffect`] — "list every file under `(repo, rev)`".
//!     Input key is `(repo, rev)`. Response is `Vec<Arc<Path>>`. Purely
//!     readable (no writes), so it rides the `PureEffect` surface and
//!     gets automatic caching via the `CacheLayer`.
//!
//! The op no longer owns an `Arc<dyn FileSource>`. Callers register a
//! [`FsListFilesBatcher`] once on the `RtCtxBuilder` and every `fs(...)`
//! call-site picks up the same cached listing across seeds, rules, and
//! pipes. Single-shot runs pay the same walk cost as before; multi-pipe
//! runs amortize it across call-sites.

use std::path::Path;
use std::sync::Arc;

use std::sync::Mutex;

use effect_runtime::{
    Batcher, BoxFuture, CancellationToken, EffectKind, PureEffect,
};

use crate::readers::FileSource;

/// List every file under `(repo, rev)`. Results are repo-relative paths.
///
/// Domain `"fs"` — a future `WriteBytes`/`Checkout` effect tagged with
/// the same domain would invalidate cached listings automatically.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FsListFilesEffect {
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
}

impl EffectKind for FsListFilesEffect {
    type Response = Vec<Arc<Path>>;

    fn response_bytes(r: &Self::Response) -> Option<usize> {
        // Path sizes are a rough proxy — the cache layer uses this for
        // weight-based eviction. Accuracy matters less than consistency.
        Some(r.iter().map(|p| p.as_os_str().len()).sum())
    }
}

impl PureEffect for FsListFilesEffect {
    type Key = (Arc<str>, Arc<str>);
    const DOMAIN: &'static str = "fs";

    fn cache_key(&self) -> Self::Key {
        (self.repo.clone(), self.rev.clone())
    }
}

/// Batcher backed by a [`FileSource`]. One per-run handle; register via
/// [`effect_runtime::RtCtxBuilder::register_pure`] to get caching for
/// free:
///
/// ```ignore
/// let rt = RtCtxBuilder::new()
///     .register_pure::<FsListFilesEffect, _>(
///         1024,
///         FsListFilesBatcher::new(Arc::new(DiskFileSource::new(root, rev))),
///     )
///     .build();
/// ```
pub struct FsListFilesBatcher {
    source: Arc<dyn FileSource>,
}

impl FsListFilesBatcher {
    pub fn new(source: Arc<dyn FileSource>) -> Self {
        Self { source }
    }
}

impl Batcher<FsListFilesEffect> for FsListFilesBatcher {
    fn run(
        &self,
        req: FsListFilesEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, Vec<Arc<Path>>> {
        // File sources are sync; wrap the listing in a ready future so
        // the effect surface stays async-uniform. Expensive backends
        // (git2, network) would `tokio::task::spawn_blocking` here.
        let source = self.source.clone();
        Box::pin(async move {
            source.files(&req.repo, &req.rev)
        })
    }
}

// ---------------------------------------------------------------------------
// ReadBytesEffect — per-file byte read. Shares the `"fs"` domain with
// `FsListFilesEffect` so a future `WriteBytes`/`Checkout` invalidates
// both cached listings and cached file bytes in one stroke.
// ---------------------------------------------------------------------------

/// Read the bytes of `(repo, rev, path)`. `None` response means the
/// backing source does not know the file; the `read` op turns that into
/// a `read/no-bytes` diagnostic. A present-but-empty `Arc<[u8]>` is a
/// legitimate empty file and flows through.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ReadBytesEffect {
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
    pub path: Arc<std::path::Path>,
}

impl EffectKind for ReadBytesEffect {
    type Response = Option<Arc<[u8]>>;

    fn response_bytes(r: &Self::Response) -> Option<usize> {
        r.as_ref().map(|b| b.len())
    }
}

impl PureEffect for ReadBytesEffect {
    type Key = (Arc<str>, Arc<str>, Arc<std::path::Path>);
    const DOMAIN: &'static str = "fs";

    fn cache_key(&self) -> Self::Key {
        (self.repo.clone(), self.rev.clone(), self.path.clone())
    }
}

/// Load the bytes referenced by `c.fs` into `c.content` if they are
/// not already there. This is the seam that makes the `read` op
/// redundant for downstream byte-readers: `comment`, `print`, and
/// future `re`/`ast`/`json`/`md` ops call this on their input cursors
/// and the framework does the right thing (cached per `(repo, rev,
/// path)` via the `"fs"` domain).
///
/// Returns:
/// - `Some(c)` unchanged when `c.content` is already populated or
///   `c.fs` is `None`.
/// - `Some(rebased)` with `content = bytes` and `byte_range = 0..len`
///   when a read succeeded.
/// - `None` when `c.fs` is set but the reader returned no bytes; the
///   caller drops the cursor.
pub async fn ensure_content_loaded(ctx: &effect_runtime::RtCtx, c: crate::_0_cursor::Cursor) -> Option<crate::_0_cursor::Cursor> {
    if !c.content.is_empty() {
        return Some(c);
    }
    let Some(path) = c.fs.clone() else {
        return Some(c);
    };
    let bytes = ctx.put(ReadBytesEffect {
        repo: c.repo.clone(),
        rev:  c.rev.clone(),
        path,
    }).await;
    match bytes {
        Some(b) => {
            let end = b.len();
            Some(c.rebase(b, 0..end))
        }
        None => None,
    }
}

pub struct ReadBytesBatcher {
    source: Arc<dyn FileSource>,
}

impl ReadBytesBatcher {
    pub fn new(source: Arc<dyn FileSource>) -> Self {
        Self { source }
    }
}

impl Batcher<ReadBytesEffect> for ReadBytesBatcher {
    fn run(
        &self,
        req: ReadBytesEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, Option<Arc<[u8]>>> {
        let source = self.source.clone();
        Box::pin(async move {
            source.file_bytes(&req.repo, &req.rev, &req.path)
        })
    }
}

// ---------------------------------------------------------------------------
// PrintEffect — first write-side (non-pure) effect. Proves the write
// path through `RtCtxBuilder::register` independently of `register_pure`.
// ---------------------------------------------------------------------------

/// Emit a single line to a sink. The sink is picked by the batcher at
/// registration time; ops do not know whether they are writing to
/// stdout, a buffer, or a network service.
#[derive(Clone, Debug)]
pub struct PrintEffect {
    pub line: Arc<str>,
}

impl EffectKind for PrintEffect {
    type Response = ();

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.line.len())
    }
}

/// Where a [`PrintBatcher`] sends its lines.
///
/// Split as an enum rather than a trait so `PrintBatcher` stays
/// `Clone + Send + Sync` without wrapping every call in `Arc<dyn Fn>`
/// and so tests can assert against captured output without building
/// their own stdout redirector.
#[derive(Clone)]
pub enum PrintSink {
    Stdout,
    Buffer(Arc<Mutex<Vec<String>>>),
}

impl PrintSink {
    pub fn buffer() -> (Self, Arc<Mutex<Vec<String>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        (PrintSink::Buffer(buf.clone()), buf)
    }

    fn write(&self, line: &str) {
        match self {
            PrintSink::Stdout => println!("{line}"),
            PrintSink::Buffer(buf) => {
                buf.lock().expect("print buffer poisoned").push(line.to_string());
            }
        }
    }
}

/// Batcher that writes each `PrintEffect` to its sink. Register via
/// [`effect_runtime::RtCtxBuilder::register`] — this effect is not
/// cacheable, so it never rides `register_pure`.
pub struct PrintBatcher {
    sink: PrintSink,
}

impl PrintBatcher {
    pub fn new(sink: PrintSink) -> Self { Self { sink } }
    pub fn stdout() -> Self { Self { sink: PrintSink::Stdout } }
    pub fn buffer() -> (Self, Arc<Mutex<Vec<String>>>) {
        let (sink, buf) = PrintSink::buffer();
        (Self { sink }, buf)
    }
}

impl Batcher<PrintEffect> for PrintBatcher {
    fn run(
        &self,
        req: PrintEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.write(&req.line);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::MemFileSource;
    use effect_runtime::RtCtxBuilder;

    #[tokio::test]
    async fn list_returns_registered_paths() {
        let src: Arc<dyn FileSource> = Arc::new(
            MemFileSource::new().with_files("r", "main", &["a.rs", "b.rs"]),
        );
        let rt = RtCtxBuilder::new()
            .register_pure::<FsListFilesEffect, _>(16, FsListFilesBatcher::new(src))
            .build();
        let files = rt.put(FsListFilesEffect {
            repo: Arc::from("r"),
            rev:  Arc::from("main"),
        }).await;
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn print_effect_writes_to_buffer_sink() {
        let (batcher, buf) = PrintBatcher::buffer();
        let rt = RtCtxBuilder::new().register::<PrintEffect, _>(batcher).build();
        rt.put(PrintEffect { line: Arc::from("hello") }).await;
        rt.put(PrintEffect { line: Arc::from("world") }).await;
        let lines = buf.lock().unwrap().clone();
        assert_eq!(lines, vec!["hello".to_string(), "world".to_string()]);
    }

    #[tokio::test]
    async fn unknown_coords_return_empty() {
        let src: Arc<dyn FileSource> = Arc::new(
            MemFileSource::new().with_files("r", "main", &["a.rs"]),
        );
        let rt = RtCtxBuilder::new()
            .register_pure::<FsListFilesEffect, _>(16, FsListFilesBatcher::new(src))
            .build();
        let files = rt.put(FsListFilesEffect {
            repo: Arc::from("other"),
            rev:  Arc::from("main"),
        }).await;
        assert!(files.is_empty());
    }
}
