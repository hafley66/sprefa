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

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use std::sync::Mutex;

use effect_runtime::{
    Batcher, BoxFuture, CancellationToken, EffectKind, PureEffect, Store,
    SubjectKey, SubjectKind, SubjectRegistry, Unsubscribed,
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
// ReadBytesBatchEffect — bulk read.
//
// `ensure_content_loaded` running in a `pipe_batch` over N=36k cursors
// fires N `ctx.put(ReadBytesEffect)` calls. Each pays the dispatch tax
// (registry+telemetry+Box+downcast+future state machine) and each
// cache-checks separately. With a fresh process the cache hit ratio is
// 0%, so the whole stack is pure overhead.
//
// This effect collapses the N puts into one. Handler reads in parallel
// via rayon `par_iter`, returns `Vec<Option<Arc<[u8]>>>` aligned with
// the request slice. Bulk ops (`pipe_batch` for ast/json/str) call
// `ensure_content_loaded_batch` instead of looping `ensure_content_loaded`.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ReadBytesBatchEffect {
    /// Aligned with the response. `(repo, rev, path)` per slot.
    pub reads: Arc<[(Arc<str>, Arc<str>, Arc<std::path::Path>)]>,
}

impl EffectKind for ReadBytesBatchEffect {
    type Response = Vec<Option<Arc<[u8]>>>;

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.reads.iter().map(|(_, _, p)| p.as_os_str().len()).sum())
    }

    fn response_bytes(r: &Self::Response) -> Option<usize> {
        Some(r.iter().filter_map(|b| b.as_ref().map(|b| b.len())).sum())
    }
}

/// Bulk variant of [`ensure_content_loaded`]. Issues one
/// `ctx.put(ReadBytesBatchEffect)` for the cursors that need a read,
/// then re-stitches results back into a `Vec<Option<Cursor>>` aligned
/// with the input. Cursors that already have content or have no `fs`
/// pass through without consuming a read slot.
pub async fn ensure_content_loaded_batch(
    ctx: &effect_runtime::RtCtx,
    cs:  &[crate::_0_cursor::Cursor],
) -> Vec<Option<crate::_0_cursor::Cursor>> {
    // Fast path: nothing to read. Avoid an empty `ctx.put`.
    let mut needs_read: Vec<usize> = Vec::new();
    for (i, c) in cs.iter().enumerate() {
        if c.content.is_empty() && c.fs.is_some() {
            needs_read.push(i);
        }
    }
    if needs_read.is_empty() {
        return cs.iter().cloned().map(Some).collect();
    }

    let reads: Vec<(Arc<str>, Arc<str>, Arc<std::path::Path>)> = needs_read
        .iter()
        .map(|&i| {
            let c = &cs[i];
            (c.repo.clone(), c.rev.clone(), c.fs.clone().unwrap())
        })
        .collect();
    let req = ReadBytesBatchEffect { reads: Arc::from(reads) };
    let responses: Vec<Option<Arc<[u8]>>> = ctx.put(req).await;

    let mut out: Vec<Option<crate::_0_cursor::Cursor>> = cs.iter().cloned().map(Some).collect();
    for (slot, resp) in needs_read.iter().zip(responses.into_iter()) {
        let i = *slot;
        let c = cs[i].clone();
        match resp {
            Some(b) => {
                let end = b.len();
                out[i] = Some(c.rebase(b, 0..end));
            }
            None => out[i] = None,
        }
    }
    out
}

/// Sync bulk-read function. Drop into a `Passthrough` batcher (or any
/// closure-shaped batcher) to handle [`ReadBytesBatchEffect`].
/// `par_iter` reads files concurrently on rayon's global pool.
pub fn read_bytes_batch(
    source: &dyn FileSource,
    req:    ReadBytesBatchEffect,
) -> Vec<Option<Arc<[u8]>>> {
    use rayon::prelude::*;
    req.reads
        .par_iter()
        .map(|(repo, rev, path)| source.file_bytes(repo, rev, path))
        .collect()
}

pub struct ReadBytesBatchBatcher {
    source: Arc<dyn FileSource>,
}

impl ReadBytesBatchBatcher {
    pub fn new(source: Arc<dyn FileSource>) -> Self { Self { source } }
}

impl Batcher<ReadBytesBatchEffect> for ReadBytesBatchBatcher {
    fn run(
        &self,
        req:     ReadBytesBatchEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, Vec<Option<Arc<[u8]>>>> {
        let source = self.source.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || read_bytes_batch(&*source, req))
                .await
                .unwrap_or_default()
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

// ---------------------------------------------------------------------------
// WriteFileEffect — first non-pure write effect that touches durable state.
// Produced by `write_file(path)`. Sink decides whether bytes hit disk or a
// test-side buffer. Shares no domain with the read-side effects yet; once
// invalidation lands, this and `ReadBytesEffect` will both ride `"fs"` so
// a write-then-read sequence sees the new bytes.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct WriteFileEffect {
    pub path:  Arc<std::path::Path>,
    pub bytes: Arc<[u8]>,
}

impl EffectKind for WriteFileEffect {
    type Response = Result<(), Arc<str>>;

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.bytes.len())
    }
}

/// Where a [`WriteFileBatcher`] sends its writes.
#[derive(Clone)]
pub enum WriteFileSink {
    Disk,
    Buffer(Arc<Mutex<Vec<(Arc<std::path::Path>, Arc<[u8]>)>>>),
}

impl WriteFileSink {
    pub fn buffer() -> (Self, Arc<Mutex<Vec<(Arc<std::path::Path>, Arc<[u8]>)>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        (WriteFileSink::Buffer(buf.clone()), buf)
    }

    fn write(&self, path: &Arc<std::path::Path>, bytes: &Arc<[u8]>) -> Result<(), String> {
        match self {
            WriteFileSink::Disk => {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("create_dir_all({}): {}", parent.display(), e))?;
                    }
                }
                std::fs::write(path.as_ref(), bytes.as_ref())
                    .map_err(|e| format!("write({}): {}", path.display(), e))
            }
            WriteFileSink::Buffer(buf) => {
                buf.lock()
                    .expect("write buffer poisoned")
                    .push((path.clone(), bytes.clone()));
                Ok(())
            }
        }
    }
}

pub struct WriteFileBatcher {
    sink: WriteFileSink,
}

impl WriteFileBatcher {
    pub fn new(sink: WriteFileSink) -> Self { Self { sink } }
    pub fn disk() -> Self { Self { sink: WriteFileSink::Disk } }
    pub fn buffer() -> (Self, Arc<Mutex<Vec<(Arc<std::path::Path>, Arc<[u8]>)>>>) {
        let (sink, buf) = WriteFileSink::buffer();
        (Self { sink }, buf)
    }
}

impl Batcher<WriteFileEffect> for WriteFileBatcher {
    fn run(
        &self,
        req: WriteFileEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, Result<(), Arc<str>>> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.write(&req.path, &req.bytes).map_err(|e| Arc::from(e.as_str()))
        })
    }
}

// ---------------------------------------------------------------------------
// WriteRangeEffect — splice bytes into a target byte_range.
//
// One effect powers `write_cursor`, future `write_file(range)` /
// `write_file_at`, and LSP code-action edits. The target is identified
// by `(file, byte_range)` and the mode controls how `new_bytes` lands:
//
//   :replace  — overwrite target span (default)
//   :append   — insert at byte_range.end
//   :prepend  — insert at byte_range.start
//   :wrap     — surround the existing target span with new_bytes
//
// Sink decides delivery (disk / buffer); domain "fs" is the same domain
// `WriteFileEffect` and `ReadBytesEffect` will share once invalidation
// lands.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode {
    Replace,
    Append,
    Prepend,
    Wrap,
}

impl WriteMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim_start_matches(':') {
            "replace" => Some(WriteMode::Replace),
            "append"  => Some(WriteMode::Append),
            "prepend" => Some(WriteMode::Prepend),
            "wrap"    => Some(WriteMode::Wrap),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            WriteMode::Replace => "replace",
            WriteMode::Append  => "append",
            WriteMode::Prepend => "prepend",
            WriteMode::Wrap    => "wrap",
        }
    }
}

#[derive(Clone, Debug)]
pub struct WriteRangeEffect {
    pub file:       Arc<Path>,
    pub byte_range: std::ops::Range<usize>,
    pub new_bytes:  Arc<[u8]>,
    pub mode:       WriteMode,
}

impl EffectKind for WriteRangeEffect {
    type Response = Result<(), Arc<str>>;

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.new_bytes.len())
    }
}

#[derive(Clone)]
pub enum WriteRangeSink {
    Buffer(Arc<Mutex<Vec<WriteRangeEffect>>>),
}

impl WriteRangeSink {
    pub fn buffer() -> (Self, Arc<Mutex<Vec<WriteRangeEffect>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        (WriteRangeSink::Buffer(buf.clone()), buf)
    }

    fn write(&self, e: WriteRangeEffect) -> Result<(), String> {
        match self {
            WriteRangeSink::Buffer(buf) => {
                buf.lock()
                    .expect("write_range buffer poisoned")
                    .push(e);
                Ok(())
            }
        }
    }
}

pub struct WriteRangeBatcher {
    sink: WriteRangeSink,
}

impl WriteRangeBatcher {
    pub fn new(sink: WriteRangeSink) -> Self { Self { sink } }
    pub fn buffer() -> (Self, Arc<Mutex<Vec<WriteRangeEffect>>>) {
        let (sink, buf) = WriteRangeSink::buffer();
        (Self { sink }, buf)
    }
}

impl Batcher<WriteRangeEffect> for WriteRangeBatcher {
    fn run(
        &self,
        req: WriteRangeEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, Result<(), Arc<str>>> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.write(req).map_err(|e| Arc::from(e.as_str()))
        })
    }
}

// ---------------------------------------------------------------------------
// LspDiagEffect — emit one LSP-style diagnostic per cursor.
//
// Produced by `lsp[severity](message=..., code=..., hint=...)` at the
// tail of a pipe. Sink decides delivery: in-process LSP backend
// publishes via textDocument/publishDiagnostics; tests collect into a
// buffer. byte_range is `cursor.byte_range` at the time of emit, so
// every capture-narrowing op upstream contributes to where the squiggle
// lands.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LspSeverity {
    Error,
    Warning,
    Hint,
    Info,
}

impl LspSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            LspSeverity::Error   => "error",
            LspSeverity::Warning => "warn",
            LspSeverity::Hint    => "hint",
            LspSeverity::Info    => "info",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LspDiagEffect {
    pub file:       Option<Arc<Path>>,
    pub byte_range: std::ops::Range<usize>,
    pub severity:   LspSeverity,
    pub message:    Arc<str>,
    pub code:       Option<Arc<str>>,
    pub hint:       Option<Arc<str>>,
}

impl EffectKind for LspDiagEffect {
    type Response = ();

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.message.len())
    }
}

#[derive(Clone)]
pub enum LspDiagSink {
    Buffer(Arc<Mutex<Vec<LspDiagEffect>>>),
}

impl LspDiagSink {
    pub fn buffer() -> (Self, Arc<Mutex<Vec<LspDiagEffect>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        (LspDiagSink::Buffer(buf.clone()), buf)
    }

    fn write(&self, diag: LspDiagEffect) {
        match self {
            LspDiagSink::Buffer(buf) => {
                buf.lock().expect("lsp diag buffer poisoned").push(diag);
            }
        }
    }
}

pub struct LspDiagBatcher {
    sink: LspDiagSink,
}

impl LspDiagBatcher {
    pub fn new(sink: LspDiagSink) -> Self { Self { sink } }
    pub fn buffer() -> (Self, Arc<Mutex<Vec<LspDiagEffect>>>) {
        let (sink, buf) = LspDiagSink::buffer();
        (Self { sink }, buf)
    }
}

impl Batcher<LspDiagEffect> for LspDiagBatcher {
    fn run(
        &self,
        req: LspDiagEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let sink = self.sink.clone();
        Box::pin(async move {
            sink.write(req);
        })
    }
}

// ---------------------------------------------------------------------------
// AstParseEffect — offload tree-sitter parse to a worker pool.
//
// The ast op used to call `lang.ast_grep(src)` inline inside its async
// `pipe()`. With sequential cursor dispatch that ran on the driver
// thread; with concurrent dispatch (`Pipeline::Op::buffered`) it ran on
// whatever tokio worker happened to poll. Routing through `ctx.put`
// + `BoundedWorkSteal` puts the parse on the dedicated worker pool so
// the sweet spot from FINDINGS §5 is reachable.
//
// Returns `Arc<AstGrep<StrDoc<SupportLang>>>` so the caller can run
// `.root().find_all(&pattern)` and bind captures from the same parse
// without re-parsing. No cache yet — V0 is parse-per-call to mirror
// the bench shape; PureEffect + content-hash key is a follow-up.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AstParseEffect {
    pub content:    Arc<[u8]>,
    pub byte_range: std::ops::Range<usize>,
    pub lang:       ast_grep_language::SupportLang,
}

impl EffectKind for AstParseEffect {
    type Response = Option<Arc<ast_grep_core::AstGrep<ast_grep_core::source::StrDoc<ast_grep_language::SupportLang>>>>;

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.byte_range.len())
    }
}

/// Synchronous parse function — drop into a `BoundedWorkSteal` closure
/// or call directly. Returns `None` for non-UTF-8 or empty range.
pub fn ast_parse(req: AstParseEffect) -> <AstParseEffect as EffectKind>::Response {
    use ast_grep_core::Language;
    let slice = req.content.get(req.byte_range)?;
    let s = std::str::from_utf8(slice).ok()?;
    Some(Arc::new(req.lang.ast_grep(s)))
}

// ---------------------------------------------------------------------------
// TagStore + TagWriteEffect — relational bag store and its writer.
//
// `tag(:name, ...)` is the first sprefa op that uses persistent shared
// state across cursors. The bag is just a vec of string-tuple rows keyed
// by tag name; writers append, readers drain + perpetually subscribe.
//
// Shared state lives in `TagStore`, registered once on `RtCtxBuilder` via
// `with_store`. Writer side issues `TagWriteEffect`; the batcher pushes
// the row into the bag and drains any waiters that the read op has
// parked there. Wakes go through `SubjectRegistry::next` — same primitive
// as `Yield`, no separate channel.
//
// Read-side state machine lives in `ops/tag.rs` because the perpetual
// subscription override is a `pipe_flat_map` shape, not an effect.
// ---------------------------------------------------------------------------

/// One row in a tag bag — opaque to the runtime, downstream zips by hole.
pub type TagRow = Vec<Arc<str>>;

/// `SubjectKind` for tag wakes. Payload is the freshly-pushed row, so
/// readers receive it directly without re-snapshotting under the store
/// mutex.
pub struct TagWake;
impl SubjectKind for TagWake {
    type Payload = TagRow;
}

/// One bag of rows + parked subscribers waiting for the next write.
#[derive(Default)]
pub struct Bag {
    pub rows: Vec<TagRow>,
    /// Subjects pre-registered on `SubjectRegistry<TagWake>` (entries
    /// already inserted in `pending`). On write, each is woken via
    /// `registry.next(key, row)` and removed from this list.
    pub waiters: Vec<SubjectKey<TagWake>>,
}

/// Outcome of an atomic snapshot-or-subscribe call. Either the caller
/// gets the tail of new rows since `last_idx`, or it gets a future that
/// resolves the next time a writer drains the waiter list.
pub enum SnapshotOrSubscribed {
    Rows(Vec<TagRow>),
    Subscribed(BoxFuture<'static, Result<Arc<TagRow>, Unsubscribed>>),
}

/// Relational bag store. `TagStore` holds one `Bag` per tag name.
///
/// Stored on `RtCtx` via `with_store`. The read op fetches via
/// `ctx.store::<TagStore>()`; the write batcher holds an `Arc<TagStore>`
/// at registration time.
#[derive(Default)]
pub struct TagStore {
    inner: Mutex<HashMap<Arc<str>, Bag>>,
}

impl Store for TagStore {}

impl TagStore {
    pub fn new() -> Self { Self::default() }

    /// Test/debug hook: how many rows currently in `name`'s bag.
    pub fn rows_len(&self, name: &str) -> usize {
        self.inner
            .lock()
            .unwrap()
            .get(name)
            .map(|b| b.rows.len())
            .unwrap_or(0)
    }

    /// Atomically check for new rows past `last_idx`. If any exist,
    /// return them as `Rows`. Otherwise register a fresh waiter on the
    /// supplied `SubjectRegistry` (synchronously inserts a pending entry
    /// per `subscribe`'s contract) and return a `Subscribed` future.
    ///
    /// The `key` passed in is consumed: it goes into both the registry
    /// pending map and the bag's waiters list under one store-lock
    /// critical section, closing the producer-side race where a write
    /// would otherwise drain `key` from the bag before the registry
    /// entry exists.
    pub fn snapshot_or_subscribe(
        &self,
        name: &Arc<str>,
        last_idx: usize,
        key:      SubjectKey<TagWake>,
        registry: &Arc<SubjectRegistry<TagWake>>,
    ) -> SnapshotOrSubscribed {
        let mut g = self.inner.lock().unwrap();
        let bag = g.entry(name.clone()).or_default();
        if bag.rows.len() > last_idx {
            let tail: Vec<TagRow> = bag.rows[last_idx..].to_vec();
            SnapshotOrSubscribed::Rows(tail)
        } else {
            let fut = registry.subscribe(key, None);
            bag.waiters.push(key);
            SnapshotOrSubscribed::Subscribed(fut)
        }
    }

    /// Append `row` to the named bag. Returns the waiter list that was
    /// parked at the time of the write; caller fires `registry.next` on
    /// each.
    pub fn push_row(&self, name: &Arc<str>, row: TagRow) -> Vec<SubjectKey<TagWake>> {
        let mut g = self.inner.lock().unwrap();
        let bag = g.entry(name.clone()).or_default();
        bag.rows.push(row);
        std::mem::take(&mut bag.waiters)
    }
}

/// Append one row to a named bag. The batcher fires `registry.next` on
/// every parked waiter so the read op can re-snapshot.
#[derive(Clone, Debug)]
pub struct TagWriteEffect {
    pub name: Arc<str>,
    pub row:  Vec<Arc<str>>,
}

impl EffectKind for TagWriteEffect {
    type Response = ();

    fn payload_bytes(&self) -> Option<usize> {
        Some(self.row.iter().map(|s| s.len()).sum::<usize>() + self.name.len())
    }
}

pub struct TagWriteBatcher {
    store:    Arc<TagStore>,
    registry: Arc<SubjectRegistry<TagWake>>,
}

impl TagWriteBatcher {
    pub fn new(store: Arc<TagStore>, registry: Arc<SubjectRegistry<TagWake>>) -> Self {
        Self { store, registry }
    }
}

impl Batcher<TagWriteEffect> for TagWriteBatcher {
    fn run(
        &self,
        req:     TagWriteEffect,
        _cancel: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        let store = self.store.clone();
        let registry = self.registry.clone();
        Box::pin(async move {
            let row = req.row;
            let waiters = store.push_row(&req.name, row.clone());
            // Ship the row through the wake. Each waiter receives a cheap
            // Arc clone; the read loop emits without re-acquiring the
            // store mutex for a snapshot.
            let wake: Arc<TagRow> = Arc::new(row);
            for k in waiters {
                registry.next(k, wake.clone());
            }
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
    async fn write_file_buffer_captures_bytes() {
        let (batcher, buf) = WriteFileBatcher::buffer();
        let rt = RtCtxBuilder::new().register::<WriteFileEffect, _>(batcher).build();
        let path: Arc<std::path::Path> = Arc::from(std::path::Path::new("out/a.txt"));
        let res = rt.put(WriteFileEffect {
            path: path.clone(),
            bytes: Arc::from(b"hello".as_slice()),
        }).await;
        assert!(res.is_ok());
        let written = buf.lock().unwrap().clone();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, path);
        assert_eq!(written[0].1.as_ref(), b"hello");
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
