//! Core-driven telemetry for the v3 effect surface.
//!
//! Every `ctx.put(E).await` opens a span at entry and closes it at
//! reply. The span captures: effect type name, wall time, optional
//! payload size, optional response size. Op authors override the
//! optional size hints on `EffectKind` to get throughput accounting;
//! the rest comes for free.
//!
//! One `Telemetry` lives per `RtCtx`. Access via `ctx.telemetry()` to
//! drain or summarize. Spans are appended under a `Mutex<Vec<Span>>`;
//! bench volume (tens of thousands of puts) is well within that
//! contention budget. Production mode would swap in a per-worker
//! thread-local ring buffer with periodic merge.
//!
//! Surfaces this feeds:
//! - `summary()` prints a per-effect table: count / p50 / p95 / p99 /
//!   MB/s / bytes/s. Same shape as the tables in FINDINGS.md §5, §6.
//! - `drain()` yields raw spans for downstream use (flame export,
//!   sqlite persist, diff between runs).
//! - `report_by_effect()` groups spans by effect type and returns
//!   structured `EffectReport` records.

use std::any::type_name;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// One observation of a single `ctx.put(E).await` call.
#[derive(Clone, Debug)]
pub struct Span {
    /// `std::any::type_name::<E>()` captured at put time.
    pub effect_type: &'static str,
    /// Put entry time, ns since the ctx's Telemetry epoch. Enables
    /// throughput rollups over the wall window (max_end − min_start)
    /// rather than the sum of per-span walls, which overcounts when
    /// puts overlap.
    pub start_ns: u64,
    /// Monotonic wall time for the whole put → response roundtrip.
    /// This is queue_wait + handler_wall as observed from the caller.
    pub wall_ns: u64,
    /// Payload size reported by `EffectKind::payload_bytes` (if the
    /// op author overrode the default). Enables throughput rollups
    /// (MB/s) without the op author writing any measurement code.
    pub payload_bytes: Option<usize>,
    /// Response size reported by `EffectKind::response_bytes`.
    pub response_bytes: Option<usize>,
}

/// Per-context telemetry sink. Cloned `Arc`-cheap; spans accumulate
/// under a `Mutex`. Call `clear()` between trials to reset.
#[derive(Clone)]
pub struct Telemetry {
    inner: Arc<Mutex<Vec<Span>>>,
    /// Epoch for relative timestamps. Reset on `clear()` so each
    /// trial computes throughput relative to its own start.
    epoch: Arc<Mutex<Instant>>,
}

impl Default for Telemetry {
    fn default() -> Self { Self::new() }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(1 << 14))),
            epoch: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// Frame entry. Returns a guard that closes the span on drop
    /// or on explicit close(). The caller fills response_bytes on
    /// close when it has the response in hand.
    pub fn start<E: super::EffectKind>(&self, payload_bytes: Option<usize>) -> SpanOpen {
        let epoch = *self.epoch.lock().unwrap();
        let now = Instant::now();
        SpanOpen {
            effect_type: type_name::<E>(),
            started: now,
            start_ns: now.saturating_duration_since(epoch).as_nanos() as u64,
            payload_bytes,
            sink: self.inner.clone(),
        }
    }

    /// Snapshot the current spans without consuming them.
    pub fn snapshot(&self) -> Vec<Span> {
        self.inner.lock().unwrap().clone()
    }

    /// Drain and return all spans; leaves the buffer empty.
    pub fn drain(&self) -> Vec<Span> {
        std::mem::take(&mut *self.inner.lock().unwrap())
    }

    /// Reset without returning. Resets the epoch too so the next
    /// trial's throughput window starts at zero.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
        *self.epoch.lock().unwrap() = Instant::now();
    }

    /// Group spans by effect type and compute p50 / p95 / p99 walls
    /// plus throughput rollups when size hints were available.
    pub fn report_by_effect(&self) -> Vec<EffectReport> {
        let spans = self.inner.lock().unwrap();
        let mut by_type: HashMap<&'static str, Vec<&Span>> = HashMap::new();
        for s in spans.iter() {
            by_type.entry(s.effect_type).or_default().push(s);
        }
        let mut out: Vec<EffectReport> = by_type
            .into_iter()
            .map(|(ty, v)| EffectReport::from_spans(ty, &v))
            .collect();
        out.sort_by(|a, b| b.total_wall_ns.cmp(&a.total_wall_ns));
        out
    }

    /// Human-readable summary. Printed to stderr at end of a run.
    /// Shape mirrors FINDINGS.md §5 / §6 tables. `MB/s_wall` uses
    /// the true wall window (earliest span start to latest span end)
    /// so concurrent puts report aggregate throughput instead of the
    /// over-counted sum of per-span walls.
    pub fn summary(&self) -> String {
        let reports = self.report_by_effect();
        let mut s = String::with_capacity(1024);
        s.push_str(&format!(
            "{:<28} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>12} {:>12}\n",
            "effect", "count", "p50", "p95", "p99", "mean",
            "total_MB", "wall", "MB/s_wall",
        ));
        s.push_str(&format!(
            "{:<28} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>12} {:>12}\n",
            "-".repeat(28), "-----", "---", "---", "---", "----",
            "--------", "----", "---------",
        ));
        for r in &reports {
            let mb_total = r.total_bytes.map(|b| b as f64 / 1_048_576.0);
            let wall_s = r.wall_window_ns as f64 / 1e9;
            let mb_s = if r.wall_window_ns > 0 {
                mb_total.map(|mb| mb / wall_s)
            } else {
                None
            };
            s.push_str(&format!(
                "{:<28} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12} {:>12} {:>12}\n",
                short_type(r.effect_type),
                r.count,
                fmt_ns(r.p50_ns),
                fmt_ns(r.p95_ns),
                fmt_ns(r.p99_ns),
                fmt_ns(r.mean_ns),
                mb_total.map(|m| format!("{:.1}", m)).unwrap_or("—".into()),
                fmt_ns(r.wall_window_ns),
                mb_s.map(|m| format!("{:.1}", m)).unwrap_or("—".into()),
            ));
        }
        s
    }
}

/// Live span, closes on drop. Close explicitly with `close()` when
/// the response is in hand to record response_bytes.
pub struct SpanOpen {
    effect_type: &'static str,
    started: Instant,
    start_ns: u64,
    payload_bytes: Option<usize>,
    sink: Arc<Mutex<Vec<Span>>>,
}

impl SpanOpen {
    /// Close the span with response size filled in.
    pub fn close(self, response_bytes: Option<usize>) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let span = Span {
            effect_type: self.effect_type,
            start_ns: self.start_ns,
            wall_ns,
            payload_bytes: self.payload_bytes,
            response_bytes,
        };
        self.sink.lock().unwrap().push(span);
        std::mem::forget(self); // skip Drop's duplicate close path
    }
}

impl Drop for SpanOpen {
    fn drop(&mut self) {
        // Close path for puts that panic or get dropped without an
        // explicit close() — still records wall time, leaves response
        // bytes empty.
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let span = Span {
            effect_type: self.effect_type,
            start_ns: self.start_ns,
            wall_ns,
            payload_bytes: self.payload_bytes,
            response_bytes: None,
        };
        if let Ok(mut v) = self.sink.lock() {
            v.push(span);
        }
    }
}

/// Rollup per effect type.
#[derive(Debug, Clone)]
pub struct EffectReport {
    pub effect_type: &'static str,
    pub count: usize,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub mean_ns: u64,
    /// Sum of per-span walls (for sequentialized-total reasoning).
    pub total_wall_ns: u64,
    /// Actual wall window spanned by this effect type's puts:
    /// latest_close − earliest_open. Used for concurrent-aware MB/s.
    pub wall_window_ns: u64,
    /// Sum of payload + response bytes across spans that supplied
    /// either hint. None when no span of this type reported sizes.
    pub total_bytes: Option<u64>,
}

impl EffectReport {
    fn from_spans(effect_type: &'static str, spans: &[&Span]) -> Self {
        let count = spans.len();
        let mut walls: Vec<u64> = spans.iter().map(|s| s.wall_ns).collect();
        walls.sort_unstable();
        let p = |q: f64| -> u64 {
            if walls.is_empty() { return 0; }
            let i = ((walls.len() - 1) as f64 * q).round() as usize;
            walls[i]
        };
        let sum_wall: u64 = walls.iter().sum();
        let mean_ns = if count > 0 { sum_wall / count as u64 } else { 0 };
        // Wall window: latest close (start + wall) minus earliest
        // open. This is the wall-clock duration during which puts of
        // this effect type were in flight. Correct throughput metric
        // even when puts overlap.
        let earliest_start = spans.iter().map(|s| s.start_ns).min().unwrap_or(0);
        let latest_end = spans
            .iter()
            .map(|s| s.start_ns.saturating_add(s.wall_ns))
            .max()
            .unwrap_or(0);
        let wall_window_ns = latest_end.saturating_sub(earliest_start);
        // Attributed bytes = payload + response, whichever the op
        // chose to report. Ops that care about input throughput
        // (writes) override `payload_bytes`; ops that care about
        // output throughput (reads) override `response_bytes`. The
        // sum captures both accounting flavors with one column.
        let mut total_bytes: Option<u64> = None;
        for s in spans {
            if let Some(b) = s.payload_bytes {
                total_bytes = Some(total_bytes.unwrap_or(0) + b as u64);
            }
            if let Some(b) = s.response_bytes {
                total_bytes = Some(total_bytes.unwrap_or(0) + b as u64);
            }
        }
        Self {
            effect_type,
            count,
            p50_ns: p(0.50),
            p95_ns: p(0.95),
            p99_ns: p(0.99),
            mean_ns,
            total_wall_ns: sum_wall,
            wall_window_ns,
            total_bytes,
        }
    }
}

// Printing helpers.
fn fmt_ns(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.2}s", ns as f64 / 1e9)
    } else if ns >= 1_000_000 {
        format!("{:.1}ms", ns as f64 / 1e6)
    } else if ns >= 1_000 {
        format!("{:.1}µs", ns as f64 / 1e3)
    } else {
        format!("{}ns", ns)
    }
}

fn short_type(full: &'static str) -> String {
    // type_name returns fully-qualified paths. Keep the last segment
    // so reports stay readable.
    full.rsplit("::").next().unwrap_or(full).to_string()
}
