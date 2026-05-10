use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use effect_runtime::v2::{
    BarrierScope, Component, ComponentLifecycle, ExpandStats, PendingSummary,
    QueueBackend, QueueRow, RenderCtx,
};
use serde::{Deserialize, Serialize};

use crate::v2_ops::{AstTelemetry, FsTelemetry};
use crate::Cursor;

#[derive(Default)]
pub struct StageTiming {
    pub name:      &'static str,
    pub calls:     AtomicU64,
    pub rows:      AtomicU64,
    pub max_batch: AtomicU64,
    pub wall_ns:   AtomicU64,
}

impl StageTiming {
    pub fn new(name: &'static str) -> Self {
        Self { name, ..Default::default() }
    }
}

pub struct TimedComponent {
    inner:  Arc<dyn Component<Next = Cursor>>,
    timing: Arc<StageTiming>,
}

impl TimedComponent {
    pub fn new(
        name: &'static str,
        inner: Arc<dyn Component<Next = Cursor>>,
        timings: &mut Vec<Arc<StageTiming>>,
    ) -> Arc<dyn Component<Next = Cursor>> {
        let timing = Arc::new(StageTiming::new(name));
        timings.push(timing.clone());
        Arc::new(Self { inner, timing })
    }
}

impl Component for TimedComponent {
    type Next = Cursor;

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let t0 = std::time::Instant::now();
        self.inner.dispatch(ctx, rows, queue);
        let dt = t0.elapsed();
        self.timing.calls.fetch_add(1, Ordering::Relaxed);
        self.timing.rows.fetch_add(rows.len() as u64, Ordering::Relaxed);
        self.timing.max_batch.fetch_max(rows.len() as u64, Ordering::Relaxed);
        self.timing.wall_ns.fetch_add(dt.as_nanos() as u64, Ordering::Relaxed);
    }

    fn batch_hint(&self) -> Option<usize> { self.inner.batch_hint() }
    fn lifecycle(&self) -> ComponentLifecycle { self.inner.lifecycle() }
    fn idle(
        &self,
        ctx:     &RenderCtx,
        scope:   BarrierScope,
        pending: PendingSummary,
        queue:   &dyn QueueBackend<Cursor>,
    ) {
        self.inner.idle(ctx, scope, pending, queue);
    }
    fn complete(
        &self,
        ctx:   &RenderCtx,
        scope: BarrierScope,
        queue: &dyn QueueBackend<Cursor>,
    ) {
        self.inner.complete(ctx, scope, queue);
    }
    fn kind(&self) -> &'static str { self.timing.name }
}

#[derive(Default)]
pub struct PipelineTelemetry {
    stages: Mutex<Vec<Arc<StageTiming>>>,
    pub fs:  Arc<FsTelemetry>,
    pub ast: Arc<AstTelemetry>,
}

impl PipelineTelemetry {
    pub fn new() -> Self {
        Self {
            stages: Mutex::new(Vec::new()),
            fs:  Arc::new(FsTelemetry::default()),
            ast: Arc::new(AstTelemetry::default()),
        }
    }

    pub fn wrap_pipe(&self, mut pipe: effect_runtime::v2::Pipe<Cursor>) -> effect_runtime::v2::Pipe<Cursor> {
        let mut timings = self.stages.lock().unwrap();
        let steps = pipe.steps.drain(..).map(|step| {
            TimedComponent::new(step.kind(), step, &mut timings)
        }).collect();
        effect_runtime::v2::Pipe::from_steps(steps)
    }

    pub fn snapshot(&self, wall: Duration, stats: ExpandStats) -> RunTelemetry {
        self.snapshot_with_phases(wall, stats, RunPhaseTelemetry::default())
    }

    pub fn snapshot_with_phases(
        &self,
        wall: Duration,
        stats: ExpandStats,
        phases: RunPhaseTelemetry,
    ) -> RunTelemetry {
        RunTelemetry {
            wall_ms: wall.as_secs_f64() * 1000.0,
            phases,
            rendered: stats.rendered,
            emitted:  stats.emitted,
            terminal: stats.terminal,
            parked:   stats.parked,
            stages: self.stages.lock().unwrap().iter().map(|s| {
                let wall_ms = s.wall_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
                let rows = s.rows.load(Ordering::Relaxed);
                StageTelemetry {
                    name: s.name.to_string(),
                    calls: s.calls.load(Ordering::Relaxed),
                    rows,
                    max_batch: s.max_batch.load(Ordering::Relaxed),
                    wall_ms,
                    rows_per_sec: rows_per_sec(rows, wall_ms),
                }
            }).collect(),
            fs: FsTelemetrySnapshot {
                seen_files:     self.fs.seen_files.load(Ordering::Relaxed),
                emitted:        self.fs.emitted.load(Ordering::Relaxed),
                ext_skipped:    self.fs.ext_skipped.load(Ordering::Relaxed),
                filter_skipped: self.fs.filter_skipped.load(Ordering::Relaxed),
            },
            ast: AstTelemetrySnapshot {
                input_rows:         self.ast.input_rows.load(Ordering::Relaxed),
                source_read_rows:   self.ast.source_read_rows.load(Ordering::Relaxed),
                source_read_bytes:  self.ast.source_read_bytes.load(Ordering::Relaxed),
                source_utf8_rows:   self.ast.source_utf8_rows.load(Ordering::Relaxed),
                source_utf8_bytes:  self.ast.source_utf8_bytes.load(Ordering::Relaxed),
                source_read_errors: self.ast.source_read_errors.load(Ordering::Relaxed),
                legacy_rows:        self.ast.legacy_rows.load(Ordering::Relaxed),
                prefilter_skips:    self.ast.prefilter_skips.load(Ordering::Relaxed),
                parses:             self.ast.parses.load(Ordering::Relaxed),
                matches:            self.ast.matches.load(Ordering::Relaxed),
                pattern_ms:         ns_to_ms(self.ast.pattern_ns.load(Ordering::Relaxed)),
                source_read_ms:     ns_to_ms(self.ast.source_read_ns.load(Ordering::Relaxed)),
                prefilter_ms:       ns_to_ms(self.ast.prefilter_ns.load(Ordering::Relaxed)),
                utf8_ms:            ns_to_ms(self.ast.utf8_ns.load(Ordering::Relaxed)),
                legacy_ms:          ns_to_ms(self.ast.legacy_ns.load(Ordering::Relaxed)),
                parse_ms:           ns_to_ms(self.ast.parse_ns.load(Ordering::Relaxed)),
                match_ms:           ns_to_ms(self.ast.match_ns.load(Ordering::Relaxed)),
                emit_stamp_ms:      ns_to_ms(self.ast.emit_stamp_ns.load(Ordering::Relaxed)),
            },
        }
    }
}

fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}

fn rows_per_sec(rows: u64, wall_ms: f64) -> f64 {
    if wall_ms <= 0.0 { 0.0 } else { rows as f64 / (wall_ms / 1000.0) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunTelemetry {
    pub wall_ms: f64,
    pub phases: RunPhaseTelemetry,
    pub rendered: u64,
    pub emitted:  u64,
    pub terminal: u64,
    pub parked:   u64,
    pub stages: Vec<StageTelemetry>,
    pub fs: FsTelemetrySnapshot,
    pub ast: AstTelemetrySnapshot,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunPhaseTelemetry {
    pub read_sprf_ms:     f64,
    pub parse_ms:         f64,
    pub lower_ms:         f64,
    pub wrap_mount_ms:    f64,
    pub expand_ms:        f64,
    pub commit_ms:        f64,
    pub resume_ms:        f64,
    pub collect_tables_ms: f64,
    pub report_ms:        f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageTelemetry {
    pub name: String,
    pub calls: u64,
    pub rows: u64,
    pub max_batch: u64,
    pub wall_ms: f64,
    pub rows_per_sec: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FsTelemetrySnapshot {
    pub seen_files:     u64,
    pub emitted:        u64,
    pub ext_skipped:    u64,
    pub filter_skipped: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AstTelemetrySnapshot {
    pub input_rows:         u64,
    pub source_read_rows:   u64,
    pub source_read_bytes:  u64,
    pub source_utf8_rows:   u64,
    pub source_utf8_bytes:  u64,
    pub source_read_errors: u64,
    pub legacy_rows:        u64,
    pub prefilter_skips:    u64,
    pub parses:             u64,
    pub matches:            u64,
    pub pattern_ms:         f64,
    pub source_read_ms:     f64,
    pub prefilter_ms:       f64,
    pub utf8_ms:            f64,
    pub legacy_ms:          f64,
    pub parse_ms:           f64,
    pub match_ms:           f64,
    pub emit_stamp_ms:      f64,
}
