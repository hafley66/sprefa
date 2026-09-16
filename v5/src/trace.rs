//! Structured tracing (`tracing` crate). Three independent knobs:
//!
//!   - `DL_TRACE`/`RUST_LOG` (CLI-entry only): an stderr layer for extra
//!     verbosity, off by default — set it to see debug/trace spans/events live
//!     on a one-shot run. `warn` and `error` events ALWAYS go to stderr so
//!     user-visible failures are never silently swallowed.
//!   - `DL_LOG` (always on): the rolling FILE layers under
//!     `<daemon_home>/log/` — `dl.log` (DL_LOG level, default `info`) and
//!     `error.log` (always `warn`-and-up, independent of DL_LOG, so a quiet
//!     run still leaves a paper trail of what went wrong). Every `dl`
//!     process — one-shot CLI, daemon, hook — writes into the SAME two
//!     files: this is the apache-style access/error log the daemon-respawn
//!     incident was missing.
//!   - `DL_TRACE_CHROME=<path>` (off by default, one path = one export): a
//!     `tracing-chrome` layer that turns every SPAN (tick / phase / job) into
//!     a chrome-trace-format JSON event, loadable at ui.perfetto.dev as a
//!     zoomable timeline. Absent, the layer is never constructed — zero
//!     overhead beyond the `std::env::var_os` check in `chrome_layer`. See
//!     `docs/tracing-chrome.md` for the repro command and kill-safety notes.
//!
//! Span CLOSE events carry durations, so the tick phases and reactivity
//! decisions surface as timed lines without recompiling or scattering ad-hoc
//! writes to stderr. Keep span levels graded: `info` for whole ticks, `debug` for
//! phases (reconcile/refresh/rebuild/gens/pulls), `trace` for per-file work.
//!
//! Tests never call `init`, so they stay silent (no subscriber => no overhead
//! beyond a relaxed-atomic load per span) UNLESS they explicitly sandbox
//! `XDG_STATE_HOME` and call `init`/exercise a code path that does.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Rotate a log file at ~4MB, one generation kept (`<name>.1`) — same budget
/// `why.jsonl` uses (`src/why.rs::ROTATE_BYTES`), chosen for the same reason:
/// small enough that a runaway process never fills a disk, big enough to hold
/// a real incident's worth of lines.
const ROTATE_BYTES: u64 = 4 * 1024 * 1024;

/// `dl daemon serve` / `dl daemon start --foreground` install their OWN
/// combined stderr+file subscriber (`daemon::init_daemon_tracing`, which
/// composes `file_layers` with an always-on stderr layer) and must win the
/// process-global `try_init` race. Every OTHER invocation — including
/// `dl daemon start` (detaches and returns), `dl daemon why`, a plain
/// `dl prog.dl` — calls this instead. `main` -> `cli::run` computes this from
/// argv before either init can run, so there is exactly one caller per
/// process for whichever side wins.
pub fn init(is_daemon_foreground: bool) {
    if is_daemon_foreground {
        return;
    }
    let home = crate::daemon::daemon_home();
    let observability = hafley_observe::Config::from_env(
        "sprefa",
        env!("CARGO_PKG_VERSION"),
        "off",
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
    )
    .expect("observability configuration");
    // Warn/error always reach stderr so tracing-converted anomalies remain
    // user-visible even without DL_TRACE/RUST_LOG.
    let stderr_warn_layer = hafley_observe::format_layer(
        hafley_observe::FormatConfig {
            format: observability.format,
            ansi: observability.ansi,
            target: false,
            thread_names: false,
            span_events: fmt::format::FmtSpan::CLOSE,
        },
        fmt::writer::BoxMakeWriter::new(std::io::stderr),
    )
    .with_filter(tracing::level_filters::LevelFilter::WARN);
    let registry = tracing_subscriber::registry()
        .with(dl_log_layer(&home))
        .with(error_log_layer(&home))
        .with(stderr_warn_layer);
    if std::env::var_os("RUST_LOG").is_none() && std::env::var_os("DL_TRACE").is_none() {
        // No extra verbosity requested: file layers + stderr warn/error only.
        let _ = registry.with(chrome_layer()).try_init();
        hafley_observe::startup(&observability);
        return;
    }
    // DL_TRACE seeds the filter when RUST_LOG is unset, so the project keeps its
    // own knob without squatting on RUST_LOG for deps.
    let filter = EnvFilter::try_from_env("RUST_LOG")
        .or_else(|_| {
            EnvFilter::try_new(&std::env::var("DL_TRACE").unwrap_or_else(|_| "off".into()))
        })
        .unwrap_or_else(|_| EnvFilter::new("off"));
    let stderr_layer = hafley_observe::format_layer(
        hafley_observe::FormatConfig {
            format: observability.format,
            ansi: observability.ansi,
            target: false,
            thread_names: false,
            span_events: fmt::format::FmtSpan::CLOSE,
        },
        fmt::writer::BoxMakeWriter::new(std::io::stderr),
    )
    .with_filter(filter);
    let _ = registry.with(stderr_layer).with(chrome_layer()).try_init();
    hafley_observe::startup(&observability);
}

/// The chrome-trace layer's `FlushGuard`, stashed process-globally rather than
/// returned up through `init`'s call chain. Reason: `std::process::exit` (used
/// at several one-shot CLI exit points, see `cli::run`) skips `Drop` entirely,
/// so a guard held only as a local variable in `init`'s caller would silently
/// never flush/close on those paths. `finish_chrome_trace`/`flush_chrome_trace`
/// let any call site reach it explicitly instead — the same "explicit, not
/// RAII" reasoning `invlog::record_end`'s doc comment already states for the
/// identical `process::exit`-skips-Drop hazard.
static CHROME_GUARD: OnceLock<Mutex<Option<tracing_chrome::FlushGuard>>> = OnceLock::new();

fn chrome_guard_slot() -> &'static Mutex<Option<tracing_chrome::FlushGuard>> {
    CHROME_GUARD.get_or_init(|| Mutex::new(None))
}

/// Build the optional `tracing-chrome` layer from `DL_TRACE_CHROME=<path>`.
/// `None` when the var is unset — `.with(None::<L>)` is a documented no-op in
/// `tracing_subscriber` (`Layer` is implemented for `Option<L>`), so the
/// layer is never constructed and costs nothing beyond this one `var_os`
/// check on every other invocation. Called at most once per process (each of
/// `init`'s two mutually-exclusive `try_init` branches ends in a `return`, so
/// only one call actually executes at runtime even though both are
/// monomorphized at compile time — same reasoning as `dl_log_layer`/
/// `error_log_layer` above for why this isn't a single generic call site).
///
/// `include_args(true)`: field values (tick number, root, phase name, job
/// key/kind) land in the chrome trace's `args` object, which is what makes
/// the perfetto view identify WHICH tick/phase/job a slice is, not just that
/// a same-named slice happened.
pub(crate) fn chrome_layer<S>() -> Option<impl tracing_subscriber::Layer<S> + Send + Sync + 'static>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    let path = std::env::var_os("DL_TRACE_CHROME")?;
    let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(PathBuf::from(path))
        .include_args(true)
        .build();
    *chrome_guard_slot()
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(guard);
    Some(layer)
}

/// Explicit finalize: signal the chrome-trace writer thread to stop, join it,
/// and write the closing `]` — the only thing that makes the file strict-JSON
/// (`jq`-parseable). MUST be called before every `std::process::exit` in the
/// one-shot CLI path (see `cli::run`'s exit sites) since `process::exit` skips
/// `Drop`; the crate's own README example relies on scope-exit `Drop`, which
/// does not fire there. No-op (idempotent) when no chrome layer was installed,
/// or when this was already called once. A SIGKILL still loses this call
/// entirely — see `docs/tracing-chrome.md` for exactly what that costs.
pub fn finish_chrome_trace() {
    let mut slot = chrome_guard_slot()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    drop(slot.take());
}

/// Best-effort mid-run flush: pushes the chrome layer's internal `BufWriter`
/// to disk WITHOUT writing the closing `]` (so the file is still open for more
/// events after this call — unlike `finish_chrome_trace`). Called at tick
/// boundaries (`activity::end_tick`) so a long-lived daemon killed between
/// ticks loses at most the in-flight tick's spans, not the whole run since
/// process start. No-op when no chrome layer is installed.
pub fn flush_chrome_trace() {
    let slot = chrome_guard_slot()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some(guard) = slot.as_ref() {
        guard.flush();
    }
}

/// The two rolling file layers every `dl` process shares:
/// `<home>/log/dl.log` filtered by `DL_LOG` (default `info`) and
/// `<home>/log/error.log` filtered `warn`-and-up unconditionally. Two
/// SEPARATE generic functions (not one returning a tuple) because
/// `.with(a).with(b)` requires `b: Layer<Layered<A, S>>`, not `Layer<S>` — a
/// single generic call site shares one `S` across both return values, which
/// cannot satisfy that. Each function call below is its own monomorphization,
/// independently inferring `S` from where it lands in the `.with()` chain, so
/// both `trace::init` (registry with no other layers yet) and
/// `daemon::init_daemon_tracing` (registry that already carries a stderr
/// layer) can compose these onto their own stack.
pub(crate) fn dl_log_layer<S>(
    home: &Path,
) -> impl tracing_subscriber::Layer<S> + Send + Sync + 'static
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let writer = RollingWriter {
        dir: home.join("log"),
        name: "dl.log",
        cap_bytes: ROTATE_BYTES,
    };
    let filter = EnvFilter::try_from_env("DL_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    fmt::layer::<S>()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .with_thread_names(true)
        .compact()
        .with_filter(filter)
}

pub(crate) fn error_log_layer<S>(
    home: &Path,
) -> impl tracing_subscriber::Layer<S> + Send + Sync + 'static
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let writer = RollingWriter {
        dir: home.join("log"),
        name: "error.log",
        cap_bytes: ROTATE_BYTES,
    };
    fmt::layer::<S>()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(false)
        .with_thread_names(true)
        .compact()
        .with_filter(tracing::level_filters::LevelFilter::WARN)
}

/// A `MakeWriter` that appends ONE already-fully-formatted event per
/// `write_all` call to `<dir>/<name>`, open/append/close every time — the
/// same "one write_all, one open/write/close" idiom `why.rs::append_line`
/// uses and for the same reason: several `dl` processes (a daemon plus any
/// number of one-shot clients) can be appending to this SAME file
/// concurrently, and only a single `write()` syscall per line is safe against
/// interleaving two processes' partial writes into corrupt output. Rotates to
/// `<name>.1` (one generation) when the file crosses `cap_bytes`.
///
/// `make_writer()` is called once per tracing event; the returned
/// `RollingGuard` BUFFERS every `Write::write` call tracing_subscriber makes
/// while formatting that one event (timestamp, level, fields, message can
/// each be a separate small `write()` call) and performs the actual
/// rotate-check + open + `write_all` + close exactly once, in `Drop`, when
/// the event's line is complete. Without this buffering step a single event
/// could touch the file several times, reintroducing the interleave hazard
/// the single-`write_all` design is meant to close.
#[derive(Clone)]
struct RollingWriter {
    dir: PathBuf,
    name: &'static str,
    cap_bytes: u64,
}

impl<'a> fmt::MakeWriter<'a> for RollingWriter {
    type Writer = RollingGuard;
    fn make_writer(&'a self) -> Self::Writer {
        RollingGuard {
            inner: self.clone(),
            buf: Vec::new(),
        }
    }
}

struct RollingGuard {
    inner: RollingWriter,
    buf: Vec<u8>,
}

impl std::io::Write for RollingGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for RollingGuard {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let _ = std::fs::create_dir_all(&self.inner.dir);
        let path = self.inner.dir.join(self.inner.name);
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > self.inner.cap_bytes {
                let rotated = self.inner.dir.join(format!("{}.1", self.inner.name));
                let _ = std::fs::rename(&path, rotated);
            }
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = f.write_all(&self.buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `RollingWriter`'s own contract in isolation, without going through
    /// a tracing subscriber: one `make_writer()`/several `write()`s/drop
    /// produces exactly one line in the file, and crossing `cap_bytes`
    /// rotates the prior content to `<name>.1`.
    #[test]
    fn rolling_writer_buffers_to_one_write_and_rotates() {
        let dir = std::env::temp_dir().join(format!("dl_trace_rw_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = RollingWriter {
            dir: dir.clone(),
            name: "t.log",
            cap_bytes: 10,
        };

        {
            use std::io::Write as _;
            let mut g = fmt::MakeWriter::make_writer(&w);
            g.write_all(b"hello ").unwrap();
            g.write_all(b"world\n").unwrap();
        } // Drop flushes the buffered line in one write_all.
        let content = std::fs::read_to_string(dir.join("t.log")).unwrap();
        assert_eq!(content, "hello world\n");

        // A second line pushes total size past cap_bytes (10) -> rotate.
        {
            use std::io::Write as _;
            let mut g = fmt::MakeWriter::make_writer(&w);
            g.write_all(b"second line\n").unwrap();
        }
        assert!(
            dir.join("t.log.1").exists(),
            "prior content rotated to t.log.1"
        );
        let rotated = std::fs::read_to_string(dir.join("t.log.1")).unwrap();
        assert_eq!(rotated, "hello world\n");
        let current = std::fs::read_to_string(dir.join("t.log")).unwrap();
        assert_eq!(current, "second line\n");
    }

    /// `finish_chrome_trace`/`flush_chrome_trace` are safe no-ops when no
    /// chrome layer was ever installed (`CHROME_GUARD` slot stays empty) —
    /// the common case for every test and for a plain run with
    /// `DL_TRACE_CHROME` unset. Guards against a future edit making these
    /// panic (e.g. an `.unwrap()` on the slot) instead of silently no-op'ing,
    /// which the real CLI exit-site call pattern (finalize called from many
    /// paths, some of which never activated tracing at all) depends on.
    #[test]
    fn chrome_trace_finalize_and_flush_are_no_ops_without_a_layer() {
        assert!(
            chrome_guard_slot().lock().unwrap().is_none(),
            "no DL_TRACE_CHROME layer is installed by any other lib test, so the \
             guard slot should start empty"
        );
        // Idempotent: calling twice (as several CLI exit sites can, e.g. a
        // nested `daemon serve` finalizing in `shutdown_cleanup` and then
        // `cli::run`'s own exit-site call finding it already taken) must not
        // panic either.
        finish_chrome_trace();
        finish_chrome_trace();
        flush_chrome_trace();
        assert!(
            chrome_guard_slot().lock().unwrap().is_none(),
            "finish/flush without an installed layer must leave the guard slot empty"
        );
    }
}
