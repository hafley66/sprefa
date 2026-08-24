//! Span vocabulary for the crate. Every field is a value the extraction already
//! holds, never a parsed string. The subscriber install is `cli`-gated: a
//! library that installs a global subscriber steals the choice from its caller.

use tracing::field::Empty;
use tracing::Span;

use crate::rows::FamilyBundle;
use crate::types::Family;

/// One backing engine's parse over one file.
pub fn parse_span(lang: &'static str, engine: &'static str) -> Span {
    tracing::debug_span!("parse", lang, engine)
}

/// One family's projection off an already-parsed tree. `nodes`, `edges` and
/// `sites` land through `record_bundle` when the projection returns.
pub fn family_span(lang: &'static str, family: &'static str) -> Span {
    tracing::debug_span!(
        "family",
        lang,
        family,
        nodes = Empty,
        edges = Empty,
        sites = Empty
    )
}

/// The counts a projection produced. `sites` is the family's side-channel row
/// count (CallF call sites); families with no side channel pass 0.
pub fn record_bundle<F: Family>(span: &Span, bundle: &FamilyBundle<F>, sites: usize) {
    span.record("nodes", bundle.nodes.len() as u64);
    span.record("edges", bundle.edges.len() as u64);
    span.record("sites", sites as u64);
}

#[cfg(feature = "cli")]
pub use sink::{install, SummaryLayer, SummaryState};

#[cfg(feature = "cli")]
mod sink {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::Subscriber;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::registry::LookupSpan;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{filter::EnvFilter, fmt, Registry};

    /// One (lang, family) row of the exit table.
    #[derive(Default)]
    struct Row {
        micros: u128,
        files: u64,
        facts: u64,
    }

    /// Shared rather than dropped-at-exit: a `Layer` inside a `Registry` has no
    /// drop the process can rely on.
    pub struct SummaryState {
        rows: Mutex<BTreeMap<(String, String), Row>>,
        start: Instant,
    }

    impl Default for SummaryState {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SummaryState {
        pub fn new() -> Self {
            Self {
                rows: Mutex::new(BTreeMap::new()),
                start: Instant::now(),
            }
        }

        fn fold(&self, lang: String, family: String, busy: Duration, facts: u64) {
            let mut rows = self.rows.lock().expect("summary rows");
            let row = rows.entry((lang, family)).or_default();
            row.micros += busy.as_micros();
            row.files += 1;
            row.facts += facts;
        }

        /// The table, wall descending. Empty string when nothing was recorded.
        pub fn render(&self) -> String {
            let rows = self.rows.lock().expect("summary rows");
            if rows.is_empty() {
                return String::new();
            }
            let mut ordered: Vec<(&(String, String), &Row)> = rows.iter().collect();
            ordered.sort_by(|left, right| {
                right
                    .1
                    .micros
                    .cmp(&left.1.micros)
                    .then_with(|| left.0.cmp(right.0))
            });
            let mut text = String::new();
            let _ = writeln!(
                text,
                "extract summary: wall {:.1}ms",
                self.start.elapsed().as_secs_f64() * 1000.0
            );
            let _ = writeln!(
                text,
                "{:<10} {:<20} {:>12} {:>8} {:>10}",
                "lang", "family", "us", "files", "facts"
            );
            for ((lang, family), row) in ordered {
                let _ = writeln!(
                    text,
                    "{lang:<10} {family:<20} {:>12} {:>8} {:>10}",
                    row.micros, row.files, row.facts
                );
            }
            text
        }

        /// Render to stderr, if anything was recorded.
        pub fn print(&self) {
            let text = self.render();
            if text.is_empty() {
                return;
            }
            let mut stderr = std::io::stderr().lock();
            // @eprintln-ok: trace summary sink
            let _ = write!(stderr, "{text}");
        }
    }

    /// Per-span accumulator, parked in the span's extensions.
    struct SpanCounts {
        lang: String,
        name: &'static str,
        family: Option<String>,
        facts: u64,
        busy: Duration,
        entered: Option<Instant>,
    }

    impl SpanCounts {
        /// The table's family axis. A span that is not the per-family projection
        /// keeps its own name in the key, so `resolve_arm`'s `call` never folds
        /// into the projection's `call`.
        fn axis(&self) -> String {
            match (&self.family, self.name) {
                (Some(family), "family") => family.clone(),
                (Some(family), name) => format!("{name}:{family}"),
                (None, name) => name.to_string(),
            }
        }
    }

    struct CountVisitor<'a>(&'a mut SpanCounts);

    impl Visit for CountVisitor<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "lang" => self.0.lang = value.to_string(),
                "family" => self.0.family = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            if matches!(field.name(), "nodes" | "edges" | "facts" | "specifiers") {
                self.0.facts += value;
            }
        }

        fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
    }

    /// Folds every span this crate opens into `SummaryState`, keyed on the span's
    /// `lang` field and its `family` field (its name, when it declares none).
    pub struct SummaryLayer {
        state: Arc<SummaryState>,
    }

    impl SummaryLayer {
        pub fn new(state: Arc<SummaryState>) -> Self {
            Self { state }
        }
    }

    impl<S> Layer<S> for SummaryLayer
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
            let mut counts = SpanCounts {
                lang: "-".to_string(),
                name: attributes.metadata().name(),
                family: None,
                facts: 0,
                busy: Duration::ZERO,
                entered: None,
            };
            attributes.record(&mut CountVisitor(&mut counts));
            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(counts);
            }
        }

        fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
            if let Some(span) = ctx.span(id) {
                if let Some(counts) = span.extensions_mut().get_mut::<SpanCounts>() {
                    values.record(&mut CountVisitor(counts));
                }
            }
        }

        fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
            if let Some(span) = ctx.span(id) {
                if let Some(counts) = span.extensions_mut().get_mut::<SpanCounts>() {
                    counts.entered = Some(Instant::now());
                }
            }
        }

        fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
            if let Some(span) = ctx.span(id) {
                if let Some(counts) = span.extensions_mut().get_mut::<SpanCounts>() {
                    if let Some(entered) = counts.entered.take() {
                        counts.busy += entered.elapsed();
                    }
                }
            }
        }

        fn on_close(&self, id: Id, ctx: Context<'_, S>) {
            let Some(span) = ctx.span(&id) else { return };
            let mut extensions = span.extensions_mut();
            let Some(counts) = extensions.remove::<SpanCounts>() else {
                return;
            };
            let axis = counts.axis();
            self.state
                .fold(counts.lang, axis, counts.busy, counts.facts);
        }
    }

    /// stdout is the fact stream and is diffed byte for byte, so every span goes
    /// to stderr, and stderr stays empty unless RUST_LOG or DL_TRACE_SUMMARY asks.
    pub fn install() -> Option<Arc<SummaryState>> {
        let want_summary = matches!(std::env::var("DL_TRACE_SUMMARY").as_deref(), Ok("1"));
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        let format = std::env::var("HAFLEY_LOG_FORMAT").unwrap_or_else(|_| "human".to_string());
        let printer = match format.as_str() {
            "human" | "text" => fmt::layer()
                .with_writer(std::io::stderr)
                .with_span_events(fmt::format::FmtSpan::CLOSE)
                .with_filter(filter)
                .boxed(),
            "json" => fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_span_events(fmt::format::FmtSpan::CLOSE)
                .with_filter(filter)
                .boxed(),
            value => panic!("unknown HAFLEY_LOG_FORMAT {value:?}; expected human or json"),
        };
        let (summary, state) = if want_summary {
            let state = Arc::new(SummaryState::new());
            let layer = SummaryLayer::new(Arc::clone(&state))
                .with_filter(EnvFilter::new("sprefa_extract=debug"));
            (Some(layer), Some(state))
        } else {
            (None, None)
        };
        Registry::default().with(printer).with(summary).init();
        tracing::debug!(
            service.name = "sprefa-extract",
            service.version = env!("CARGO_PKG_VERSION"),
            process.pid = std::process::id(),
            log.format = format,
            "observability initialized"
        );
        state
    }
}
