//! Structured tracing (`tracing` crate). Two independent knobs:
//!
//!   - `DL_TRACE`/`RUST_LOG` (CLI-entry only): an stderr layer, off by
//!     default — set it to see spans/events live on a one-shot run.
//!   - `DL_LOG` (always on): the rolling FILE layers under
//!     `<daemon_home>/log/` — `dl.log` (DL_LOG level, default `info`) and
//!     `error.log` (always `warn`-and-up, independent of DL_LOG, so a quiet
//!     run still leaves a paper trail of what went wrong). Every `dl`
//!     process — one-shot CLI, daemon, hook — writes into the SAME two
//!     files: this is the apache-style access/error log the daemon-respawn
//!     incident was missing.
//!
//! Span CLOSE events carry durations, so the tick phases and reactivity
//! decisions surface as timed lines without recompiling or scattering
//! `eprintln!`s. Keep span levels graded: `info` for whole ticks, `debug` for
//! phases (reconcile/refresh/rebuild/gens/pulls), `trace` for per-file work.
//!
//! Tests never call `init`, so they stay silent (no subscriber => no overhead
//! beyond a relaxed-atomic load per span) UNLESS they explicitly sandbox
//! `XDG_STATE_HOME` and call `init`/exercise a code path that does.

use std::path::{Path, PathBuf};

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
    let registry = tracing_subscriber::registry()
        .with(dl_log_layer(&home))
        .with(error_log_layer(&home));
    if std::env::var_os("RUST_LOG").is_none() && std::env::var_os("DL_TRACE").is_none() {
        // No live-stderr knob requested: file layers only, cheap and silent.
        let _ = registry.try_init();
        return;
    }
    // DL_TRACE seeds the filter when RUST_LOG is unset, so the project keeps its
    // own knob without squatting on RUST_LOG for deps.
    let filter = EnvFilter::try_from_env("RUST_LOG")
        .or_else(|_| EnvFilter::try_new(&std::env::var("DL_TRACE").unwrap_or_else(|_| "off".into())))
        .unwrap_or_else(|_| EnvFilter::new("off"));
    let stderr_layer = fmt::layer()
        .with_target(false)
        .with_span_events(fmt::format::FmtSpan::CLOSE)
        .compact()
        .with_filter(filter);
    let _ = registry.with(stderr_layer).try_init();
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
    let writer = RollingWriter { dir: home.join("log"), name: "dl.log", cap_bytes: ROTATE_BYTES };
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
    let writer = RollingWriter { dir: home.join("log"), name: "error.log", cap_bytes: ROTATE_BYTES };
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
        RollingGuard { inner: self.clone(), buf: Vec::new() }
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
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
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
        let w = RollingWriter { dir: dir.clone(), name: "t.log", cap_bytes: 10 };

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
        assert!(dir.join("t.log.1").exists(), "prior content rotated to t.log.1");
        let rotated = std::fs::read_to_string(dir.join("t.log.1")).unwrap();
        assert_eq!(rotated, "hello world\n");
        let current = std::fs::read_to_string(dir.join("t.log")).unwrap();
        assert_eq!(current, "second line\n");
    }
}
