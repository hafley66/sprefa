//! `expand` — the queue-backed loop, generic over carrier.
//!
//! Pull a runnable row → look up the component at `depth` → render →
//! flatten → enqueue children. Repeat until nothing is runnable.
//!
//! Synchronous Phase-3 form. Yield parks rows; the caller advances
//! the `EventBus` ready set (or the global tick) and re-enters `expand`
//! to make progress.

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::component::{ComponentLifecycle, DynComponent, EffectTelemetry, RenderCtx};
use super::diag::{DiagSink, NoopDiagSink};
use super::event_bus::EventBus;
use super::next::Next;
use super::queue::{BarrierScope, ExpandTick, InstanceId, PipeHash, QueueBackend, QueueRow};
use super::wake::Wake;

/// Unbuilt pipe value. The shape DSL holes resolve to at lower-time:
/// a flat sequence of components over carrier `N`. CST grammars can
/// produce this without a runtime dep beyond the `Component` trait.
///
/// Build into a runnable form via `into_instance()`.
pub struct Pipe<N: Next> {
    pub steps: Vec<DynComponent<N>>,
}

impl<N: Next> Pipe<N> {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn from_steps(steps: Vec<DynComponent<N>>) -> Self {
        Self { steps }
    }

    /// Append a step. Builder form: `Pipe::new().step(a).step(b)`.
    pub fn step(mut self, c: DynComponent<N>) -> Self {
        self.steps.push(c);
        self
    }

    /// Append all steps from another pipe. Used when a DSL hole's
    /// inner pipe is spliced into an outer one at lower-time.
    pub fn extend(mut self, other: Pipe<N>) -> Self {
        self.steps.extend(other.steps);
        self
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Convert to a mounted, runnable instance. Pipe-hash and
    /// instance-id default to 0 today; assign once the lower-pass
    /// has identity stamping.
    pub fn into_instance(self) -> PipeInstance<N> {
        PipeInstance::new(self.steps)
    }
}

impl<N: Next> Default for Pipe<N> {
    fn default() -> Self {
        Self::new()
    }
}

// Cloning a Pipe deep-copies the step Vec but shares each component
// via Arc clone. Lower-time call sites need this when a Value::Pipe
// argument is consumed in more than one place (e.g. validate scans the
// shape, lower then folds the steps in).
impl<N: Next> Clone for Pipe<N> {
    fn clone(&self) -> Self {
        Self {
            steps: self.steps.iter().cloned().collect(),
        }
    }
}

/// One mounted pipe instance. Pipe homogeneous in `N`; components
/// pinned via `dyn Component<Next = N>`.
pub struct PipeInstance<N: Next> {
    pub pipe_hash: PipeHash,
    pub instance_id: InstanceId,
    pub components: Vec<DynComponent<N>>,
}

impl<N: Next> PipeInstance<N> {
    pub fn new(components: Vec<DynComponent<N>>) -> Self {
        Self {
            pipe_hash: 0,
            instance_id: 0,
            components,
        }
    }
}

/// Caller-supplied state. Holds the `EventBus` so the driver can
/// consult ready keys per loop iteration. `batch_cap` is the max rows
/// the driver pulls per dispatch — Component::render_batch sees up to
/// that many homogeneous rows in one call.
#[derive(Clone)]
pub struct ExpandOpts {
    pub bus: Arc<EventBus>,
    pub diag: Arc<dyn DiagSink>,
    pub batch_cap: usize,
    pub runtime: Option<Arc<dyn Any + Send + Sync>>,
    pub telemetry: Option<Arc<EffectTelemetry>>,
    /// Phase 4: the memo/replay/reconcile seam. Stored type-erased
    /// (same `Arc<dyn Any>` trick as `runtime`) so `ExpandOpts` stays
    /// non-generic; `expand<N>` downcasts to `Arc<dyn MemoSeam<N>>`.
    /// `None` ⇒ the pre-Phase-4 path (no probe, every component
    /// dispatches).
    pub memo_seam: Option<Arc<dyn Any + Send + Sync>>,
}

pub const DEFAULT_BATCH_CAP: usize = 256;

impl Default for ExpandOpts {
    fn default() -> Self {
        Self {
            bus: Arc::new(EventBus::new()),
            diag: Arc::new(NoopDiagSink),
            batch_cap: DEFAULT_BATCH_CAP,
            runtime: None,
            telemetry: None,
            memo_seam: None,
        }
    }
}

impl ExpandOpts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = bus;
        self
    }

    pub fn with_diag(mut self, diag: Arc<dyn DiagSink>) -> Self {
        self.diag = diag;
        self
    }

    pub fn with_batch_cap(mut self, cap: usize) -> Self {
        self.batch_cap = cap.max(1);
        self
    }

    pub fn with_runtime<T: Any + Send + Sync>(mut self, runtime: Arc<T>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_telemetry(mut self, telemetry: Arc<EffectTelemetry>) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Phase 4: install the memo/replay/reconcile seam. Type-erased
    /// behind a sized newtype so `ExpandOpts` stays non-generic;
    /// `expand<N>` recovers it via `downcast`.
    pub fn with_memo_seam<N: Next>(
        mut self,
        seam: super::memo_seam::DynMemoSeam<N>,
    ) -> Self {
        self.memo_seam = Some(Arc::new(SeamCell(seam)));
        self
    }
}

/// Sized wrapper so a fat `Arc<dyn MemoSeam<N>>` can ride the
/// `Arc<dyn Any>` slot on `ExpandOpts` (a trait-object Arc is not
/// itself `Any`). Mirrors how `runtime` would wrap a `dyn` value.
struct SeamCell<N: Next>(super::memo_seam::DynMemoSeam<N>);

#[derive(Debug, Default, Clone)]
pub struct ExpandStats {
    pub rendered: u64,
    pub emitted: u64,
    pub terminal: u64,
    pub parked: u64,
}

static GLOBAL_TICK: AtomicU64 = AtomicU64::new(1);

fn bump_global_tick() -> ExpandTick {
    GLOBAL_TICK.fetch_add(1, Ordering::SeqCst)
}

/// Drive a single pipe instance until nothing is runnable. Pass empty
/// `seed` to resume an already-seeded queue.
pub fn expand<N: Next>(
    pipe: &PipeInstance<N>,
    queue: Arc<dyn QueueBackend<N>>,
    seed: Vec<Arc<N>>,
    opts: ExpandOpts,
) -> ExpandStats {
    let expand_tick = bump_global_tick();

    for value in seed {
        queue.enqueue(QueueRow {
            id: 0,
            parent_id: None,
            batch_idx: 0,
            path: Vec::new(),
            pipe_hash: pipe.pipe_hash,
            instance_id: pipe.instance_id,
            depth: 0,
            value,
            wake: Wake::Immediate,
            expand_tick,
            enqueued_at_ns: 0,
        });
    }

    let mut stats = ExpandStats::default();

    loop {
        let pull_t0 = std::time::Instant::now();
        let batch = queue.pull_runnable_batch_for(
            pipe.pipe_hash,
            pipe.instance_id,
            expand_tick,
            opts.batch_cap,
        );
        if let Some(telemetry) = &opts.telemetry {
            telemetry.record_pull(batch.len() as u64, pull_t0.elapsed());
        }
        if batch.is_empty() {
            if drive_barriers(pipe, queue.as_ref(), expand_tick, &opts) {
                continue;
            }
            stats.parked = queue.depth();
            break;
        }

        let head_depth = batch[0].depth;
        if head_depth as usize >= pipe.components.len() {
            stats.terminal += batch.len() as u64;
            continue;
        }

        let comp = &pipe.components[head_depth as usize];
        let mut ctx = RenderCtx::new(batch[0].pipe_hash, head_depth, expand_tick)
            .with_bus(opts.bus.clone())
            .with_diag(opts.diag.clone());
        if let Some(runtime) = &opts.runtime {
            ctx = ctx.with_runtime_any(runtime.clone());
        }
        if let Some(telemetry) = &opts.telemetry {
            ctx = ctx.with_telemetry(telemetry.clone());
        }

        // PHASE 4: the memo/replay/reconcile hook. When a `MemoSeam`
        // is installed it is probed for EVERY component (rule or not)
        // before dispatch. A non-stale `Replay` splices the stored
        // child rows downstream and SKIPS `comp.dispatch` for that
        // input (the op runs 0×). `Miss`/`Stale` rows still run; their
        // freshly-enqueued children are captured by a recording queue
        // shim and routed through `reconcile`, which performs the
        // presence-based `Retract` teardown (same semantics as the
        // prior `mounted_query` path — no mult/DRed; that is Phase 5)
        // and records the new memo entry, then hands back the rows to
        // assert downstream. With no seam this is exactly the
        // pre-Phase-4 path: every component dispatches.
        let seam: Option<super::memo_seam::DynMemoSeam<N>> = opts
            .memo_seam
            .as_ref()
            .and_then(|cell| cell.downcast_ref::<SeamCell<N>>())
            .map(|c| c.0.clone());

        let owner_id = seam.as_ref().map(|_| {
            let mut h = blake3::Hasher::new();
            h.update(&batch[0].pipe_hash.to_le_bytes());
            h.update(&batch[0].instance_id.to_le_bytes());
            h.update(&head_depth.to_le_bytes());
            h.update(comp.kind().as_bytes());
            *h.finalize().as_bytes()
        });

        let depth_before = queue.depth();
        let batch_len = batch.len() as u64;

        // Gate every tracing-related cost on whether a subscriber wants
        // it. `event_enabled!` is one atomic load when no subscriber is
        // installed; the Instant::now() and span allocation only run
        // when DEBUG events on the `expand` target are sampled.
        let trace_on = tracing::event_enabled!(
            target: "expand", tracing::Level::DEBUG
        );

        if trace_on {
            let kind = comp.kind();
            let span = tracing::debug_span!(
                target: "expand",
                "render_batch",
                op    = kind,
                depth = head_depth as u32,
                n     = batch_len,
            );
            let _g = span.enter();
            let t0 = std::time::Instant::now();
            dispatch_with_seam(
                comp.as_ref(),
                &ctx,
                &batch,
                queue.as_ref(),
                seam.as_ref(),
                owner_id,
                head_depth + 1,
                expand_tick,
            );
            let elapsed = t0.elapsed();
            if let Some(telemetry) = &opts.telemetry {
                telemetry.record_dispatch(kind, batch_len, elapsed);
            }
            let depth_after = queue.depth();
            let emitted_in_batch = depth_after.saturating_sub(depth_before);

            tracing::debug!(
                target: "expand",
                op       = kind,
                depth    = head_depth as u32,
                n        = batch_len,
                emitted  = emitted_in_batch,
                wall_us  = elapsed.as_micros() as u64,
                "batch"
            );

            stats.rendered += batch_len;
            stats.emitted += emitted_in_batch;
        } else {
            let t0 = std::time::Instant::now();
            dispatch_with_seam(
                comp.as_ref(),
                &ctx,
                &batch,
                queue.as_ref(),
                seam.as_ref(),
                owner_id,
                head_depth + 1,
                expand_tick,
            );
            if let Some(telemetry) = &opts.telemetry {
                telemetry.record_dispatch(comp.kind(), batch_len, t0.elapsed());
            }
            let depth_after = queue.depth();
            stats.rendered += batch_len;
            stats.emitted += depth_after.saturating_sub(depth_before);
        }
    }

    stats
}

/// A `QueueBackend` shim that BUFFERS every `enqueue` instead of
/// forwarding it. The seam path uses this to capture the rows a
/// `Miss`/`Stale` `dispatch` would have produced so `reconcile` can
/// decide which to actually assert downstream. Non-`enqueue` side
/// effects of a `dispatch` override (graph/store writes, dep
/// recording) still hit the real backend — only queue children are
/// intercepted. All other `QueueBackend` calls delegate to `inner`.
struct RecordingQueue<'a, N: Next> {
    inner: &'a dyn QueueBackend<N>,
    captured: std::sync::Mutex<Vec<QueueRow<N>>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl<'a, N: Next> RecordingQueue<'a, N> {
    fn new(inner: &'a dyn QueueBackend<N>) -> Self {
        Self {
            inner,
            captured: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }
    fn take(&self) -> Vec<QueueRow<N>> {
        std::mem::take(&mut *self.captured.lock().unwrap())
    }
}

impl<'a, N: Next> QueueBackend<N> for RecordingQueue<'a, N> {
    fn enqueue(&self, row: QueueRow<N>) -> super::queue::QueueId {
        // Provisional id so a dispatch that chains enqueues within one
        // call still gets a unique handle back; the real id is minted
        // when the kept rows are forwarded to `inner`.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut r = row;
        r.id = id;
        self.captured.lock().unwrap().push(r);
        id
    }
    fn pull_runnable(&self, global_tick: ExpandTick) -> Option<QueueRow<N>> {
        self.inner.pull_runnable(global_tick)
    }
    fn depth(&self) -> u64 {
        self.inner.depth()
    }
    fn dispatch_park(
        &self,
        domain: &str,
        key: Option<super::next_key::NextKey>,
    ) -> u64 {
        self.inner.dispatch_park(domain, key)
    }
    fn has_parked_domain(&self, domain: &str) -> bool {
        self.inner.has_parked_domain(domain)
    }
    fn cascade_delete(&self, root: super::queue::QueueId) -> u64 {
        self.inner.cascade_delete(root)
    }
}

/// Run one homogeneous batch through the memo seam (when installed),
/// else straight through `comp.dispatch`. See the Phase-4 comment at
/// the call site for the contract.
#[allow(clippy::too_many_arguments)]
fn dispatch_with_seam<N: Next>(
    comp: &dyn super::component::Component<Next = N>,
    ctx: &RenderCtx,
    batch: &[QueueRow<N>],
    queue: &dyn QueueBackend<N>,
    seam: Option<&super::memo_seam::DynMemoSeam<N>>,
    owner_id: Option<[u8; 32]>,
    next_depth: u32,
    expand_tick: ExpandTick,
) {
    use super::memo_seam::{MemoDelta, MemoProbe};

    let (Some(seam), Some(owner)) = (seam, owner_id) else {
        comp.dispatch(ctx, batch, queue);
        return;
    };

    // Partition by index into `batch`: REPLAY (skip dispatch) vs RUN.
    // `QueueRow`'s derived `Clone` over-constrains `N: Clone`, so the
    // run set is referenced by index, never cloned.
    let mut run_idx: Vec<usize> = Vec::new();
    let mut prior_for: Vec<Option<Vec<Arc<N>>>> = Vec::new();
    for (bi, row) in batch.iter().enumerate() {
        // Phase 6: the seam decides the memo key for this input row.
        // For a content-threaded op it is the source-keyed digest
        // (stable under edits to the read sources); otherwise the
        // default is `content_hash()` (== Phase-0 whole-cursor key).
        let in_key = seam.in_key_for(owner, row.value.as_ref());
        match seam.probe(owner, in_key) {
            MemoProbe::Replay(rows) => {
                // Pure replay: splice stored child rows downstream,
                // `comp.dispatch` is skipped for this input entirely.
                for (i, v) in rows.into_iter().enumerate() {
                    super::flatten::splice_into_at(
                        row,
                        super::node::Node::Emit(v),
                        next_depth,
                        expand_tick,
                        queue,
                        i as u32,
                    );
                }
            }
            MemoProbe::Stale(prior) => {
                run_idx.push(bi);
                prior_for.push(Some(prior));
            }
            MemoProbe::Miss => {
                run_idx.push(bi);
                prior_for.push(None);
            }
        }
    }

    if run_idx.is_empty() {
        return;
    }

    // RUN the Miss/Stale rows. `dispatch` takes a slice; rebuild a
    // contiguous run slice by borrowing the chosen `batch` rows into
    // a `Vec<&QueueRow>` is not what `dispatch` wants (it wants
    // `&[QueueRow]`). When the whole batch runs, pass it directly;
    // otherwise dispatch each chosen row singly (correct, just less
    // batched — the seam path is the cold/changed path).
    let rec = RecordingQueue::new(queue);
    if run_idx.len() == batch.len() {
        comp.dispatch(ctx, batch, &rec);
    } else {
        for &bi in &run_idx {
            comp.dispatch(ctx, std::slice::from_ref(&batch[bi]), &rec);
        }
    }
    let captured = rec.take();

    // Group captured children by the input row that produced them
    // (`parent_id` == the input row's queue id).
    for (slot, &bi) in run_idx.iter().enumerate() {
        let in_row = &batch[bi];
        let idx = slot;
        // Same Phase-6 source-keyed digest used for the probe above;
        // `reconcile` must store/diff the memo under the SAME key the
        // next render will probe with.
        let in_key = seam.in_key_for(owner, in_row.value.as_ref());
        let fresh: Vec<Arc<N>> = captured
            .iter()
            .filter(|c| c.parent_id == Some(in_row.id))
            .map(|c| c.value.clone())
            .collect();
        let prior = std::mem::take(&mut prior_for[idx]);
        let deltas = seam.reconcile(owner, in_key, prior, &fresh);
        for d in deltas {
            if let MemoDelta::Assert(i) = d {
                if let Some(v) = fresh.get(i) {
                    super::flatten::splice_into_at(
                        in_row,
                        super::node::Node::Emit(v.clone()),
                        next_depth,
                        expand_tick,
                        queue,
                        i as u32,
                    );
                }
            }
            // `Retract` is performed inside `reconcile` (presence
            // teardown, Phase 4). The driver does not act on it.
        }
    }
}

fn drive_barriers<N: Next>(
    pipe: &PipeInstance<N>,
    queue: &dyn QueueBackend<N>,
    expand_tick: ExpandTick,
    opts: &ExpandOpts,
) -> bool {
    let mut emitted = false;

    for (depth, comp) in pipe.components.iter().enumerate() {
        if comp.lifecycle() != ComponentLifecycle::Barrier {
            continue;
        }

        let depth = depth as u32;
        let scope = BarrierScope {
            pipe_hash: pipe.pipe_hash,
            instance_id: pipe.instance_id,
            expand_tick,
            depth,
        };
        let ctx = RenderCtx::new(pipe.pipe_hash, depth, expand_tick)
            .with_bus(opts.bus.clone())
            .with_diag(opts.diag.clone());
        let pending = queue.pending_summary_before_or_at(
            pipe.pipe_hash,
            pipe.instance_id,
            expand_tick,
            depth,
        );

        let before = queue.depth();
        if pending.parked > 0 {
            comp.idle(&ctx, scope, pending, queue);
        } else if pending.total() == 0 {
            comp.complete(&ctx, scope, queue);
        }
        emitted |= queue.depth() > before;
    }

    emitted
}
