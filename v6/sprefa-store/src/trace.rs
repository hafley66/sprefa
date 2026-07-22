//! The ONE measurement harness: plain `tracing`, collected like normal people.
//!
//! Instrument code with run-of-the-mill tracing — `tracing::info_span!("phase")`
//! for the code that ran (and its timing), `tracing::info!(rss_kb = …)` for a
//! metric sample. Wrap the workload in [`collect`] and get back every span
//! (name + elapsed) and event (fields) that fired. No `dlsym`, no sqlite3-CLI
//! shell-out, no bespoke sensor plumbing — a small `tracing_subscriber::Layer`
//! into a `Vec`.
//!
//! Async note: use a current-thread runtime inside the `collect` closure so
//! every `.await` stays on this thread and is captured (the subscriber default
//! is thread-local):
//!
//! ```ignore
//! let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
//! let (out, records) = trace::collect(|| rt.block_on(async { store.commit(&d).await }));
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

/// A span closing (code that ran + how long) or an event (a metric sample).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Span,
    Event,
}

/// One collected tracing record.
#[derive(Clone, Debug)]
pub struct Record {
    pub kind: Kind,
    pub target: String,
    /// span name (the code), or the event's callsite name.
    pub name: String,
    /// event fields, or span fields captured at open.
    pub fields: BTreeMap<String, String>,
    /// wall time inside the span; 0 for an event.
    pub elapsed_ns: u64,
}

impl Record {
    /// Convenience: parse a field as i64 (metrics are usually integers).
    pub fn i64(&self, key: &str) -> Option<i64> {
        self.fields.get(key).and_then(|v| v.parse().ok())
    }
}

#[derive(Default)]
struct FieldVisitor(BTreeMap<String, String>);

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_string(), format!("{value:?}"));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

/// Per-span state stashed in the registry's span extensions.
struct SpanState {
    started: Instant,
    fields: BTreeMap<String, String>,
}

struct CollectLayer {
    out: Arc<Mutex<Vec<Record>>>,
}

impl<S> Layer<S> for CollectLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut visitor = FieldVisitor::default();
            attrs.record(&mut visitor);
            span.extensions_mut().insert(SpanState {
                started: Instant::now(),
                fields: visitor.0,
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        self.out.lock().unwrap().push(Record {
            kind: Kind::Event,
            target: event.metadata().target().to_string(),
            name: event.metadata().name().to_string(),
            fields: visitor.0,
            elapsed_ns: 0,
        });
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            let extensions = span.extensions();
            let (elapsed_ns, fields) = match extensions.get::<SpanState>() {
                Some(state) => (state.started.elapsed().as_nanos() as u64, state.fields.clone()),
                None => (0, BTreeMap::new()),
            };
            self.out.lock().unwrap().push(Record {
                kind: Kind::Span,
                target: span.metadata().target().to_string(),
                name: span.name().to_string(),
                fields,
                elapsed_ns,
            });
        }
    }
}

/// Run `body` with a collecting subscriber installed as the thread-local
/// default, and return its result plus every span/event emitted on this thread.
/// Scoped (via `with_default`), so it composes and can run repeatedly and in
/// parallel across threads — the one measured path.
pub fn collect<T>(body: impl FnOnce() -> T) -> (T, Vec<Record>) {
    let out = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(CollectLayer { out: out.clone() });
    let result = tracing::subscriber::with_default(subscriber, body);
    let records = std::mem::take(&mut *out.lock().unwrap());
    (result, records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_span_timing_and_event_fields() {
        let (out, records) = collect(|| {
            let span = tracing::info_span!("op", workload = "DAG");
            let _guard = span.enter();
            tracing::info!(target: "measure", rss_kb = 123i64, disk_read = 4096i64, "sample");
            42
        });

        assert_eq!(out, 42);

        let event = records
            .iter()
            .find(|r| r.kind == Kind::Event)
            .expect("an event was collected");
        assert_eq!(event.i64("rss_kb"), Some(123));
        assert_eq!(event.i64("disk_read"), Some(4096));

        let span = records
            .iter()
            .find(|r| r.kind == Kind::Span && r.name == "op")
            .expect("the op span was collected");
        assert_eq!(span.fields.get("workload").map(String::as_str), Some("DAG"));
    }

    #[test]
    fn empty_when_nothing_is_instrumented() {
        let (out, records) = collect(|| 7);
        assert_eq!(out, 7);
        assert!(records.is_empty());
    }
}
