// @comment-ok: the module's reader contract, the one doc site for the three
// measurement doors and the label stack that joins spans to seam cost.
// Every statement is attributed to the (verb, relation) the IR names for it;
// nothing here parses SQL text to decide what a statement was for.
//
//   RUST_LOG=sprefa_engine_rs=debug   spans per tick / phase / verb / round
//   RUST_LOG=sprefa_engine_rs=trace   adds one span per statement
//   DL_TRACE_SUMMARY=1                one table at the end of the fold
//
// A Scope pushes its label, the seam reads the innermost one, so a statement's
// cost lands on the verb that asked for it however deep the call sits.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Label {
    pub verb: &'static str,
    pub relation: Arc<str>,
}

const UNLABELLED: &str = "-";

/// A scope's own time is its elapsed minus what its children took, so a table
/// row is exclusive and the rows sum to the fold.
struct Frame {
    label: Label,
    child_nanos: u64,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

pub fn current_label() -> Label {
    FRAMES.with(|frames| {
        frames
            .borrow()
            .last()
            .map(|frame| frame.label.clone())
            .unwrap_or_else(|| Label {
                verb: "unlabelled",
                relation: Arc::from(UNLABELLED),
            })
    })
}

#[derive(Default, Clone, Copy)]
pub struct Stat {
    pub calls: u64,
    pub rows_out: u64,
    pub rows_changed: u64,
    /// Time inside the seam.
    pub nanos: u64,
    /// Exclusive wall of the scopes carrying this label; the difference is the
    /// Rust work between the statements.
    pub wall_nanos: u64,
    pub scopes: u64,
}

static SUMMARY: OnceLock<Mutex<BTreeMap<Label, Stat>>> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

fn summary() -> &'static Mutex<BTreeMap<Label, Stat>> {
    SUMMARY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Armed before the first fold by a runner whose program reads its own cost, so
/// the table records without the caller having to spell the env var.
static FORCED: AtomicBool = AtomicBool::new(false);

pub fn force_summary() {
    FORCED.store(true, Ordering::Relaxed);
}

/// One env read per process; a fold that does not ask pays a bool load per
/// statement. `force_summary` only counts before the first record.
pub fn summary_wanted() -> bool {
    static WANTED: OnceLock<bool> = OnceLock::new();
    *WANTED.get_or_init(|| {
        std::env::var_os("DL_TRACE_SUMMARY").is_some() || FORCED.load(Ordering::Relaxed)
    })
}

/// The printed table is the env var's door alone: a program reading `tick_cost`
/// relationally must not also dump a block of text to stderr.
fn printing_wanted() -> bool {
    summary_wanted() && std::env::var_os("DL_TRACE_SUMMARY").is_some()
}

/// Pins the wall clock the Rust remainder is measured against; the driver arms
/// it before DDL so the table covers the whole fold.
pub fn arm() {
    let _ = STARTED.get_or_init(Instant::now);
}

pub fn record(label: Label, nanos: u64, rows_out: u64, rows_changed: u64) {
    if !summary_wanted() {
        return;
    }
    let mut table = summary().lock().expect("trace summary");
    let stat = table.entry(label).or_default();
    stat.calls += 1;
    stat.rows_out += rows_out;
    stat.rows_changed += rows_changed;
    stat.nanos += nanos;
}

fn record_scope(label: Label, exclusive_nanos: u64) {
    if !summary_wanted() {
        return;
    }
    let mut table = summary().lock().expect("trace summary");
    let stat = table.entry(label).or_default();
    stat.wall_nanos += exclusive_nanos;
    stat.scopes += 1;
}

/// Test door: the rows the fold attributed, sorted by cost like the printed
/// table.
pub fn summary_rows() -> Vec<(Label, Stat)> {
    let table = summary().lock().expect("trace summary");
    let mut rows: Vec<(Label, Stat)> = table.iter().map(|(k, v)| (k.clone(), *v)).collect();
    rows.sort_by_key(|(_, stat)| std::cmp::Reverse(stat.nanos));
    rows
}

pub fn reset() {
    summary().lock().expect("trace summary").clear();
}

/// The one line that answers "SQLite or Rust": everything the seam spent,
/// against the wall clock the driver armed.
pub fn report() {
    if !printing_wanted() {
        return;
    }
    let rows = summary_rows();
    let sqlite_nanos: u64 = rows.iter().map(|(_, stat)| stat.nanos).sum();
    let scoped_nanos: u64 = rows.iter().map(|(_, stat)| stat.wall_nanos).sum();
    let wall_nanos = STARTED
        .get()
        .map(|start| start.elapsed().as_nanos() as u64)
        .unwrap_or(sqlite_nanos);
    let mut out = String::new();
    out.push_str("\n== DL_TRACE_SUMMARY ==\n");
    out.push_str(&format!(
        "{:>10} {:>10} {:>10} {:>6} {:>8} {:>9} {:>9}  {:<16} {}\n",
        "us_total", "us_sql", "us_rust", "pct", "calls", "rows_out", "changed", "verb", "relation"
    ));
    for (label, stat) in &rows {
        let total = stat.wall_nanos.max(stat.nanos);
        out.push_str(&format!(
            "{:>10} {:>10} {:>10} {:>5.1}% {:>8} {:>9} {:>9}  {:<16} {}\n",
            total / 1_000,
            stat.nanos / 1_000,
            total.saturating_sub(stat.nanos) / 1_000,
            percent(total, wall_nanos),
            stat.calls,
            stat.rows_out,
            stat.rows_changed,
            label.verb,
            label.relation
        ));
    }
    let unscoped = wall_nanos.saturating_sub(scoped_nanos.max(sqlite_nanos));
    for (name, value) in [
        ("TOTAL sqlite", sqlite_nanos),
        (
            "TOTAL rust in scopes",
            scoped_nanos.saturating_sub(sqlite_nanos),
        ),
        ("TOTAL rust unscoped", unscoped),
        ("TOTAL wall", wall_nanos),
    ] {
        out.push_str(&format!(
            "{:>10} {:>10} {:>10} {:>5.1}% {:>8} {:>9} {:>9}  {:<16} {}\n",
            value / 1_000,
            "",
            "",
            percent(value, wall_nanos),
            "",
            "",
            "",
            name,
            ""
        ));
    }
    // @eprintln-ok: the summary is one block of text, and it has to survive a
    // run whose RUST_LOG is off; routing it through the fmt layer would prefix
    // every row with a timestamp and lose the table.
    use std::io::Write;
    let _ = std::io::stderr().write_all(out.as_bytes());
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 * 100.0 / whole as f64
}

/// A span plus the label the seam attributes statements to while it lives.
/// `rows` and `us` land on the span at exit, where the size is known.
pub struct Scope {
    span: tracing::span::EnteredSpan,
    started: Instant,
    rows: u64,
}

impl Scope {
    pub fn verb(verb: &'static str, relation: &str, strategy: &'static str) -> Scope {
        let span = tracing::debug_span!(
            "verb",
            verb,
            relation,
            strategy,
            rows = tracing::field::Empty,
            us = tracing::field::Empty
        );
        Scope::open(
            span,
            Label {
                verb,
                relation: Arc::from(relation),
            },
        )
    }

    /// Not one of the eight verbs (`stage`/`arrive`/`clear`/`read_staged`/
    /// `edge_lookup`/`edge_write`/`snapshot`/`recompute`): interning, decoding.
    pub fn phase(name: &'static str) -> Scope {
        let span = tracing::debug_span!(
            "phase",
            phase = name,
            rows = tracing::field::Empty,
            us = tracing::field::Empty
        );
        Scope::open(
            span,
            Label {
                verb: name,
                relation: Arc::from(UNLABELLED),
            },
        )
    }

    fn open(span: tracing::Span, label: Label) -> Scope {
        FRAMES.with(|frames| {
            frames.borrow_mut().push(Frame {
                label,
                child_nanos: 0,
            })
        });
        Scope {
            span: span.entered(),
            started: Instant::now(),
            rows: 0,
        }
    }

    pub fn rows(&mut self, rows: usize) {
        self.rows += rows as u64;
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed().as_nanos() as u64;
        self.span.record("rows", self.rows);
        self.span.record("us", elapsed / 1_000);
        let frame = FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            let frame = frames.pop();
            if let Some(parent) = frames.last_mut() {
                parent.child_nanos += elapsed;
            }
            frame
        });
        if let Some(frame) = frame {
            record_scope(frame.label, elapsed.saturating_sub(frame.child_nanos));
        }
    }
}

/// One fixpoint round; `delta_rows` is the count that decides whether another
/// round runs.
pub fn round_span(round: u64) -> tracing::Span {
    tracing::debug_span!("round", round, delta_rows = tracing::field::Empty)
}
