//! Span vocabulary for the crate. Every field is a value the extraction already
//! holds, never a parsed string. The subscriber install is `cli`-gated: a
//! library that installs a global subscriber steals the choice from its caller.

use tracing::field::Empty;
use tracing::Span;

use crate::rows::FamilyBundle;
use crate::types::Family;

/// One leg of the extraction. CLOSED: a phase off this list is a compile error
/// rather than a string a caller invents, so the table's axis cannot drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Hash,
    Parse,
    Family,
    BindPlan,
    Chain,
    TsiSyntax,
    TsiSemantic,
    Flatten,
    Write,
    ResolveLeg,
}

impl Phase {
    /// The name the span field and the phase table carry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Phase::Hash => "hash",
            Phase::Parse => "parse",
            Phase::Family => "family",
            Phase::BindPlan => "bind_plan",
            Phase::Chain => "chain",
            Phase::TsiSyntax => "tsi_syntax",
            Phase::TsiSemantic => "tsi_semantic",
            Phase::Flatten => "flatten",
            Phase::Write => "write",
            Phase::ResolveLeg => "resolve_leg",
        }
    }

    /// The inverse, for a subscriber folding a recorded field back to the axis.
    pub fn from_name(name: &str) -> Option<Phase> {
        let phase = match name {
            "hash" => Phase::Hash,
            "parse" => Phase::Parse,
            "family" => Phase::Family,
            "bind_plan" => Phase::BindPlan,
            "chain" => Phase::Chain,
            "tsi_syntax" => Phase::TsiSyntax,
            "tsi_semantic" => Phase::TsiSemantic,
            "flatten" => Phase::Flatten,
            "write" => Phase::Write,
            "resolve_leg" => Phase::ResolveLeg,
            _ => return None,
        };
        Some(phase)
    }
}

/// One phase over one file, or one leg over one project. `bytes`, `rows` and
/// `calls` land through [`record_phase`]; unset reads 0 in the table.
pub fn phase_span(lang: &'static str, phase: Phase) -> Span {
    tracing::debug_span!(
        "phase",
        lang,
        phase = phase.as_str(),
        bytes = Empty,
        rows = Empty,
        calls = Empty
    )
}

/// The counts a phase produced. Integers only: no formatting on a hot path.
pub fn record_phase(span: &Span, bytes: u64, rows: u64, calls: u64) {
    span.record("bytes", bytes);
    span.record("rows", rows);
    span.record("calls", calls);
}

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
pub use sink::{
    install, load_avg_1min, FamilyRow, PhaseRowOut, RunSnapshot, SummaryLayer, SummaryState,
};

#[cfg(feature = "cli")]
mod sink {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant, SystemTime};

    use super::Phase;

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::Subscriber;
    use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
    use tracing_subscriber::registry::{LookupSpan, SpanRef};
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{filter::EnvFilter, Registry};

    /// One (lang, family) row of the exit table.
    #[derive(Default)]
    struct Row {
        micros: u128,
        files: u64,
        facts: u64,
    }

    /// One (lang, phase) row of the phase table. `files` counts span entries,
    /// so a leg entered twice per file reads 2 whatever the machine's load.
    #[derive(Default)]
    struct PhaseRow {
        micros: u128,
        files: u64,
        calls: u64,
        rows: u64,
        bytes: u64,
    }

    /// One (lang, family) row as the trail writes it.
    pub struct FamilyRow {
        pub lang: String,
        pub family: String,
        pub micros: u128,
        pub files: u64,
        pub facts: u64,
    }

    /// One (lang, phase) row as the trail writes it.
    pub struct PhaseRowOut {
        pub lang: String,
        pub phase: Phase,
        pub micros: u128,
        pub files: u64,
        pub calls: u64,
        pub rows: u64,
        pub bytes: u64,
    }

    /// Everything one run puts on disk, read out under each lock exactly once.
    pub struct RunSnapshot {
        pub started: SystemTime,
        pub wall: Duration,
        pub load_start: f64,
        pub load_end: f64,
        pub families: Vec<FamilyRow>,
        pub phases: Vec<PhaseRowOut>,
    }

    /// The 1-minute load average, 0.0 where the platform will not report it.
    pub fn load_avg_1min() -> f64 {
        let mut avg = [0f64; 3];
        // SAFETY: getloadavg fills at most `nelem` entries of the caller's array.
        let filled = unsafe { libc::getloadavg(avg.as_mut_ptr(), 3) };
        if filled >= 1 {
            avg[0]
        } else {
            0.0
        }
    }

    /// Shared rather than dropped-at-exit: a `Layer` inside a `Registry` has no
    /// drop the process can rely on.
    pub struct SummaryState {
        rows: Mutex<BTreeMap<(String, String), Row>>,
        phases: Mutex<BTreeMap<(String, Phase), PhaseRow>>,
        start: Instant,
        started: SystemTime,
        load_start: f64,
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
                phases: Mutex::new(BTreeMap::new()),
                start: Instant::now(),
                started: SystemTime::now(),
                load_start: load_avg_1min(),
            }
        }

        fn fold(&self, lang: String, family: String, busy: Duration, facts: u64) {
            let mut rows = self.rows.lock().expect("summary rows");
            let row = rows.entry((lang, family)).or_default();
            row.micros += busy.as_micros();
            row.files += 1;
            row.facts += facts;
        }

        fn fold_phase(
            &self,
            lang: String,
            phase: Phase,
            busy: Duration,
            counts: (u64, u64, u64),
        ) {
            let mut phases = self.phases.lock().expect("summary phases");
            let row = phases.entry((lang, phase)).or_default();
            row.micros += busy.as_micros();
            row.files += 1;
            row.calls += counts.0;
            row.rows += counts.1;
            row.bytes += counts.2;
        }

        /// Everything the trail writes, borrowed out under each lock once.
        pub fn snapshot(&self) -> RunSnapshot {
            let families = {
                let rows = self.rows.lock().expect("summary rows");
                rows.iter()
                    .map(|((lang, family), row)| FamilyRow {
                        lang: lang.clone(),
                        family: family.clone(),
                        micros: row.micros,
                        files: row.files,
                        facts: row.facts,
                    })
                    .collect()
            };
            let phases = {
                let phases = self.phases.lock().expect("summary phases");
                phases
                    .iter()
                    .map(|((lang, phase), row)| PhaseRowOut {
                        lang: lang.clone(),
                        phase: *phase,
                        micros: row.micros,
                        files: row.files,
                        calls: row.calls,
                        rows: row.rows,
                        bytes: row.bytes,
                    })
                    .collect()
            };
            RunSnapshot {
                started: self.started,
                wall: self.start.elapsed(),
                load_start: self.load_start,
                load_end: load_avg_1min(),
                families,
                phases,
            }
        }

        /// The phase half of the table, wall descending. Empty when no phase ran.
        fn render_phases(&self) -> String {
            let phases = self.phases.lock().expect("summary phases");
            if phases.is_empty() {
                return String::new();
            }
            let mut ordered: Vec<(&(String, Phase), &PhaseRow)> = phases.iter().collect();
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
                "extract phases: load {:.2} -> {:.2}",
                self.load_start,
                load_avg_1min()
            );
            let _ = writeln!(
                text,
                "{:<10} {:<14} {:>8} {:>10} {:>12} {:>14} {:>12}",
                "lang", "phase", "files", "calls", "rows", "bytes", "us"
            );
            for ((lang, phase), row) in ordered {
                let _ = writeln!(
                    text,
                    "{lang:<10} {:<14} {:>8} {:>10} {:>12} {:>14} {:>12}",
                    phase.as_str(),
                    row.files,
                    row.calls,
                    row.rows,
                    row.bytes,
                    row.micros
                );
            }
            text
        }

        /// Both tables, family first, separated by one blank line. Empty string
        /// when nothing was recorded.
        pub fn render(&self) -> String {
            let mut text = self.render_families();
            let phases = self.render_phases();
            if phases.is_empty() {
                return text;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&phases);
            text
        }

        /// The family table, wall descending.
        fn render_families(&self) -> String {
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
        phase: Option<Phase>,
        facts: u64,
        calls: u64,
        rows: u64,
        bytes: u64,
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
                "phase" => self.0.phase = Phase::from_name(value),
                _ => {}
            }
        }

        fn record_u64(&mut self, field: &Field, value: u64) {
            match field.name() {
                "nodes" | "edges" | "facts" | "specifiers" => self.0.facts += value,
                "calls" => self.0.calls += value,
                "rows" => self.0.rows += value,
                "bytes" if self.0.phase.is_some() => self.0.bytes += value,
                _ => {}
            }
        }

        fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
    }

    /// The nearest enclosing span's `lang`. A phase below the language door
    /// (the content hash, the flatten) names none of its own.
    fn inherited_lang<S>(span: &SpanRef<'_, S>) -> Option<String>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        span.scope().skip(1).find_map(|parent| {
            let extensions = parent.extensions();
            extensions
                .get::<SpanCounts>()
                .map(|counts| counts.lang.clone())
                .filter(|lang| lang != "-")
        })
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
                phase: None,
                facts: 0,
                calls: 0,
                rows: 0,
                bytes: 0,
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
            drop(extensions);
            if let Some(phase) = counts.phase {
                let tallies = (counts.calls, counts.rows, counts.bytes);
                let lang = if counts.lang == "-" {
                    inherited_lang(&span).unwrap_or(counts.lang)
                } else {
                    counts.lang
                };
                self.state.fold_phase(lang, phase, counts.busy, tallies);
                return;
            }
            // `parse` and `family` predate the phase axis, so they feed both
            // tables: the family one keeps its shape, the phase one is complete.
            match counts.name {
                "parse" => self.state.fold_phase(
                    counts.lang.clone(),
                    Phase::Parse,
                    counts.busy,
                    (1, 0, 0),
                ),
                "family" => self.state.fold_phase(
                    counts.lang.clone(),
                    Phase::Family,
                    counts.busy,
                    (1, counts.facts, 0),
                ),
                _ => {}
            }
            let axis = counts.axis();
            self.state
                .fold(counts.lang, axis, counts.busy, counts.facts);
        }
    }

    /// stdout is the fact stream and is diffed byte for byte, so every span goes
    /// to stderr, and stderr stays empty unless RUST_LOG or DL_TRACE_SUMMARY asks.
    pub fn install() -> Option<Arc<SummaryState>> {
        // `--bench` is read off argv because the subscriber must exist before
        // clap parses: a span opened earlier than the layer is a span lost.
        let want_summary = matches!(std::env::var("DL_TRACE_SUMMARY").as_deref(), Ok("1"))
            || std::env::args().any(|arg| arg == "--bench");
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
        let observability = hafley_observe::Config::from_env(
            "sprefa-extract",
            env!("CARGO_PKG_VERSION"),
            "off",
            false,
        )
        .expect("observability configuration");
        let printer = hafley_observe::format_layer(
            hafley_observe::FormatConfig {
                format: observability.format,
                ansi: observability.ansi,
                target: true,
                thread_names: false,
                span_events: tracing_subscriber::fmt::format::FmtSpan::CLOSE,
            },
            tracing_subscriber::fmt::writer::BoxMakeWriter::new(std::io::stderr),
        )
        .with_filter(filter);
        let (summary, state) = if want_summary {
            let state = Arc::new(SummaryState::new());
            let layer = SummaryLayer::new(Arc::clone(&state))
                .with_filter(EnvFilter::new("sprefa_extract=debug"));
            (Some(layer), Some(state))
        } else {
            (None, None)
        };
        Registry::default().with(printer).with(summary).init();
        hafley_observe::startup(&observability);
        state
    }
}
