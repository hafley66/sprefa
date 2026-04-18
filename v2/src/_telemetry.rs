//! Periscope — tracing subscribers.
//!
//! Two switches, independent:
//!
//! - `SPREFA_TRACE=/path/trace.json` → Chrome trace via tracing-chrome.
//!   Open in ui.perfetto.dev for flame view.
//! - `SPREFA_SPAN_LOG=1` → one stderr line per span close, in order, with
//!   start offset and duration:
//!       `[+123ms  45ms]   reader.bytes {repo=swc rev=HEAD path=wt fp=...}`
//!   Indentation = depth in span stack. Shows the real sequence of work.
//!
//! Fmt layer and chrome layer can both be installed on top of the span logger.
//! The returned guard holds the chrome FlushGuard when present — drop on exit.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use tracing::{span, Subscriber};
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::{prelude::*, EnvFilter, fmt, Layer};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

pub struct Periscope {
    _flush: Option<FlushGuard>,
}

/// Per-span timing + field capture. Stored in span extensions so nested
/// spans track their own wall independently.
struct Timed {
    start:  Instant,
    depth:  usize,
    fields: String,
}

struct FieldVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for FieldVisitor<'a> {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        let _ = write!(self.0, " {}={}", f.name(), v);
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        let _ = write!(self.0, " {}={}", f.name(), v);
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        let _ = write!(self.0, " {}={}", f.name(), v);
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        let _ = write!(self.0, " {}={}", f.name(), v);
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        let _ = write!(self.0, " {}={:?}", f.name(), v);
    }
}

/// Stderr span logger. Writes one line per span close in the order spans
/// close, which matches "order work finished". Pair with `op.*` + `reader.*`
/// spans to see the actual sequence.
struct SpanLog {
    epoch: Instant,
    /// Serialize writes so lines from different tasks don't interleave.
    out:   Mutex<std::io::Stderr>,
}

impl<S> Layer<S> for SpanLog
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = match ctx.span(id) { Some(s) => s, None => return };
        let depth = ctx.span_scope(id).map(|s| s.count()).unwrap_or(1);
        let mut fields = String::new();
        attrs.record(&mut FieldVisitor(&mut fields));
        span.extensions_mut().insert(Timed {
            start: Instant::now(),
            depth,
            fields,
        });
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let span = match ctx.span(id) { Some(s) => s, None => return };
        let mut ext = span.extensions_mut();
        if let Some(timed) = ext.get_mut::<Timed>() {
            values.record(&mut FieldVisitor(&mut timed.fields));
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        let span = match ctx.span(&id) { Some(s) => s, None => return };
        let ext = span.extensions();
        let Some(t) = ext.get::<Timed>() else { return };
        let offset_ms = self.epoch.elapsed().as_millis() as i64 - t.start.elapsed().as_millis() as i64;
        let dur_ms    = t.start.elapsed().as_millis();
        let indent    = "  ".repeat(t.depth.saturating_sub(1));
        let name      = span.metadata().name();
        use std::io::Write as _;
        let mut w = self.out.lock().unwrap();
        let _ = writeln!(
            w,
            "[+{:>6}ms {:>5}ms] {}{}{}",
            offset_ms, dur_ms, indent, name, t.fields,
        );
    }
}

/// Initialize the global tracing subscriber. Safe to call once per process.
pub fn init() -> Periscope {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hyper=warn,axum=info"));

    let fmt_layer = fmt::layer().with_writer(std::io::stderr);

    let span_log_on = matches!(
        std::env::var("SPREFA_SPAN_LOG").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    );
    let span_log_layer = span_log_on.then(|| SpanLog {
        epoch: Instant::now(),
        out:   Mutex::new(std::io::stderr()),
    });

    let trace_path: Option<PathBuf> = std::env::var_os("SPREFA_TRACE")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty());

    let (chrome_layer, flush) = match trace_path {
        Some(path) => {
            let (layer, guard) = ChromeLayerBuilder::new()
                .file(path)
                .include_args(true)
                .build();
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(span_log_layer)
        .with(chrome_layer)
        .try_init();

    Periscope { _flush: flush }
}
